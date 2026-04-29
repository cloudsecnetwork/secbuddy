//! Helpers that build the exact `chat_event` JSON shapes the frontend expects.
//! `tests/event_payload_compat.rs` pins these shapes against the frontend's
//! `ChatEventPayload` discriminated union.

use serde_json::{json, Value};

pub fn message_chunk(content: &str) -> Value {
    json!({ "type": "MessageChunk", "content": content })
}

pub fn message_complete(message_id: &str) -> Value {
    json!({ "type": "MessageComplete", "message_id": message_id })
}

pub fn approval_required(
    invocation_id: &str,
    tool_name: &str,
    args: &str,
    target: &str,
    risk_category: &str,
) -> Value {
    json!({
        "type": "ApprovalRequired",
        "invocation_id": invocation_id,
        "tool_name": tool_name,
        "args": args,
        "target": target,
        "risk_category": risk_category,
    })
}

pub fn tool_running(
    invocation_id: &str,
    tool_name: &str,
    args: &str,
    risk_category: &str,
    phase_name: Option<&str>,
) -> Value {
    json!({
        "type": "ToolRunning",
        "invocation_id": invocation_id,
        "tool_name": tool_name,
        "args": args,
        "risk_category": risk_category,
        "phase_name": phase_name,
    })
}

pub fn tool_complete(
    invocation_id: &str,
    output: &str,
    duration_ms: Option<i64>,
    status: Option<&str>,
    phase_name: Option<&str>,
) -> Value {
    let mut obj = json!({
        "type": "ToolComplete",
        "invocation_id": invocation_id,
        "output": output,
        "duration_ms": duration_ms,
    });
    if let Some(s) = status {
        obj["status"] = json!(s);
        obj["phase_name"] = json!(phase_name);
    }
    obj
}

pub fn tool_complete_simple(invocation_id: &str, output: &str, duration_ms: Option<i64>) -> Value {
    json!({
        "type": "ToolComplete",
        "invocation_id": invocation_id,
        "output": output,
        "duration_ms": duration_ms,
    })
}

pub fn tool_denied(invocation_id: &str, reason: &str) -> Value {
    json!({
        "type": "ToolDenied",
        "invocation_id": invocation_id,
        "reason": reason,
    })
}

pub fn finding_found(id: &str, title: &str, severity: &str, description: &str) -> Value {
    json!({
        "type": "FindingFound",
        "id": id,
        "title": title,
        "severity": severity,
        "description": description,
    })
}

pub fn confidence_preview(
    explanation: &str,
    what_will_be_tested: &str,
    tool_count: usize,
    execution_plan: Vec<Value>,
) -> Value {
    json!({
        "type": "ConfidencePreview",
        "explanation": explanation,
        "what_will_be_tested": what_will_be_tested,
        "tool_count": tool_count,
        "execution_plan": execution_plan,
    })
}

pub fn execution_plan_entry(
    tool_name: &str,
    args: &str,
    target: &str,
    risk_category: &str,
    requires_approval: bool,
) -> Value {
    json!({
        "tool_name": tool_name,
        "args": args,
        "target": target,
        "risk_category": risk_category,
        "requires_approval": requires_approval,
    })
}
