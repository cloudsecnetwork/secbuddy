//! MCP client: connect to external MCP servers (stdio), list tools, and call tools.
//! SecBuddy does not run or expose an MCP server.

use crate::tool_registry::{ToolInfo, ToolRegistry};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::Duration;

const MCP_CONFIG_KEY: &str = "mcp_servers";
const PROTOCOL_VERSION: &str = "2024-11-05";

// ---- Config ----

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct McpConfig {
    #[serde(default, rename = "mcpServers")]
    pub mcp_servers: HashMap<String, McpServerEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpServerEntry {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Free-form description shown in the UI; ignored by the runtime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Per-server tools/call timeout (seconds). Overrides the global tool timeout when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    /// When true, the runtime skips this server during reload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
}

/// Path to the canonical mcp.json under the app data directory.
pub fn mcp_json_path(app_data_dir: &Path) -> std::path::PathBuf {
    app_data_dir.join("mcp.json")
}

/// Load MCP config. `mcp.json` is authoritative when present so hand edits are picked up;
/// the DB cache is used as a fallback when the file is missing or unreadable so startup
/// still works on a freshly installed copy.
pub async fn load_mcp_config(
    pool: &sqlx::SqlitePool,
    app_data_dir: &Path,
) -> Result<McpConfig, String> {
    let mcp_path = mcp_json_path(app_data_dir);
    if mcp_path.exists() {
        let content = std::fs::read_to_string(&mcp_path).map_err(|e| e.to_string())?;
        let config: McpConfig = serde_json::from_str(&content).map_err(|e| e.to_string())?;
        let cache = serde_json::to_string(&config).map_err(|e| e.to_string())?;
        let _ = crate::db::save_setting(pool, MCP_CONFIG_KEY, &cache).await;
        return Ok(config);
    }
    let from_db = crate::db::get_setting(pool, MCP_CONFIG_KEY)
        .await
        .map_err(|e| e.to_string())?;
    if let Some(json) = from_db {
        let config: McpConfig = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        return Ok(config);
    }
    Ok(McpConfig::default())
}

/// Persist MCP config: pretty-write `mcp.json` (always) and refresh the DB cache.
/// `_write_file` is retained for ABI stability; the file is always written now that
/// `mcp.json` is the source of truth.
pub async fn save_mcp_config(
    pool: &sqlx::SqlitePool,
    app_data_dir: &Path,
    config: &McpConfig,
    _write_file: bool,
) -> Result<(), String> {
    let pretty = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    let mcp_path = mcp_json_path(app_data_dir);
    if let Some(parent) = mcp_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&mcp_path, &pretty).map_err(|e| e.to_string())?;
    let cache = serde_json::to_string(config).map_err(|e| e.to_string())?;
    crate::db::save_setting(pool, MCP_CONFIG_KEY, &cache)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Read the raw mcp.json text for the in-app JSON editor. Returns a default skeleton
/// when the file does not yet exist so the editor always has something to render.
pub fn read_mcp_json_text(app_data_dir: &Path) -> Result<String, String> {
    let mcp_path = mcp_json_path(app_data_dir);
    if !mcp_path.exists() {
        return Ok("{\n  \"mcpServers\": {}\n}".to_string());
    }
    std::fs::read_to_string(&mcp_path).map_err(|e| e.to_string())
}

/// Parse and persist raw mcp.json text. Returns a serde error (with line/col) on
/// invalid input without writing anything to disk.
pub async fn save_mcp_config_text(
    pool: &sqlx::SqlitePool,
    app_data_dir: &Path,
    text: &str,
) -> Result<McpConfig, String> {
    let config: McpConfig = serde_json::from_str(text).map_err(|e| e.to_string())?;
    save_mcp_config(pool, app_data_dir, &config, true).await?;
    Ok(config)
}

use sqlx::SqlitePool;

// ---- Runtime: running server processes ----

struct StdioConnection {
    child: Child,
    request_id: AtomicU32,
    #[allow(dead_code)]
    server_name: String,
    /// Per-server timeout override (seconds). When `Some`, used in place of the
    /// caller-provided `timeout_secs` in `call_tool`.
    timeout_secs: Option<u64>,
}

/// Holds running MCP server processes and allows tools/call.
pub struct McpRuntime {
    servers: Mutex<HashMap<String, StdioConnection>>,
}

impl McpRuntime {
    pub fn new() -> Self {
        Self {
            servers: Mutex::new(HashMap::new()),
        }
    }

    /// Clear all running servers and stop their processes.
    pub fn stop_all(&self) {
        let mut guard = self.servers.lock().unwrap();
        for (_, mut conn) in guard.drain() {
            let _ = conn.child.kill();
            let _ = conn.child.wait();
        }
    }

    /// Reload: stop existing, spawn each server from config, initialize + tools/list, register tools.
    pub fn reload(
        &self,
        _pool: &SqlitePool,
        _app_data_dir: &Path,
        config: &McpConfig,
        registry: &ToolRegistry,
    ) -> Result<(), String> {
        self.stop_all();
        registry.clear_mcp_tools();

        for (server_name, entry) in &config.mcp_servers {
            if entry.command.is_empty() {
                continue;
            }
            if entry.disabled.unwrap_or(false) {
                log::info!("[mcp] Server {} is disabled, skipping", server_name);
                continue;
            }
            if let Err(e) = spawn_and_register(self, server_name, entry, registry) {
                log::error!("[mcp] Server {} failed to start: {}", server_name, e);
            }
        }
        Ok(())
    }

    /// Call a tool on a connected MCP server. Blocks on IO; run from spawn_blocking if in async context.
    pub fn call_tool(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: serde_json::Value,
        timeout_secs: u64,
    ) -> Result<String, String> {
        let mut guard = self.servers.lock().unwrap();
        let conn = guard
            .get_mut(server_name)
            .ok_or_else(|| format!("MCP server not connected: {}", server_name))?;
        let effective_timeout = conn.timeout_secs.unwrap_or(timeout_secs);
        let id = conn.request_id.fetch_add(1, Ordering::SeqCst);
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": arguments
            }
        });
        let line = serde_json::to_string(&request).map_err(|e| e.to_string())?;
        let stdin = conn.child.stdin.as_mut().ok_or("Server stdin closed")?;
        stdin
            .write_all(line.as_bytes())
            .map_err(|e| e.to_string())?;
        stdin.write_all(b"\n").map_err(|e| e.to_string())?;
        stdin.flush().map_err(|e| e.to_string())?;

        let stdout = conn.child.stdout.as_mut().ok_or("Server stdout closed")?;
        let mut reader = BufReader::new(stdout);
        let mut response_line = String::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(effective_timeout);
        loop {
            if std::time::Instant::now() > deadline {
                return Err("MCP tools/call timeout".to_string());
            }
            response_line.clear();
            let n = reader
                .read_line(&mut response_line)
                .map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("MCP server closed stream".to_string());
            }
            let response_line = response_line.trim();
            if response_line.is_empty() {
                continue;
            }
            let response: serde_json::Value =
                serde_json::from_str(response_line).map_err(|e| e.to_string())?;
            let resp_id = response.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
            if resp_id != id as u64 {
                continue;
            }
            if let Some(err) = response.get("error") {
                let msg = err
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown error");
                return Err(msg.to_string());
            }
            let result = response
                .get("result")
                .ok_or("Missing result in MCP response")?;
            let content = result
                .get("content")
                .and_then(|c| c.as_array())
                .and_then(|arr| arr.first())
                .and_then(|c| c.get("text"))
                .and_then(|t| t.as_str())
                .unwrap_or("");
            return Ok(content.to_string());
        }
    }

    /// Check if a server is currently connected.
    pub fn has_server(&self, server_name: &str) -> bool {
        self.servers.lock().unwrap().contains_key(server_name)
    }
}

