//! Integration test: rig orchestrator dispatches MCP tool calls through
//! `mcp_client::McpRuntime`. We register an MCP tool in the registry without
//! a corresponding running server. The runtime returns a clean error which
//! the orchestrator surfaces as `ToolComplete{status="failed"}`. This proves
//! the MCP branch is taken and produces the documented failure payload.

use mockito::Server;
use secbuddy_lib::rig_orchestrator;
use secbuddy_lib::test_support::{
    drive_all_approvals, event_types, query_invocations, register_test_mcp_tool, seed_chat,
    seed_execution_mode, seed_ollama_via_mockito, spawn_state_for_test,
};
use serde_json::json;

fn mock_completions_with_mcp_tool_call(server: &mut Server) -> Vec<mockito::Mock> {
    let first_body = json!({
        "id": "chatcmpl-1",
        "object": "chat.completion",
        "created": 1_700_000_000_u64,
        "model": "llama3.2",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "Calling MCP tool.",
                "tool_calls": [{
                    "id": "call_mcp_1",
                    "type": "function",
                    "function": {
                        "name": "mcp_ping",
                        "arguments": r#"{"args":"-c 1","target":"127.0.0.1"}"#
                    }
                }]
            },
            "finish_reason": "tool_calls"
        }]
    });
    let second_body = json!({
        "id": "chatcmpl-2",
        "object": "chat.completion",
        "created": 1_700_000_001_u64,
        "model": "llama3.2",
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": "Wrapping up." },
            "finish_reason": "stop"
        }]
    });
    vec![
        server
            .mock("POST", "/v1/chat/completions")
            .expect_at_least(1)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(first_body.to_string())
            .create(),
        server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(second_body.to_string())
            .create(),
    ]
}

#[tokio::test]
async fn mcp_tool_dispatch_branch_runs_and_surfaces_failure() {
    let mut server = Server::new_async().await;
    let _mocks = mock_completions_with_mcp_tool_call(&mut server);

    let state = spawn_state_for_test().await;
    seed_ollama_via_mockito(&state.pool, &server.url()).await;
    seed_execution_mode(&state.pool, "guided").await;
    seed_chat(&state.pool, "chat-mcp-1").await;
    register_test_mcp_tool(&state.tools, "mcp_ping", "phantom-server");

    let pool = state.pool.clone();
    let pending_for_drive = state.pending_approvals.clone();
    let drive =
        tokio::spawn(async move { drive_all_approvals(&pending_for_drive, "approved", 1).await });

    let result: Result<(), String> = rig_orchestrator::run_agent_loop(
        pool.clone(),
        state.tools.clone(),
        state.mcp_runtime.clone(),
        state.pending_approvals.clone(),
        state.running_handles.clone(),
        state.dyn_sink.clone(),
        "chat-mcp-1".to_string(),
        "Ping target".to_string(),
    )
    .await;

    drive.await.unwrap().expect("approval drive");
    result.expect("agent loop completed");

    let events = state.sink.snapshot();
    let kinds = event_types(&events);

    assert!(kinds.contains(&"ApprovalRequired".to_string()));
    assert!(
        kinds.contains(&"ToolRunning".to_string()),
        "MCP tool should still emit ToolRunning: {:?}",
        kinds
    );
    assert!(kinds.contains(&"ToolComplete".to_string()));

    let invocations = query_invocations(&pool, "chat-mcp-1").await;
    assert_eq!(invocations.len(), 1);
    assert_eq!(invocations[0].tool_name, "mcp_ping");
    assert_eq!(
        invocations[0].tool_source, "mcp",
        "must take the MCP dispatch branch"
    );
    assert_eq!(
        invocations[0].status, "failed",
        "phantom server must fail cleanly"
    );
    let raw = invocations[0].raw_output.as_deref().unwrap_or("");
    assert!(
        raw.to_lowercase().contains("not connected") || raw.to_lowercase().contains("phantom"),
        "expected MCP server-not-connected error, got: {}",
        raw
    );
}
