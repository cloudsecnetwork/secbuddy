//! Convert `ToolRegistry` entries into Rig `ToolDefinition`s.
//!
//! Parallel to `ToolRegistry::list_available_for_llm()` (which returns raw OpenAI
//! function-call JSON). The Rig pipeline accepts `Vec<rig::completion::ToolDefinition>`,
//! which serializes to the same `{ "type": "function", "function": {...} }` wire
//! format on the OpenAI/Ollama path, so semantics are preserved.

use crate::tool_registry::ToolRegistry;
use rig::completion::ToolDefinition;
use serde_json::json;

pub const REPORT_FINDING_TOOL_NAME: &str = "report_finding";

fn mcp_fallback_input_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "args": { "type": "string", "description": "CLI or tool arguments" },
            "target": { "type": "string", "description": "Primary target (host, IP, URL)" }
        },
        "required": ["args", "target"]
    })
}

/// All available local + MCP tools as Rig `ToolDefinition`s, with the
/// synthetic `report_finding` tool appended.
pub fn build_tool_definitions(registry: &ToolRegistry) -> Vec<ToolDefinition> {
    let mut defs = Vec::new();
    for raw in registry.list_available_for_llm() {
        let func = match raw.get("function") {
            Some(f) => f,
            None => continue,
        };
        let name = func
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .to_string();
        if name.is_empty() {
            continue;
        }
        let description = func
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap_or("")
            .to_string();
        let parameters = func
            .get("parameters")
            .cloned()
            .unwrap_or_else(mcp_fallback_input_schema);
        defs.push(ToolDefinition {
            name,
            description,
            parameters,
        });
    }
    defs.push(report_finding_definition());
    defs
}

pub fn report_finding_definition() -> ToolDefinition {
    ToolDefinition {
        name: REPORT_FINDING_TOOL_NAME.to_string(),
        description:
            "Report a security finding from your analysis. Call this when tool output or context \
             clearly indicates a finding (e.g. open risky port, certificate issue, \
             misconfiguration). Only report findings that are directly supported by the evidence. \
             Use severity: low, medium, high, or critical.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "title": { "type": "string", "description": "Short finding title (e.g. SSH Service Exposed)" },
                "severity": { "type": "string", "description": "One of: low, medium, high, critical", "enum": ["low", "medium", "high", "critical"] },
                "description": { "type": "string", "description": "Clear description of the finding and evidence" },
                "mitre_ref": { "type": "string", "description": "Optional MITRE ATT&CK ID (e.g. T1021.004)" },
                "owasp_ref": { "type": "string", "description": "Optional OWASP reference" },
                "cwe_ref": { "type": "string", "description": "Optional CWE ID (e.g. CWE-295)" },
                "recommended_action": { "type": "string", "description": "Optional remediation or next step" }
            },
            "required": ["title", "severity", "description"]
        }),
    }
}
