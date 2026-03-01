//! Execution policy: mode + risk category.

/// True if this tool run requires user approval given execution_mode and tool risk_category.
/// When multiple tools are proposed in one turn (multi_tool_batch), guided mode requires
/// approval for every tool so the user can accept/skip each individually.
/// Policy is deterministic and enforced in code; the LLM does not decide.
pub fn requires_approval(
    execution_mode: &str,
    risk_category: &str,
    multi_tool_batch: bool,
) -> bool {
    match execution_mode {
        "manual" => true,
        "autonomous" => false,
        "guided" => {
            if multi_tool_batch {
                true
            } else {
                matches!(risk_category, "active" | "high_impact")
            }
        }
        _ => true, // unknown mode: require approval
    }
}
