//! Agent loop: send_message triggers LLM + tool execution. Approval flow for high blast radius.

use crate::audit;
use crate::context;
use crate::db;
use crate::evidence;
use crate::governance;
use crate::llm_client;
use crate::prompts;
use crate::tool_runner;
use crate::tool_registry::{ToolInfo, ToolRegistry};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::collections::HashSet;
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::oneshot;
use uuid::Uuid;

const MAX_ITERATIONS: u32 = 20;
const DEFAULT_MODEL: &str = "llama3.2";

/// Name of the synthetic tool the LLM uses to report security findings. Not executed; we persist and return a result.
const REPORT_FINDING_TOOL_NAME: &str = "report_finding";

/// Tool definition for report_finding: LLM reports a finding with title, severity, description, optional refs.
fn report_finding_tool_json() -> Value {
    json!({
        "type": "function",
        "function": {
            "name": REPORT_FINDING_TOOL_NAME,
            "description": "Report a security finding from your analysis. Call this when tool output or context clearly indicates a finding (e.g. open risky port, certificate issue, misconfiguration). Only report findings that are directly supported by the evidence. Use severity: low, medium, high, or critical.",
            "parameters": {
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
            }
        }
    })
}

/// When set (e.g. DEBUG_LLM=1), log prompt and LLM response to stderr for testing.
fn debug_llm_enabled() -> bool {
    std::env::var("DEBUG_LLM")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes"))
        .unwrap_or(false)
}

fn emit_chat_event(app_handle: &tauri::AppHandle, payload: Value) {
    let _ = app_handle.emit("chat_event", payload);
}

/// Build tool context block for system prompt: tools by category, risk guidance, alternatives, and explicit rule.
fn build_tool_context(tools: &ToolRegistry) -> String {
    let list = tools.list_tools();
    // Group by category (only available tools for brevity)
    let mut by_category: std::collections::HashMap<String, Vec<&ToolInfo>> = std::collections::HashMap::new();
    for t in &list {
        if !t.available && t.source == "local" {
            continue;
        }
        let cat = t.category.as_deref().unwrap_or("other");
        by_category.entry(cat.to_string()).or_default().push(t);
    }
    let order = [
        "network",
        "recon",
        "http",
        "tls",
        "web",
        "brute_force",
        "binary",
        "cloud",
        "other",
    ];
    let mut tools_by_cat = String::new();
    for cat in order {
        if let Some(entries) = by_category.get(cat) {
            let label = cat;
            let names: Vec<String> = entries
                .iter()
                .map(|t| {
                    let desc = t.description.split('.').next().unwrap_or(&t.description);
                    format!("{} ({})", t.name, desc)
                })
                .collect();
            if !tools_by_cat.is_empty() {
                tools_by_cat.push_str(". ");
            }
            tools_by_cat.push_str(&format!("{}: {}.", label, names.join(", ")));
        }
    }
    // Risk guidance
    let passive: Vec<&str> = list
        .iter()
        .filter(|t| t.risk_category == "passive")
        .map(|t| t.name.as_str())
        .collect();
    let active: Vec<&str> = list
        .iter()
        .filter(|t| t.risk_category == "active")
        .map(|t| t.name.as_str())
        .collect();
    let high: Vec<&str> = list
        .iter()
        .filter(|t| t.risk_category == "high_impact")
        .map(|t| t.name.as_str())
        .collect();
    let risk_block = format!(
        "Passive tools (recon only): {}. Active: {}. High-impact (use only with authorization): {}. \
         Prefer passive then active unless the user clearly needs deeper assessment.",
        passive.join(", "),
        active.join(", "),
        high.join(", ")
    );
    // Alternatives
    let alt_pairs: Vec<String> = list
        .iter()
        .filter_map(|t| {
            t.alternatives.as_ref().map(|a| {
                format!("{} → {}", t.name, a.join(", "))
            })
        })
        .collect();
    let alternatives_block = if alt_pairs.is_empty() {
        String::new()
    } else {
        format!("Alternatives: {}.", alt_pairs.join("; "))
    };
    // Explicit rule
    let rule = "When a tool returns failed or is unavailable, try one of its listed alternatives before reporting failure. \
               If the user explicitly skipped a tool, do not re-request that tool unless they ask.";
    let mut out = format!(
        "TOOLS BY CATEGORY (available): {}\n\nRISK: {}\n\n",
        tools_by_cat, risk_block
    );
    if !alternatives_block.is_empty() {
        out.push_str(&format!("{}\n\n", alternatives_block));
    }
    out.push_str(&format!("RULE: {}", rule));
    out
}