impl Default for McpRuntime {
    fn default() -> Self {
        Self::new()
    }
}

/// Spawn one server process, perform initialize + tools/list, register tools, store child in runtime.
fn spawn_and_register(
    runtime: &McpRuntime,
    server_name: &str,
    entry: &McpServerEntry,
    registry: &ToolRegistry,
) -> Result<(), String> {
    let mut cmd = Command::new(&entry.command);
    cmd.args(&entry.args);
    if !entry.env.is_empty() {
        cmd.env_clear();
        for (k, v) in &entry.env {
            cmd.env(k, v);
        }
    }
    // When env is empty, child inherits parent environment (e.g. PATH for npx).
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::null());
    crate::process_ext::no_window_std(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let mut stdin = child.stdin.take().ok_or("Failed to take stdin")?;
    let stdout = child.stdout.take().ok_or("Failed to take stdout")?;

    let init_request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": { "name": "secbuddy", "version": "0.1.0" }
        }
    });
    writeln!(stdin, "{}", init_request).map_err(|e| e.to_string())?;
    stdin.flush().map_err(|e| e.to_string())?;

    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|e| e.to_string())?;
    let init_response: serde_json::Value =
        serde_json::from_str(line.trim()).map_err(|e| e.to_string())?;
    if init_response.get("error").is_some() {
        let _ = child.kill();
        let msg = init_response
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|v| v.as_str())
            .unwrap_or("Initialize failed");
        return Err(msg.to_string());
    }

    let initialized = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    writeln!(stdin, "{}", initialized).map_err(|e| e.to_string())?;
    stdin.flush().map_err(|e| e.to_string())?;

    let list_request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    });
    writeln!(stdin, "{}", list_request).map_err(|e| e.to_string())?;
    stdin.flush().map_err(|e| e.to_string())?;

    line.clear();
    reader.read_line(&mut line).map_err(|e| e.to_string())?;
    let list_response: serde_json::Value =
        serde_json::from_str(line.trim()).map_err(|e| e.to_string())?;
    if list_response.get("error").is_some() {
        let _ = child.kill();
        let msg = list_response
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|v| v.as_str())
            .unwrap_or("tools/list failed");
        return Err(msg.to_string());
    }

    let tools_array = list_response
        .get("result")
        .and_then(|r| r.get("tools"))
        .and_then(|t| t.as_array())
        .ok_or("Invalid tools/list response")?;

    child.stdin = Some(stdin);
    child.stdout = Some(reader.into_inner());

    for t in tools_array {
        let name = t
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .to_string();
        if name.is_empty() {
            continue;
        }
        let description = t
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap_or("")
            .to_string();
        let input_schema = t.get("inputSchema").cloned().unwrap_or_else(|| {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "args": { "type": "string" },
                    "target": { "type": "string" }
                },
                "required": ["args", "target"]
            })
        });
        let info = ToolInfo {
            name: name.clone(),
            title: t.get("title").and_then(|v| v.as_str()).map(String::from),
            description: description.clone(),
            available: true,
            detected_path: None,
            source: "mcp".to_string(),
            server_name: Some(server_name.to_string()),
            risk_category: "active".to_string(),
            input_schema: Some(input_schema),
            category: None,
            output_schema: t.get("outputSchema").cloned(),
            alternatives: None,
            returns: None,
        };
        registry.register_mcp_tool(info);
    }

    let mut guard = runtime.servers.lock().unwrap();
    guard.insert(
        server_name.to_string(),
        StdioConnection {
            child,
            request_id: AtomicU32::new(3),
            server_name: server_name.to_string(),
            timeout_secs: entry.timeout,
        },
    );

    Ok(())
}

