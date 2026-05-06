//! Integration test: run one full iteration of the rig orchestrator with
//! an OpenAI-compatible mocked endpoint and a real local tool. Asserts the
//! event sequence and DB persistence match the documented contract.

use mockito::Server;
use secbuddy_lib::rig_orchestrator;
use secbuddy_lib::test_support::{
    drive_all_approvals, event_types, seed_chat, seed_execution_mode, seed_ollama_via_mockito,
    spawn_state_for_test,
};
use serde_json::json;

#[cfg(target_os = "windows")]
const TEST_BINARY: &str = "cmd";
#[cfg(target_os = "windows")]
const TEST_ARGS: &str = "/c echo";
#[cfg(target_os = "windows")]
const TEST_TARGET: &str = "secbuddy";
#[cfg(not(target_os = "windows"))]
const TEST_BINARY: &str = "echo";
#[cfg(not(target_os = "windows"))]
const TEST_ARGS: &str = "";
#[cfg(not(target_os = "windows"))]
const TEST_TARGET: &str = "secbuddy";

fn register_echo_tool(state: &secbuddy_lib::test_support::RigTestState) {
    let json_def = format!(
        r#"[{{
            "name": "echotool",
            "description": "Echo a string for testing.",
            "binary": "{}",
            "risk_category": "active",
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
        TEST_BINARY
    );
    state.tools.load_local_tools_from_str(&json_def).unwrap();
}

fn mock_completions_two_turns(server: &mut Server, tool_call: bool) -> Vec<mockito::Mock> {
    let first_body = if tool_call {
        json!({
            "id": "chatcmpl-1",
            "object": "chat.completion",
            "created": 1_700_000_000_u64,
            "model": "llama3.2",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "I'll run echotool to demonstrate.",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "echotool",
                            "arguments": format!(r#"{{"args":"{}","target":"{}"}}"#, TEST_ARGS, TEST_TARGET)
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        })
    } else {
        json!({
            "id": "chatcmpl-1",
            "object": "chat.completion",
            "created": 1_700_000_000_u64,
            "model": "llama3.2",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "Done." },
                "finish_reason": "stop"
            }]
        })
    };

    let second_body = json!({
        "id": "chatcmpl-2",
        "object": "chat.completion",
        "created": 1_700_000_001_u64,
        "model": "llama3.2",
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": "All done." },
            "finish_reason": "stop"
        }]
    });

    let m1 = server
        .mock("POST", "/v1/chat/completions")
        .expect_at_least(1)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(first_body.to_string())
        .create();

    let m2 = server
        .mock("POST", "/v1/chat/completions")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(second_body.to_string())
        .create();

    vec![m1, m2]
}

#[tokio::test]
async fn local_tool_full_loop_with_approval() {
    let mut server = Server::new_async().await;
    let _mocks = mock_completions_two_turns(&mut server, true);

    let state = spawn_state_for_test().await;
    seed_ollama_via_mockito(&state.pool, &server.url()).await;
    seed_execution_mode(&state.pool, "guided").await;
    seed_chat(&state.pool, "chat-1").await;
    register_echo_tool(&state);

    let pool = state.pool.clone();
    let tools = state.tools.clone();
    let mcp_runtime = state.mcp_runtime.clone();
    let pending = state.pending_approvals.clone();
    let running = state.running_handles.clone();
    let dyn_sink = state.dyn_sink.clone();

    let pending_for_drive = pending.clone();
    let drive =
        tokio::spawn(async move { drive_all_approvals(&pending_for_drive, "approved", 1).await });

    let result: Result<(), String> = rig_orchestrator::run_agent_loop(
        pool.clone(),
        tools,
        mcp_runtime,
        pending,
        running,
        dyn_sink,
        "chat-1".to_string(),
        "Please run echotool".to_string(),
    )
    .await;

    drive.await.unwrap().expect("approval drive");
    result.expect("agent loop completed");

    let events = state.sink.snapshot();
    let kinds = event_types(&events);

    assert!(
        kinds.contains(&"MessageChunk".to_string()),
        "no MessageChunk emitted: {:?}",
        kinds
    );
    assert!(
        kinds.contains(&"MessageComplete".to_string()),
        "no MessageComplete: {:?}",
        kinds
    );
    assert!(
        kinds.contains(&"ConfidencePreview".to_string()),
        "no ConfidencePreview: {:?}",
        kinds
    );
    assert!(
        kinds.contains(&"ApprovalRequired".to_string()),
        "no ApprovalRequired: {:?}",
        kinds
    );
    assert!(
        kinds.contains(&"ToolRunning".to_string()),
        "no ToolRunning: {:?}",
        kinds
    );
    assert!(
        kinds.contains(&"ToolComplete".to_string()),
        "no ToolComplete: {:?}",
        kinds
    );

    let approval_idx = kinds.iter().position(|k| k == "ApprovalRequired").unwrap();
    let running_idx = kinds.iter().position(|k| k == "ToolRunning").unwrap();
    let complete_idx = kinds.iter().rposition(|k| k == "ToolComplete").unwrap();
    assert!(approval_idx < running_idx);
    assert!(running_idx < complete_idx);

    let invocations = secbuddy_lib::test_support::query_invocations(&pool, "chat-1").await;
    assert_eq!(invocations.len(), 1);
    let row = &invocations[0];
    assert_eq!(row.tool_name, "echotool");
    assert_eq!(row.tool_source, "local");
    assert_eq!(row.status, "complete");
    assert!(row.raw_output.as_deref().unwrap_or("").contains("secbuddy"));
}

