//! Test helpers exposed from the lib so integration tests in `tests/` can
//! reuse them. Hidden from docs to avoid surfacing in the public API.

#![doc(hidden)]
#![allow(dead_code)]

use crate::db;
use crate::event_sink::{CapturingSink, EventSink};
use crate::mcp_client::McpRuntime;
use crate::tool_registry::ToolRegistry;
use serde_json::Value;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};
use std::str::FromStr;
use std::sync::Arc;

/// In-memory sqlite pool with all production migrations applied.
pub async fn init_test_db() -> SqlitePool {
    let opts = SqliteConnectOptions::from_str("sqlite::memory:")
        .unwrap()
        .create_if_missing(true);
    let pool = SqlitePool::connect_with(opts).await.unwrap();
    crate::db::migrations_run_for_tests(&pool).await.unwrap();
    pool
}

/// Wrap a `CapturingSink` as the `Arc<dyn EventSink>` the orchestrators need.
pub fn capturing_sink() -> (CapturingSink, Arc<dyn EventSink>) {
    let cap = CapturingSink::new();
    let dyn_sink: Arc<dyn EventSink> = Arc::new(cap.clone());
    (cap, dyn_sink)
}

/// Register a local tool definition with a custom binary path so tests can
/// avoid relying on `where`/`which` lookups.
pub fn register_test_local_tool(
    registry: &ToolRegistry,
    name: &str,
    binary_path: &str,
    description: &str,
    risk_category: &str,
) {
    let json = format!(
        r#"[{{
            "name": "{name}",
            "description": "{description}",
            "binary": "{binary_path}",
            "risk_category": "{risk_category}",
            "category": "recon",
            "inputSchema": {{
                "type": "object",
                "properties": {{
                    "args": {{ "type": "string" }},
                    "target": {{ "type": "string" }}
                }},
                "required": ["args", "target"]
            }}
        }}]"#,
        name = name,
        description = description,
        binary_path = binary_path.replace('\\', "\\\\"),
        risk_category = risk_category,
    );
    registry.load_local_tools_from_str(&json).unwrap();
    // Some platforms' detection step (`where`/`which`) won't see the absolute
    // path we baked in, so manually mark it available.
    registry.force_set_local_path(name, Some(binary_path.to_string()));
}

/// Register an MCP tool. The orchestrator dispatches MCP calls through
/// `McpRuntime::call_tool`, which fails (cleanly) for unconfigured servers —
/// useful for asserting the dispatch branch is taken without spawning a
/// real MCP server.
pub fn register_test_mcp_tool(registry: &ToolRegistry, name: &str, server: &str) {
    use crate::tool_registry::ToolInfo;
    registry.register_mcp_tool(ToolInfo {
        name: name.to_string(),
        title: None,
        description: format!("Test MCP tool {}", name),
        available: true,
        detected_path: None,
        source: "mcp".to_string(),
        server_name: Some(server.to_string()),
        risk_category: "active".to_string(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "args": { "type": "string" },
                "target": { "type": "string" }
            },
            "required": ["args", "target"]
        })),
        category: Some("recon".to_string()),
        output_schema: None,
        alternatives: None,
        returns: None,
    });
}

/// Bundle of state every orchestrator-driven test needs.
pub struct RigTestState {
    pub pool: SqlitePool,
    pub tools: Arc<ToolRegistry>,
    pub mcp_runtime: Arc<McpRuntime>,
    pub pending_approvals: Arc<
        std::sync::RwLock<std::collections::HashMap<String, tokio::sync::oneshot::Sender<String>>>,
    >,
    pub running_handles:
        Arc<tokio::sync::Mutex<std::collections::HashMap<String, crate::RunningToolEntry>>>,
    pub sink: CapturingSink,
    pub dyn_sink: Arc<dyn EventSink>,
}