/// Load config, then reload servers. Called at startup and when user saves MCP config.
pub async fn load_and_reload(
    pool: &SqlitePool,
    app_data_dir: &Path,
    registry: &ToolRegistry,
    runtime: &McpRuntime,
) -> Result<(), String> {
    let config = load_mcp_config(pool, app_data_dir).await?;
    runtime.reload(pool, app_data_dir, &config, registry)
}

/// Test one server config: spawn, initialize + tools/list, return tool count, then kill process.
pub fn test_mcp_server(entry: &McpServerEntry) -> Result<u32, String> {
    let mut cmd = Command::new(&entry.command);
    cmd.args(&entry.args);
    if !entry.env.is_empty() {
        cmd.env_clear();
        for (k, v) in &entry.env {
            cmd.env(k, v);
        }
    }
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::null());
    crate::process_ext::no_window_std(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let mut stdin = child.stdin.take().ok_or("Failed to take stdin")?;
    let stdout = child.stdout.take().ok_or("Failed to take stdout")?;

    let init_request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": { "name": "secbuddy", "version": "0.1.0" }
        }
    });
    writeln!(stdin, "{}", init_request).map_err(|e| e.to_string())?;
    stdin.flush().map_err(|e| e.to_string())?;

    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|e| e.to_string())?;
    let init_response: serde_json::Value =
        serde_json::from_str(line.trim()).map_err(|e| e.to_string())?;
    if init_response.get("error").is_some() {
        let _ = child.kill();
        let msg = init_response
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|v| v.as_str())
            .unwrap_or("Initialize failed");
        return Err(msg.to_string());
    }

    let initialized = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    writeln!(stdin, "{}", initialized).map_err(|e| e.to_string())?;
    stdin.flush().map_err(|e| e.to_string())?;

    let list_request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    });
    writeln!(stdin, "{}", list_request).map_err(|e| e.to_string())?;
    stdin.flush().map_err(|e| e.to_string())?;

    line.clear();
    reader.read_line(&mut line).map_err(|e| e.to_string())?;
    let _ = child.kill();
    let _ = child.wait();

    let list_response: serde_json::Value =
        serde_json::from_str(line.trim()).map_err(|e| e.to_string())?;
    if list_response.get("error").is_some() {
        let msg = list_response
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|v| v.as_str())
            .unwrap_or("tools/list failed");
        return Err(msg.to_string());
    }
    let count = list_response
        .get("result")
        .and_then(|r| r.get("tools"))
        .and_then(|t| t.as_array())
        .map(|a| a.len() as u32)
        .unwrap_or(0);
    Ok(count)
}
