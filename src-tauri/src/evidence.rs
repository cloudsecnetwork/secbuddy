//! Findings: persisted from LLM-reported findings via the report_finding tool.
//! The LLM is the sole source of findings; no heuristic extraction.

use crate::db;
use serde::Deserialize;
use sqlx::SqlitePool;
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize)]
pub struct ExtractedFinding {
    pub title: String,
    pub severity: String,
    pub description: String,
    #[serde(default)]
    pub mitre_ref: Option<String>,
    #[serde(default)]
    pub owasp_ref: Option<String>,
    #[serde(default)]
    pub cwe_ref: Option<String>,
    #[serde(default)]
    pub recommended_action: Option<String>,
}

/// Parse a single finding from the report_finding tool call arguments (JSON).
/// Used when the LLM calls report_finding with title, severity, description, etc.
pub fn parse_finding_from_report_args(args_json: &str) -> Result<ExtractedFinding, String> {
    let v: serde_json::Value =
        serde_json::from_str(args_json).map_err(|e| format!("Invalid JSON: {}", e))?;
    let obj = v.as_object().ok_or("Expected JSON object")?;
    let title = obj
        .get("title")
        .and_then(|v| v.as_str())
        .ok_or("Missing or invalid 'title'")?
        .to_string();
    let severity = obj
        .get("severity")
        .and_then(|v| v.as_str())
        .unwrap_or("medium")
        .to_string();
    let description = obj
        .get("description")
        .and_then(|v| v.as_str())
        .ok_or("Missing or invalid 'description'")?
        .to_string();
    let mitre_ref = obj.get("mitre_ref").and_then(|v| v.as_str()).map(String::from);
    let owasp_ref = obj.get("owasp_ref").and_then(|v| v.as_str()).map(String::from);
    let cwe_ref = obj.get("cwe_ref").and_then(|v| v.as_str()).map(String::from);
    let recommended_action = obj
        .get("recommended_action")
        .and_then(|v| v.as_str())
        .map(String::from);
    Ok(ExtractedFinding {
        title,
        severity,
        description,
        mitre_ref,
        owasp_ref,
        cwe_ref,
        recommended_action,
    })
}

pub async fn persist_findings(
    pool: &SqlitePool,
    chat_id: &str,
    tool_invocation_id: Option<&str>,
    findings: &[ExtractedFinding],
) -> Result<Vec<String>, String> {
    let mut ids = Vec::with_capacity(findings.len());
    for f in findings {
        let id = Uuid::new_v4().to_string();
        db::insert_finding(
            pool,
            &id,
            chat_id,
            tool_invocation_id,
            &f.title,
            &f.severity,
            &f.description,
            f.mitre_ref.as_deref(),
            f.owasp_ref.as_deref(),
            f.cwe_ref.as_deref(),
            f.recommended_action.as_deref(),
        )
        .await
        .map_err(|e| e.to_string())?;
        ids.push(id);
    }
    Ok(ids)
}
