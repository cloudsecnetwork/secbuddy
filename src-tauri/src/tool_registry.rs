//! Tool registry: load tools.json, detect local binaries (where/which), expose list for LLM and IPC.
//! Tool definitions follow the MCP tool schema: https://modelcontextprotocol.io/specification/2025-06-18/server/tools
//! Local tools require name, description, inputSchema; MCP-registered tools use the same shape.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;
use std::sync::RwLock;

/// Fallback input schema for MCP-registered tools that omit inputSchema (args + target).
fn mcp_fallback_input_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "args": { "type": "string", "description": "CLI or tool arguments" },
            "target": { "type": "string", "description": "Primary target (host, IP, URL)" }
        },
        "required": ["args", "target"]
    })
}

#[derive(Clone, Debug, Serialize)]
pub struct ToolInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub description: String,
    pub available: bool,
    pub detected_path: Option<String>,
    pub source: String, // "local" | "mcp"
    pub server_name: Option<String>,
    pub risk_category: String, // "passive" | "active" | "high_impact"
    /// MCP-style input schema (JSON Schema). Present when tool follows MCP tool spec.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<serde_json::Value>,
    /// Tool group for prompt and UI (e.g. network, recon, web, tls).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Optional MCP outputSchema. When present, describes result shape.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
    /// Alternative tool names if this tool fails or is unavailable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alternatives: Option<Vec<String>>,
    /// Optional description of what the tool returns (MCP-style documentation).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub returns: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ToolDef {
    /// Unique identifier (MCP: name)
    pub name: String,
    /// Optional human-readable title (MCP: title)
    #[serde(default)]
    pub title: Option<String>,
    /// Human-readable description (MCP: description). Required.
    pub description: String,
    /// JSON Schema for parameters (MCP: inputSchema). Required. Use "inputSchema" in JSON for MCP alignment.
    #[serde(alias = "inputSchema")]
    pub input_schema: serde_json::Value,
    /// Secproof: binary name for local execution (not part of MCP spec)
    pub binary: String,
    #[serde(default = "default_risk_category")]
    pub risk_category: String,
    /// Optional category for grouping (e.g. network, recon, web, tls).
    #[serde(default)]
    pub category: Option<String>,
    /// Optional MCP outputSchema for result shape. Not used for execution.
    #[serde(default, alias = "outputSchema")]
    pub output_schema: Option<serde_json::Value>,
    /// Alternative tool names if this tool fails or is unavailable.
    #[serde(default)]
    pub alternatives: Option<Vec<String>>,
    /// Optional description of what the tool returns (e.g. "Stdout/stderr and exit code").
    #[serde(default)]
    pub returns: Option<String>,
}

fn default_risk_category() -> String {
    "active".to_string()
}

struct LocalToolEntry {
    def: ToolDef,
    detected_path: Option<String>,
    available: bool,
}