pub async fn run_agent_loop(
    pool: SqlitePool,
    tools: Arc<ToolRegistry>,
    mcp_runtime: Arc<crate::mcp_client::McpRuntime>,
    pending_approvals: Arc<std::sync::RwLock<std::collections::HashMap<String, oneshot::Sender<String>>>>,
    running_handles: Arc<tokio::sync::Mutex<std::collections::HashMap<String, crate::RunningToolEntry>>>,
    app_handle: tauri::AppHandle,
    chat_id: String,
    content: String,
) -> Result<(), String> {
    let user_msg_id = Uuid::new_v4().to_string();
    db::insert_message(&pool, &user_msg_id, &chat_id, "user", &content, None)
        .await
        .map_err(|e| e.to_string())?;
    db::update_chat_updated(&pool, &chat_id)
        .await
        .map_err(|e| e.to_string())?;

    context::set_goal_if_empty(&pool, &chat_id, &content).await?;

    let config = llm_client::get_llm_config_from_pool(&pool).await?;
    let model = db::get_setting(&pool, "llm_model")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());

    let chat_mode = db::get_chat_mode(&pool, &chat_id)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "recon".to_string());

    let tool_timeout_secs: u64 = db::get_setting(&pool, "tool_timeout_minutes")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.parse::<u64>().ok())
        .map(|m| m * 60)
        .unwrap_or(tool_runner::DEFAULT_TOOL_TIMEOUT_SECS);

    let mut iterations = 0;
    loop {
        if iterations >= MAX_ITERATIONS {
            break;
        }
        iterations += 1;

        let messages = db::get_chat_messages(&pool, &chat_id)
            .await
            .map_err(|e| e.to_string())?;
        let tool_context = build_tool_context(&tools);
        let battle_map = context::build_battle_map_from_db(&pool, &chat_id).await?;
        let battle_map_block = context::render_battle_map(&battle_map);
        let system_content = prompts::build_system_prompt(&chat_mode, &tool_context, Some(&battle_map_block));
        let api_messages = context::build_api_messages(&system_content, &messages, context::WINDOW_SIZE);

        let mut tools_json = tools.list_available_for_llm();
        tools_json.push(report_finding_tool_json());

        if debug_llm_enabled() {
            let prompt_log = serde_json::json!({
                "model": model,
                "messages": api_messages,
                "tools": tools_json
            });
            if let Ok(s) = serde_json::to_string_pretty(&prompt_log) {
                log::debug!("[DEBUG_LLM] === PROMPT ===\n{}\n", s);
            }
        }

        let result = llm_client::chat(&config, &model, &api_messages, &tools_json).await?;

        if debug_llm_enabled() {
            log::debug!("[DEBUG_LLM] === RESPONSE ===");
            log::debug!("content: {}", result.content);
            log::debug!("tool_calls: {}", result.tool_calls.len());
            for (i, tc) in result.tool_calls.iter().enumerate() {
                log::debug!("  [{}] {} args={}", i, tc.name, tc.arguments);
            }
            log::debug!("");
        }

        if !result.content.is_empty() {
            for chunk in result.content.chars().collect::<Vec<_>>().chunks(64) {
                let s: String = chunk.iter().collect();
                emit_chat_event(&app_handle, json!({ "type": "MessageChunk", "content": s }));
            }
            let assistant_msg_id = Uuid::new_v4().to_string();
            db::insert_message(&pool, &assistant_msg_id, &chat_id, "assistant", &result.content, None)
                .await
                .map_err(|e| e.to_string())?;
            emit_chat_event(&app_handle, json!({ "type": "MessageComplete", "message_id": assistant_msg_id }));
        }

        if result.tool_calls.is_empty() {
            break;
        }

        // Dedupe by (name, arguments) so the same command only gets one approval card
        let mut seen: HashSet<(String, String)> = HashSet::new();
        let tool_calls: Vec<_> = result
            .tool_calls
            .iter()
            .filter(|tc| seen.insert((tc.name.clone(), tc.arguments.clone())))
            .cloned()
            .collect();

        let execution_mode = db::get_setting(&pool, "execution_mode")
            .await
            .ok()
            .flatten()
            .unwrap_or_else(|| "guided".to_string());

        let real_tool_calls: Vec<_> = tool_calls
            .iter()
            .filter(|tc| tc.name != REPORT_FINDING_TOOL_NAME)
            .cloned()
            .collect();
        let real_indices: Vec<usize> = tool_calls
            .iter()
            .enumerate()
            .filter(|(_, tc)| tc.name != REPORT_FINDING_TOOL_NAME)
            .map(|(i, _)| i)
            .collect();
        let phase_name = phase_from_tool_names(
            &tools,
            &real_tool_calls.iter().map(|t| t.name.clone()).collect::<Vec<_>>(),
        );
        let phase_name_ref = phase_name.as_deref();

        let multi_tool_batch = real_tool_calls.len() > 1;

        // Build execution plan for transparency (real tools only; report_finding is not shown)
        let execution_plan: Vec<Value> = real_tool_calls
            .iter()
            .map(|tc| {
                let (args_str, target) = parse_tool_args(&tc.arguments);
                let risk_cat = tools.risk_category(&tc.name);
                let requires_approval =
                    governance::requires_approval(&execution_mode, &risk_cat, multi_tool_batch);
                json!({
                    "tool_name": tc.name,
                    "args": args_str,
                    "target": target,
                    "risk_category": risk_cat,
                    "requires_approval": requires_approval
                })
            })
            .collect();

        // Always show execution plan and explanation before any tool runs
        emit_chat_event(
            &app_handle,
            json!({
                "type": "ConfidencePreview",
                "explanation": result.content,
                "what_will_be_tested": result.content,
                "tool_count": tool_calls.len(),
                "execution_plan": execution_plan
            }),
        );

        struct ApprovedTool {
            invocation_id: String,
            tool_name: String,
            target: String,
            args_str: String,
            approval_id: String,
            risk_category: String,
            /// Index into tool_calls for ordering tool result messages
            tool_call_index: usize,
        }
        let mut approved_to_run: Vec<ApprovedTool> = Vec::new();

        // Pending (invocation_id, receiver, tool_name, target, args_str, risk_category, real_index)
        let mut pending_approval_rxs: Vec<(
            String,
            oneshot::Receiver<String>,
            String,
            String,
            String,
            String,
            usize,
        )> = Vec::new();

        // Pre-process report_finding: persist findings and prepare tool result messages (no approval/run).
        let mut ordered_tool_results: Vec<(String, Option<String>)> =
            vec![(String::new(), None); tool_calls.len()];
        for (i, tc) in tool_calls.iter().enumerate() {
            if tc.name == REPORT_FINDING_TOOL_NAME {
                let msg = match evidence::parse_finding_from_report_args(&tc.arguments) {
                    Ok(f) => {
                        let ids = evidence::persist_findings(
                            &pool,
                            &chat_id,
                            None,
                            &[f.clone()],
                        )
                        .await?;
                        for id in &ids {
                            emit_chat_event(
                                &app_handle,
                                json!({
                                    "type": "FindingFound",
                                    "id": id,
                                    "title": f.title,
                                    "severity": f.severity,
                                    "description": f.description
                                }),
                            );
                        }
                        if let Err(e) = context::add_finding_to_battle_map(
                            &pool, &chat_id, &f.title, &f.severity,
                        ).await {
                            log::error!("[battle_map] finding update failed: {}", e);
                        }
                        "Tool report_finding: status=complete, output=Finding recorded.".to_string()
                    }
                    Err(e) => format!(
                        "Tool report_finding: status=error, output=Invalid arguments: {}",
                        e
                    ),
                };
                ordered_tool_results[i] = (msg, None);
            }
        }

        // Phase 1: Create all invocations and emit all ApprovalRequired events so the UI shows every tool card at once (real tools only).
        for (j, tc) in real_tool_calls.iter().enumerate() {
            let tool_call_index = real_indices[j];
            let invocation_id = Uuid::new_v4().to_string();
            let tool_name = tc.name.clone();
            let args_json = tc.arguments.clone();
            let (args_str, target) = parse_tool_args(&args_json);
            let args_str = args_str.clone();
            let target = target.clone();
            let risk_category = tools.risk_category(&tool_name);

            let tool_source = if tools.is_mcp_tool(&tool_name) {
                "mcp"
            } else {
                "local"
            };

            db::insert_tool_invocation(
                &pool,
                &invocation_id,
                &chat_id,
                &tool_name,
                tool_source,
                &args_str,
                &target,
                "pending",
                phase_name_ref,
                Some(&risk_category),
            )
            .await
            .map_err(|e| e.to_string())?;

            if governance::requires_approval(&execution_mode, &risk_category, multi_tool_batch) {
                let (tx, rx) = oneshot::channel();
                {
                    pending_approvals.write().unwrap().insert(invocation_id.clone(), tx);
                }
                emit_chat_event(
                    &app_handle,
                    json!({
                        "type": "ApprovalRequired",
                        "invocation_id": invocation_id,
                        "tool_name": tool_name,
                        "args": args_str,
                        "target": target,
                        "risk_category": risk_category
                    }),
                );
                pending_approval_rxs.push((
                    invocation_id,
                    rx,
                    tool_name,
                    target,
                    args_str,
                    risk_category,
                    j,
                ));
            } else {
                let approval_id = Uuid::new_v4().to_string();
                db::insert_approval(&pool, &approval_id, &invocation_id, "approved")
                    .await
                    .map_err(|e| e.to_string())?;
                let ts = db::now_ms();
                audit::write_audit(
                    &pool,
                    ts,
                    "approval",
                    &invocation_id,
                    "decision=approved",
                    None,
                )
                .await
                .map_err(|e| e.to_string())?;
                approved_to_run.push(ApprovedTool {
                    invocation_id: invocation_id.clone(),
                    tool_name: tool_name.clone(),
                    target: target.clone(),
                    args_str: args_str.clone(),
                    approval_id: approval_id.clone(),
                    risk_category: risk_category.clone(),
                    tool_call_index,
                });
            }
        }

        // Phase 2: Wait for each approval decision and record approved / denied / dry_run.
        for (
            invocation_id,
            rx,
            tool_name,
            target,
            args_str,
            risk_category,
            real_idx,
        ) in pending_approval_rxs
        {
            let decision = rx.await.map_err(|_| "Approval channel closed".to_string())?;
            let tool_call_index = real_indices[real_idx];

            let approval_id = Uuid::new_v4().to_string();
            db::insert_approval(&pool, &approval_id, &invocation_id, &decision)
                .await
                .map_err(|e| e.to_string())?;
            let ts = db::now_ms();
            audit::write_audit(
                &pool,
                ts,
                "approval",
                &invocation_id,
                &format!("decision={}", decision),
                None,
            )
            .await
            .map_err(|e| e.to_string())?;

            if decision == "denied" {
                db::update_tool_invocation_result(
                    &pool,
                    &invocation_id,
                    None,
                    None,
                    None,
                    "denied",
                    Some(&approval_id),
                )
                .await
                .map_err(|e| e.to_string())?;
                emit_chat_event(
                    &app_handle,
                    json!({ "type": "ToolDenied", "invocation_id": invocation_id, "reason": "User denied" }),
                );
                if let Err(e) = context::mark_tool_skipped(&pool, &chat_id, &tool_name).await {
                    log::error!("[battle_map] skipped-tool update failed: {}", e);
                }
                ordered_tool_results[tool_call_index] =
                    ("Tool denied by user.".to_string(), Some(invocation_id.clone()));
                continue;
            }

            if decision == "dry_run" {
                let cmd_preview = format!("{} {} {}", tool_name, args_str, target).trim().to_string();
                let skip_message = format!(
                    "Tool: {}\nCommand: {}\nStatus: SKIPPED by user\nInstruction: Do not request this tool again in this session. Continue your analysis without {} output.",
                    tool_name, cmd_preview, tool_name
                );
                db::update_tool_invocation_result(
                    &pool,
                    &invocation_id,
                    Some(&cmd_preview),
                    None,
                    None,
                    "complete",
                    Some(&approval_id),
                )
                .await
                .map_err(|e| e.to_string())?;
                emit_chat_event(
                    &app_handle,
                    json!({
                        "type": "ToolComplete",
                        "invocation_id": invocation_id,
                        "output": cmd_preview,
                        "duration_ms": 0
                    }),
                );
                if let Err(e) = context::mark_tool_skipped(&pool, &chat_id, &tool_name).await {
                    log::error!("[battle_map] skipped-tool update failed: {}", e);
                }
                ordered_tool_results[tool_call_index] = (skip_message, Some(invocation_id.clone()));
                continue;
            }

            approved_to_run.push(ApprovedTool {
                invocation_id: invocation_id.clone(),
                tool_name: tool_name.clone(),
                target: target.clone(),
                args_str: args_str.clone(),
                approval_id: approval_id.clone(),
                risk_category: risk_category.clone(),
                tool_call_index,
            });
        }

        // Run all approved tools in parallel; register (cancel_tx, handle) so cancel_tool_invocation can kill the child
        for item in &approved_to_run {
            db::update_tool_invocation_status(&pool, &item.invocation_id, "running")
                .await
                .map_err(|e| e.to_string())?;
            emit_chat_event(
                &app_handle,
                json!({
                    "type": "ToolRunning",
                    "invocation_id": item.invocation_id,
                    "tool_name": item.tool_name,
                    "args": item.args_str,
                    "risk_category": item.risk_category,
                    "phase_name": phase_name_ref
                }),
            );
            let inv_id = item.invocation_id.clone();
            let name = item.tool_name.clone();
            let tgt = item.target.clone();
            let args = item.args_str.clone();

            if tools.is_mcp_tool(&item.tool_name) {
                let server_name = tools
                    .get_mcp_server_name(&item.tool_name)
                    .ok_or_else(|| format!("MCP server name unknown for tool: {}", item.tool_name))?;
                let mcp_runtime_clone = mcp_runtime.clone();
                let timeout = tool_timeout_secs;
                let handle = tokio::spawn(async move {
                    let start = std::time::Instant::now();
                    let result = tokio::task::spawn_blocking({
                        let mcp_runtime = mcp_runtime_clone.clone();
                        let server_name = server_name.clone();
                        let name = name.clone();
                        let args = args.clone();
                        let tgt = tgt.clone();
                        move || {
                            let arguments = serde_json::json!({ "args": args, "target": tgt });
                            mcp_runtime.call_tool(&server_name, &name, arguments, timeout)
                        }
                    })
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r);
                    let duration_ms = start.elapsed().as_millis() as i64;
                    match result {
                        Ok(output) => tool_runner::ToolResult {
                            invocation_id: inv_id,
                            status: "complete".to_string(),
                            raw_output: Some(output),
                            exit_code: Some(0),
                            duration_ms: Some(duration_ms),
                        },
                        Err(e) => tool_runner::ToolResult {
                            invocation_id: inv_id,
                            status: "failed".to_string(),
                            raw_output: Some(e),
                            exit_code: None,
                            duration_ms: Some(duration_ms),
                        },
                    }
                });
                let pid_holder = Arc::new(std::sync::atomic::AtomicU32::new(0));
                running_handles
                    .lock()
                    .await
                    .insert(item.invocation_id.clone(), (None, handle, pid_holder));
            } else {
                let tools_clone = tools.clone();
                let (cancel_tx, cancel_rx) = oneshot::channel();
                let pid_holder = Arc::new(std::sync::atomic::AtomicU32::new(0));
                let pid_clone = pid_holder.clone();
                let timeout = tool_timeout_secs;
                let handle = tokio::spawn(async move {
                    tool_runner::run_local_with_cancel(&tools_clone, &inv_id, &name, &tgt, &args, timeout, pid_clone, cancel_rx).await
                });
                running_handles
                    .lock()
                    .await
                    .insert(item.invocation_id.clone(), (Some(cancel_tx), handle, pid_holder));
            }
        }

        let cancelled_result = |item: &ApprovedTool| tool_runner::ToolResult {
            invocation_id: item.invocation_id.clone(),
            status: "failed".to_string(),
            raw_output: Some("Cancelled by user.".to_string()),
            exit_code: None,
            duration_ms: None,
        };

        let mut run_results = Vec::with_capacity(approved_to_run.len());
        for item in &approved_to_run {
            let entry_opt = running_handles.lock().await.remove(&item.invocation_id);
            let result = match entry_opt {
                Some((_cancel_tx, handle, _pid)) => match handle.await {
                    Ok(r) => r,
                    Err(e) if e.is_cancelled() => cancelled_result(item),
                    Err(e) => return Err(e.to_string()),
                },
                None => cancelled_result(item),
            };
            run_results.push(result);
        }
        for (item, result) in approved_to_run.iter().zip(run_results) {
            let status = result.status.clone();
            let raw_output = result.raw_output.clone();
            let exit_code = result.exit_code;
            let duration_ms = result.duration_ms;
            let output_str = raw_output.as_deref().unwrap_or("");

            db::update_tool_invocation_result(
                &pool,
                &item.invocation_id,
                raw_output.as_deref(),
                exit_code,
                duration_ms,
                &status,
                Some(&item.approval_id),
            )
            .await
            .map_err(|e| e.to_string())?;

            emit_chat_event(
                &app_handle,
                json!({
                    "type": "ToolComplete",
                    "invocation_id": item.invocation_id,
                    "output": output_str,
                    "duration_ms": duration_ms,
                    "status": status,
                    "phase_name": phase_name_ref
                }),
            );

            if let Err(e) = context::update_battle_map(
                &pool, &chat_id, &item.tool_name, &item.target, output_str,
            ).await {
                log::error!("[battle_map] update failed for {}: {}", item.tool_name, e);
            }

            let tool_content = format!(
                "Tool {}: status={}, output={}",
                item.tool_name,
                status,
                output_str
            );
            ordered_tool_results[item.tool_call_index] =
                (tool_content, Some(item.invocation_id.clone()));
        }

        // Insert all tool result messages in tool_calls order (so LLM sees results in order).
        for (_i, (content, inv_id)) in ordered_tool_results.iter().enumerate() {
            if content.is_empty() {
                continue;
            }
            let tool_msg_id = Uuid::new_v4().to_string();
            db::insert_message(
                &pool,
                &tool_msg_id,
                &chat_id,
                "tool",
                content,
                inv_id.as_deref(),
            )
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

fn parse_tool_args(args_json: &str) -> (String, String) {
    let v: Value = serde_json::from_str(args_json).unwrap_or(Value::Null);
    let args = v.get("args").and_then(Value::as_str).unwrap_or("").to_string();
    let target = v.get("target").and_then(Value::as_str).unwrap_or("").to_string();
    (args, target)
}

/// Map registry category to UI phase label (lowercase, underscores for spaces).
fn category_to_phase_label(category: &str) -> &'static str {
    match category {
        "network" => "network",
        "web" => "web",
        "recon" => "recon",
        "tls" => "tls",
        "http" => "http",
        "brute_force" | "high_impact" => "security_assessment",
        "binary" => "binary",
        "cloud" => "cloud",
        _ => "security_assessment",
    }
}

/// Derive a phase label for the UI from tool names using registry category.
fn phase_from_tool_names(tools: &ToolRegistry, tool_names: &[String]) -> Option<String> {
    let mut labels: Vec<&'static str> = tool_names
        .iter()
        .filter_map(|name| {
            tools
                .get_category(name)
                .as_deref()
                .map(category_to_phase_label)
        })
        .collect();
    labels.dedup();
    if labels.len() == 1 {
        Some(labels[0].to_string())
    } else {
        Some("security_assessment".to_string())
    }
}