pub async fn spawn_state_for_test() -> RigTestState {
    let pool = init_test_db().await;
    let tools = Arc::new(ToolRegistry::new());
    let mcp_runtime = Arc::new(McpRuntime::new());
    let pending_approvals = Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
    let running_handles = Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new()));
    let (sink, dyn_sink) = capturing_sink();
    RigTestState {
        pool,
        tools,
        mcp_runtime,
        pending_approvals,
        running_handles,
        sink,
        dyn_sink,
    }
}

/// Helper: pull every payload's `type` field in order. Useful for asserting
/// the event sequence without caring about field-by-field equality.
pub fn event_types(events: &[Value]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| {
            e.get("type")
                .and_then(|t| t.as_str())
                .map(|s| s.to_string())
        })
        .collect()
}

/// Send a fixed approval decision for the next pending approval inserted into
/// `pending_approvals`. Polls briefly so the orchestrator has time to register.
pub async fn drive_approval(
    pending: &Arc<
        std::sync::RwLock<std::collections::HashMap<String, tokio::sync::oneshot::Sender<String>>>,
    >,
    decision: &str,
) -> Result<(), String> {
    for _ in 0..200 {
        let key_opt = {
            let guard = pending.read().unwrap();
            guard.keys().next().cloned()
        };
        if let Some(key) = key_opt {
            let tx = pending.write().unwrap().remove(&key).unwrap();
            tx.send(decision.to_string())
                .map_err(|_| "send failed".to_string())?;
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    Err("no pending approval registered within timeout".to_string())
}

/// Drive every approval still pending, applying the same decision to all.
pub async fn drive_all_approvals(
    pending: &Arc<
        std::sync::RwLock<std::collections::HashMap<String, tokio::sync::oneshot::Sender<String>>>,
    >,
    decision: &str,
    expected: usize,
) -> Result<(), String> {
    let mut sent = 0;
    let mut waited = 0;
    while sent < expected {
        let key_opt = {
            let guard = pending.read().unwrap();
            guard.keys().next().cloned()
        };
        match key_opt {
            Some(key) => {
                let tx = pending.write().unwrap().remove(&key).unwrap();
                tx.send(decision.to_string())
                    .map_err(|_| "send failed".to_string())?;
                sent += 1;
            }
            None => {
                if waited > 200 {
                    return Err("timeout waiting for pending approvals".to_string());
                }
                waited += 1;
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }
    }
    Ok(())
}

/// Seed a chat row so the orchestrator's DB writes have a parent.
pub async fn seed_chat(pool: &SqlitePool, chat_id: &str) {
    db::create_chat(pool, chat_id, "test-chat", "recon")
        .await
        .unwrap();
}

/// Seed the LLM provider/base_url settings for a given mockito server.
pub async fn seed_ollama_via_mockito(pool: &SqlitePool, base_url: &str) {
    db::save_setting(pool, "llm_provider", "ollama")
        .await
        .unwrap();
    db::save_setting(pool, "llm_base_url", base_url)
        .await
        .unwrap();
    db::save_setting(pool, "llm_model", "llama3.2")
        .await
        .unwrap();
}

/// Seed `execution_mode` so all approvals trigger via gating.
pub async fn seed_execution_mode(pool: &SqlitePool, mode: &str) {
    db::save_setting(pool, "execution_mode", mode)
        .await
        .unwrap();
}

/// Lightweight projection of `tool_invocations` rows for tests.
pub struct InvocationRow {
    pub id: String,
    pub tool_name: String,
    pub tool_source: String,
    pub raw_output: Option<String>,
    pub status: String,
}

pub async fn query_invocations(pool: &SqlitePool, chat_id: &str) -> Vec<InvocationRow> {
    let rows: Vec<(String, String, String, Option<String>, String)> = sqlx::query_as(
        "SELECT id, tool_name, tool_source, raw_output, status FROM tool_invocations WHERE chat_id = ? ORDER BY created_at ASC",
    )
    .bind(chat_id)
    .fetch_all(pool)
    .await
    .unwrap();
    rows.into_iter()
        .map(
            |(id, tool_name, tool_source, raw_output, status)| InvocationRow {
                id,
                tool_name,
                tool_source,
                raw_output,
                status,
            },
        )
        .collect()
}