#[tokio::test]
async fn local_tool_full_loop_with_dry_run() {
    let mut server = Server::new_async().await;
    let _mocks = mock_completions_two_turns(&mut server, true);

    let state = spawn_state_for_test().await;
    seed_ollama_via_mockito(&state.pool, &server.url()).await;
    seed_execution_mode(&state.pool, "guided").await;
    seed_chat(&state.pool, "chat-2").await;
    register_echo_tool(&state);

    let pool = state.pool.clone();
    let pending_for_drive = state.pending_approvals.clone();
    let drive =
        tokio::spawn(async move { drive_all_approvals(&pending_for_drive, "dry_run", 1).await });

    let result: Result<(), String> = rig_orchestrator::run_agent_loop(
        pool.clone(),
        state.tools.clone(),
        state.mcp_runtime.clone(),
        state.pending_approvals.clone(),
        state.running_handles.clone(),
        state.dyn_sink.clone(),
        "chat-2".to_string(),
        "Please run echotool".to_string(),
    )
    .await;

    drive.await.unwrap().expect("approval drive");
    result.expect("agent loop completed");

    let events = state.sink.snapshot();
    let kinds = event_types(&events);
    assert!(kinds.contains(&"ApprovalRequired".to_string()));
    assert!(kinds.contains(&"ToolComplete".to_string()));
    assert!(
        !kinds.contains(&"ToolRunning".to_string()),
        "dry_run should skip ToolRunning"
    );

    let invocations = secbuddy_lib::test_support::query_invocations(&pool, "chat-2").await;
    assert_eq!(invocations.len(), 1);
    assert_eq!(invocations[0].status, "complete");
}

#[tokio::test]
async fn local_tool_full_loop_with_denial() {
    let mut server = Server::new_async().await;
    let _mocks = mock_completions_two_turns(&mut server, true);

    let state = spawn_state_for_test().await;
    seed_ollama_via_mockito(&state.pool, &server.url()).await;
    seed_execution_mode(&state.pool, "guided").await;
    seed_chat(&state.pool, "chat-3").await;
    register_echo_tool(&state);

    let pool = state.pool.clone();
    let pending_for_drive = state.pending_approvals.clone();
    let drive =
        tokio::spawn(async move { drive_all_approvals(&pending_for_drive, "denied", 1).await });

    let result: Result<(), String> = rig_orchestrator::run_agent_loop(
        pool.clone(),
        state.tools.clone(),
        state.mcp_runtime.clone(),
        state.pending_approvals.clone(),
        state.running_handles.clone(),
        state.dyn_sink.clone(),
        "chat-3".to_string(),
        "Please run echotool".to_string(),
    )
    .await;

    drive.await.unwrap().expect("approval drive");
    result.expect("agent loop completed");

    let events = state.sink.snapshot();
    let kinds = event_types(&events);
    assert!(
        kinds.contains(&"ToolDenied".to_string()),
        "no ToolDenied: {:?}",
        kinds
    );
    assert!(
        !kinds.contains(&"ToolRunning".to_string()),
        "denial should skip ToolRunning"
    );

    let invocations = secbuddy_lib::test_support::query_invocations(&pool, "chat-3").await;
    assert_eq!(invocations.len(), 1);
    assert_eq!(invocations[0].status, "denied");
}
