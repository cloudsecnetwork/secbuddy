//! Rig `Tool` adapters wrapping the existing local and MCP runners.
//!
//! These adapters exist so the rig orchestrator has a single source of truth
//! for tool metadata (`definition()`). The orchestrator never calls `call()`
//! on the gated path — it dispatches through `tool_runner::run_local_with_cancel`
//! / `mcp_client::McpRuntime::call_tool` directly so PID tracking, timeouts,
//! and the approval gate stay intact. `call()` is provided for autonomous-mode
//! flows and unit tests.

#![allow(dead_code)] // adapters are exercised by tests + autonomous-mode flows

use crate::mcp_client::McpRuntime;
use crate::tool_registry::ToolRegistry;
use crate::tool_runner;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicU32;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::oneshot;

/// Args every SecBuddy tool accepts. Mirrors `tools.json` and the MCP fallback schema.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct SecBuddyToolArgs {
    #[serde(default)]
    pub args: String,
    #[serde(default)]
    pub target: String,
}

#[derive(Debug, Error)]
pub enum ToolAdapterError {
    #[error("{0}")]
    Runtime(String),
}

/// Adapter for a local (subprocess) tool. `NAME_PLACEHOLDER` is required by the
/// trait but the real name comes from `tool_name`; clients should always read
/// `Tool::name()` (which we override).
pub struct LocalToolAdapter {
    pub tool_name: String,
    pub description: String,
    pub parameters: serde_json::Value,
    pub registry: Arc<ToolRegistry>,
    pub timeout_secs: u64,
}

impl Tool for LocalToolAdapter {
    const NAME: &'static str = "__secbuddy_local__";
    type Error = ToolAdapterError;
    type Args = SecBuddyToolArgs;
    type Output = String;

    fn name(&self) -> String {
        self.tool_name.clone()
    }

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.tool_name.clone(),
            description: self.description.clone(),
            parameters: self.parameters.clone(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let invocation_id = format!("autonomous-{}", uuid::Uuid::new_v4());
        let pid = Arc::new(AtomicU32::new(0));
        let (_cancel_tx, cancel_rx) = oneshot::channel();
        let result = tool_runner::run_local_with_cancel(
            &self.registry,
            &invocation_id,
            &self.tool_name,
            &args.target,
            &args.args,
            self.timeout_secs,
            pid,
            cancel_rx,
        )
        .await;
        if result.status == "complete" {
            Ok(result.raw_output.unwrap_or_default())
        } else {
            Err(ToolAdapterError::Runtime(
                result.raw_output.unwrap_or_else(|| "tool failed".to_string()),
            ))
        }
    }
}

/// Adapter for an MCP tool. `call()` blocks on the MCP server inside `spawn_blocking`
/// so the futures stay `Send`.
pub struct McpToolAdapter {
    pub tool_name: String,
    pub server_name: String,
    pub description: String,
    pub parameters: serde_json::Value,
    pub runtime: Arc<McpRuntime>,
    pub timeout_secs: u64,
}

impl Tool for McpToolAdapter {
    const NAME: &'static str = "__secbuddy_mcp__";
    type Error = ToolAdapterError;
    type Args = SecBuddyToolArgs;
    type Output = String;

    fn name(&self) -> String {
        self.tool_name.clone()
    }

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.tool_name.clone(),
            description: self.description.clone(),
            parameters: self.parameters.clone(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let runtime = self.runtime.clone();
        let server = self.server_name.clone();
        let name = self.tool_name.clone();
        let timeout = self.timeout_secs;
        let payload = serde_json::json!({ "args": args.args, "target": args.target });
        let result = tokio::task::spawn_blocking(move || {
            runtime.call_tool(&server, &name, payload, timeout)
        })
        .await
        .map_err(|e| ToolAdapterError::Runtime(e.to_string()))?;
        result.map_err(ToolAdapterError::Runtime)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_adapter_definition_matches_inputs() {
        let registry = Arc::new(ToolRegistry::new());
        let adapter = LocalToolAdapter {
            tool_name: "nmap".to_string(),
            description: "scan ports".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "args": { "type": "string" },
                    "target": { "type": "string" }
                },
                "required": ["args", "target"]
            }),
            registry,
            timeout_secs: 30,
        };
        let def = adapter.definition(String::new()).await;
        assert_eq!(def.name, "nmap");
        assert_eq!(def.description, "scan ports");
        assert_eq!(adapter.name(), "nmap");
        assert!(def.parameters.get("properties").is_some());
    }

    #[tokio::test]
    async fn mcp_adapter_definition_round_trip() {
        let runtime = Arc::new(McpRuntime::new());
        let adapter = McpToolAdapter {
            tool_name: "ping".to_string(),
            server_name: "echo".to_string(),
            description: "ping host".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "args": { "type": "string" },
                    "target": { "type": "string" }
                },
                "required": ["args", "target"]
            }),
            runtime,
            timeout_secs: 5,
        };
        let def = adapter.definition(String::new()).await;
        assert_eq!(def.name, "ping");
        assert_eq!(adapter.name(), "ping");
    }

    #[test]
    fn args_deserialize_partial() {
        let v: SecBuddyToolArgs =
            serde_json::from_str(r#"{"args":"-sV","target":"127.0.0.1"}"#).unwrap();
        assert_eq!(v.args, "-sV");
        assert_eq!(v.target, "127.0.0.1");

        let v2: SecBuddyToolArgs = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(v2.args, "");
        assert_eq!(v2.target, "");
    }
}
