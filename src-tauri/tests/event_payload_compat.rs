//! Regression test: the orchestrator's `events::*` helpers must produce the
//! exact JSON shapes the frontend's `ChatEventPayload` discriminated union
//! expects. Pinning these here keeps the wire contract from drifting silently.

use assert_json_diff::assert_json_eq;
use secbuddy_lib::rig_orchestrator::events;
use serde_json::json;

#[test]
fn message_chunk_shape() {
    let expected = json!({ "type": "MessageChunk", "content": "hello" });
    assert_json_eq!(events::message_chunk("hello"), expected);
}

#[test]
fn message_complete_shape() {
    let expected = json!({ "type": "MessageComplete", "message_id": "msg-1" });
    assert_json_eq!(events::message_complete("msg-1"), expected);
}

#[test]
fn approval_required_shape() {
    let expected = json!({
        "type": "ApprovalRequired",
        "invocation_id": "inv-1",
        "tool_name": "nmap",
        "args": "-sV",
        "target": "127.0.0.1",
        "risk_category": "active"
    });
    assert_json_eq!(
        events::approval_required("inv-1", "nmap", "-sV", "127.0.0.1", "active"),
        expected
    );
}

#[test]
fn tool_running_shape_with_phase() {
    let expected = json!({
        "type": "ToolRunning",
        "invocation_id": "inv-2",
        "tool_name": "curl",
        "args": "-I",
        "risk_category": "passive",
        "phase_name": "http"
    });
    assert_json_eq!(
        events::tool_running("inv-2", "curl", "-I", "passive", Some("http")),
        expected
    );
}

#[test]
fn tool_running_shape_without_phase() {
    let expected = json!({
        "type": "ToolRunning",
        "invocation_id": "inv-3",
        "tool_name": "dig",
        "args": "",
        "risk_category": "passive",
        "phase_name": null
    });
    assert_json_eq!(
        events::tool_running("inv-3", "dig", "", "passive", None),
        expected
    );
}

#[test]
fn tool_complete_with_status_shape() {
    let expected = json!({
        "type": "ToolComplete",
        "invocation_id": "inv-4",
        "output": "ok",
        "duration_ms": 42,
        "status": "complete",
        "phase_name": "recon"
    });
    assert_json_eq!(
        events::tool_complete("inv-4", "ok", Some(42), Some("complete"), Some("recon")),
        expected
    );
}

#[test]
fn tool_complete_simple_shape_dry_run() {
    let expected = json!({
        "type": "ToolComplete",
        "invocation_id": "inv-5",
        "output": "preview",
        "duration_ms": 0
    });
    assert_json_eq!(
        events::tool_complete_simple("inv-5", "preview", Some(0)),
        expected
    );
}

#[test]
fn tool_denied_shape() {
    let expected = json!({
        "type": "ToolDenied",
        "invocation_id": "inv-6",
        "reason": "User denied"
    });
    assert_json_eq!(events::tool_denied("inv-6", "User denied"), expected);
}

#[test]
fn finding_found_shape() {
    let expected = json!({
        "type": "FindingFound",
        "id": "f-1",
        "title": "Open SSH",
        "severity": "medium",
        "description": "Port 22 open"
    });
    assert_json_eq!(
        events::finding_found("f-1", "Open SSH", "medium", "Port 22 open"),
        expected
    );
}

#[test]
fn confidence_preview_shape() {
    let plan_entry = events::execution_plan_entry("nmap", "-sV", "10.0.0.1", "active", true);
    let expected_plan = json!({
        "tool_name": "nmap",
        "args": "-sV",
        "target": "10.0.0.1",
        "risk_category": "active",
        "requires_approval": true
    });
    assert_json_eq!(plan_entry.clone(), expected_plan);

    let expected = json!({
        "type": "ConfidencePreview",
        "explanation": "explain",
        "what_will_be_tested": "explain",
        "tool_count": 1,
        "execution_plan": [expected_plan]
    });
    assert_json_eq!(
        events::confidence_preview("explain", "explain", 1, vec![plan_entry]),
        expected
    );
}