pub struct ToolRegistry {
    local_tools: RwLock<Vec<LocalToolEntry>>,
    mcp_tools: RwLock<Vec<ToolInfo>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            local_tools: RwLock::new(Vec::new()),
            mcp_tools: RwLock::new(Vec::new()),
        }
    }

    /// Load tools from JSON string (e.g. embedded include_str!) and detect local binaries.
    pub fn load_local_tools_from_str(&self, content: &str) -> Result<(), String> {
        let defs: Vec<ToolDef> = serde_json::from_str(content).map_err(|e| e.to_string())?;
        let mut entries = Vec::with_capacity(defs.len());
        for def in defs {
            let (detected_path, available) = Self::detect_binary(&def.binary);
            entries.push(LocalToolEntry {
                def,
                detected_path,
                available,
            });
        }
        *self.local_tools.write().unwrap() = entries;
        Ok(())
    }

    /// Load tools.json from the given path and detect local binaries.
    pub fn load_local_tools(&self, tools_json_path: &std::path::Path) -> Result<(), String> {
        let content = std::fs::read_to_string(tools_json_path).map_err(|e| e.to_string())?;
        self.load_local_tools_from_str(&content)
    }

    /// Detect binary path: Windows = where, macOS/Linux = which.
    fn detect_binary(binary: &str) -> (Option<String>, bool) {
        let output = if cfg!(target_os = "windows") {
            Command::new("where")
                .arg(binary)
                .output()
        } else {
            Command::new("which")
                .arg(binary)
                .output()
        };
        match output {
            Ok(o) if o.status.success() => {
                let path = String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .next()
                    .map(|s| s.trim().to_string());
                (path.clone(), path.is_some())
            }
            _ => (None, false),
        }
    }

    /// Refresh detection for all local tools (e.g. when Settings is opened).
    pub fn refresh_detection(&self, tools_json_path: &std::path::Path) -> Result<(), String> {
        self.load_local_tools(tools_json_path)
    }

    /// Re-run detection using the bundled tools.json (no path needed). Use when PATH may have changed.
    pub fn refresh_detection_embedded(&self) -> Result<(), String> {
        self.load_local_tools_from_str(include_str!("../tools.json"))
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    /// Resolve binary path for a local tool. Returns None if unavailable or not found.
    pub fn resolve_local_path(&self, tool_name: &str) -> Option<PathBuf> {
        let guard = self.local_tools.read().ok()?;
        for entry in guard.iter() {
            if entry.def.name == tool_name && entry.available {
                return entry.detected_path.as_deref().map(PathBuf::from);
            }
        }
        None
    }

    /// Get risk_category for a tool by name (local or MCP). MCP unknowns default to "active".
    pub fn risk_category(&self, tool_name: &str) -> String {
        if let Ok(guard) = self.local_tools.read() {
            for entry in guard.iter() {
                if entry.def.name == tool_name {
                    return entry.def.risk_category.clone();
                }
            }
        }
        if let Ok(guard) = self.mcp_tools.read() {
            for t in guard.iter() {
                if t.name == tool_name {
                    return t.risk_category.clone();
                }
            }
        }
        "active".to_string()
    }

    /// Get category for a tool by name (local or MCP). Used for phase label and prompt.
    pub fn get_category(&self, tool_name: &str) -> Option<String> {
        if let Ok(guard) = self.local_tools.read() {
            for entry in guard.iter() {
                if entry.def.name == tool_name {
                    return entry.def.category.clone();
                }
            }
        }
        if let Ok(guard) = self.mcp_tools.read() {
            for t in guard.iter() {
                if t.name == tool_name {
                    return t.category.clone();
                }
            }
        }
        None
    }

    /// Whether the tool is from MCP (then we call MCP server, not subprocess).
    pub fn is_mcp_tool(&self, tool_name: &str) -> bool {
        let guard = self.mcp_tools.read().ok();
        guard
            .map(|g| g.iter().any(|t| t.name == tool_name))
            .unwrap_or(false)
    }

    /// Server name for an MCP tool (None if local or unknown).
    pub fn get_mcp_server_name(&self, tool_name: &str) -> Option<String> {
        let guard = self.mcp_tools.read().ok()?;
        guard
            .iter()
            .find(|t| t.name == tool_name)
            .and_then(|t| t.server_name.clone())
    }

    /// List all tools for IPC (sidebar, settings): local + MCP with availability. MCP tool shape.
    pub fn list_tools(&self) -> Vec<ToolInfo> {
        let mut out = Vec::new();
        if let Ok(guard) = self.local_tools.read() {
            for entry in guard.iter() {
                out.push(ToolInfo {
                    name: entry.def.name.clone(),
                    title: entry.def.title.clone(),
                    description: entry.def.description.clone(),
                    available: entry.available,
                    detected_path: entry.detected_path.clone(),
                    source: "local".to_string(),
                    server_name: None,
                    risk_category: entry.def.risk_category.clone(),
                    input_schema: Some(entry.def.input_schema.clone()),
                    category: entry.def.category.clone(),
                    output_schema: entry.def.output_schema.clone(),
                    alternatives: entry.def.alternatives.clone(),
                    returns: entry.def.returns.clone(),
                });
            }
        }
        if let Ok(guard) = self.mcp_tools.read() {
            out.extend(guard.clone());
        }
        out
    }

    /// List only available tools in OpenAI/Claude function-calling format (for LLM). MCP inputSchema → parameters.
    pub fn list_available_for_llm(&self) -> Vec<serde_json::Value> {
        let mut tools = Vec::new();
        if let Ok(guard) = self.local_tools.read() {
            for entry in guard.iter() {
                if !entry.available {
                    continue;
                }
                tools.push(serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": entry.def.name,
                        "description": entry.def.description,
                        "parameters": entry.def.input_schema
                    }
                }));
            }
        }
        if let Ok(guard) = self.mcp_tools.read() {
            for t in guard.iter() {
                let parameters = t
                    .input_schema
                    .clone()
                    .unwrap_or_else(mcp_fallback_input_schema);
                tools.push(serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": parameters
                    }
                }));
            }
        }
        tools
    }

    /// Register an MCP tool (called from MCP client when server reports tools/list).
    pub fn register_mcp_tool(&self, info: ToolInfo) {
        self.mcp_tools.write().unwrap().push(info);
    }

    /// Clear MCP tools (e.g. on reconnect).
    pub fn clear_mcp_tools(&self) {
        self.mcp_tools.write().unwrap().clear();
    }

    /// Return all available tools in MCP tools/list response shape for easier migration to MCP.
    /// See: https://modelcontextprotocol.io/specification/2025-06-18/server/tools
    pub fn list_tools_mcp_format(&self) -> Vec<serde_json::Value> {
        let mut out = Vec::new();
        for t in self.list_tools() {
            if !t.available && t.source == "local" {
                continue;
            }
            let mut tool = serde_json::json!({
                "name": t.name,
                "description": t.description,
            });
            if let Some(title) = &t.title {
                tool["title"] = serde_json::json!(title);
            }
            if let Some(schema) = &t.input_schema {
                tool["inputSchema"] = schema.clone();
            } else {
                tool["inputSchema"] = mcp_fallback_input_schema();
            }
            if let Some(out_schema) = &t.output_schema {
                tool["outputSchema"] = out_schema.clone();
            }
            out.push(tool);
        }
        out
    }
}
