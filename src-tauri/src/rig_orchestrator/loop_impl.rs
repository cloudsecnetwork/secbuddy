//! Rig-driven agent loop: build prompt → `CompletionModel::completion` →
//! split text/tool calls → dedupe → ConfidencePreview → approval gate →
//! dispatch to `tool_runner`/`mcp_client` → persist + emit `chat_event`s.

use crate::audit;
use crate::context;
use crate::db;
use crate::event_sink::EventSink;
use crate::evidence;
use crate::governance;
use crate::llm_client;
use crate::prompts;
use crate::rig_orchestrator::definitions::{
    build_tool_definitions, REPORT_FINDING_TOOL_NAME,
};
use crate::rig_orchestrator::events;
use crate::rig_orchestrator::provider::RigChatModel;
use crate::tool_registry::{ToolInfo, ToolRegistry};
use crate::tool_runner;
use rig::OneOrMany;
use rig::completion::message::{AssistantContent, Message};
use rig::completion::CompletionRequest;
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::oneshot;
use uuid::Uuid;

const MAX_ITERATIONS: u32 = 20;
const DEFAULT_MODEL: &str = "llama3.2";

#[derive(Clone, Debug)]
struct ParsedToolCall {
    #[allow(dead_code)]
    id: String,
    name: String,
    arguments: String,
}

fn debug_llm_enabled() -> bool {
    std::env::var("DEBUG_LLM")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes"))
        .unwrap_or(false)
}

fn emit(sink: &Arc<dyn EventSink>, payload: Value) {
    sink.emit_chat_event(payload);
}

/// Build the tool-context block embedded in the system prompt: tools by
/// category, risk classes, alternatives, and the alternative-on-failure rule.
fn build_tool_context(tools: &ToolRegistry) -> String {
    let list = tools.list_tools();
    let mut by_category: std::collections::HashMap<String, Vec<&ToolInfo>> =
        std::collections::HashMap::new();
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
    let alt_pairs: Vec<String> = list
        .iter()
        .filter_map(|t| {
            t.alternatives
                .as_ref()
                .map(|a| format!("{} → {}", t.name, a.join(", ")))
        })
        .collect();
    let alternatives_block = if alt_pairs.is_empty() {
        String::new()
    } else {
        format!("Alternatives: {}.", alt_pairs.join("; "))
    };
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

fn parse_tool_args(args_json: &str) -> (String, String) {
    let v: Value = serde_json::from_str(args_json).unwrap_or(Value::Null);
    let args = v.get("args").and_then(Value::as_str).unwrap_or("").to_string();
    let target = v.get("target").and_then(Value::as_str).unwrap_or("").to_string();
    (args, target)
}

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

/// Convert our DB-backed messages into Rig's `Message` enum. Tool-result
/// rows arrive as `role = "tool"` and are remapped to user-role messages
/// with a "[Tool result]\n..." prefix (handled in `context::format_message_for_llm`).
fn db_messages_to_rig(
    api_messages: &[Value],
) -> Result<(Option<String>, Vec<Message>), String> {
    let mut preamble: Option<String> = None;
    let mut history: Vec<Message> = Vec::new();
    for m in api_messages {
        let role = m.get("role").and_then(|v| v.as_str()).unwrap_or("");
        let content = m
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        match role {
            "system" => {
                preamble = Some(content);
            }
            "assistant" => {
                history.push(Message::assistant(content));
            }
            // `context::format_message_for_llm` maps `tool` rows to a user
            // message with a "[Tool result]" prefix, so by the time we see
            // it here the role is already "user" or "system".
            _ => {
                history.push(Message::user(content));
            }
        }
    }
    Ok((preamble, history))
}

/// Walk an `AssistantContent` stream and split text from tool calls.
/// `Reasoning` is dropped on the floor (it never leaves the model's chain of
/// thought; we don't surface it).
fn split_assistant_content(items: Vec<AssistantContent>) -> (String, Vec<ParsedToolCall>) {
    let mut text = String::new();
    let mut calls = Vec::new();
    for item in items {
        match item {
            AssistantContent::Text(t) => text.push_str(&t.text),
            AssistantContent::ToolCall(tc) => {
                let arguments = match &tc.function.arguments {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                calls.push(ParsedToolCall {
                    id: if tc.id.is_empty() {
                        Uuid::new_v4().to_string()
                    } else {
                        tc.id.clone()
                    },
                    name: tc.function.name.clone(),
                    arguments,
                });
            }
            AssistantContent::Reasoning(_) => {}
        }
    }
    (text, calls)
}

// All arguments are shared `AppState` handles passed straight through from the
// `send_message` Tauri command; collapsing them into a struct would just push
// the same fields into the call site without reducing complexity.
#[allow(clippy::too_many_arguments)]
pub async fn run_agent_loop(
    pool: SqlitePool,
    tools: Arc<ToolRegistry>,
    mcp_runtime: Arc<crate::mcp_client::McpRuntime>,
    pending_approvals: Arc<
        std::sync::RwLock<std::collections::HashMap<String, oneshot::Sender<String>>>,
    >,
    running_handles: Arc<
        tokio::sync::Mutex<std::collections::HashMap<String, crate::RunningToolEntry>>,
    >,
    sink: Arc<dyn EventSink>,
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
    let model_name = db::get_setting(&pool, "llm_model")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());

    let chat_model = RigChatModel::from_config(&config, &model_name)?;

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
        let system_content =
            prompts::build_system_prompt(&chat_mode, &tool_context, Some(&battle_map_block));
        let api_messages =
            context::build_api_messages(&system_content, &messages, context::WINDOW_SIZE);

        let (preamble, mut rig_history) = db_messages_to_rig(&api_messages)?;
        // Rig requires a final prompt message; pop the last one to use as the
        // builder's prompt so we don't double-add it.
        let prompt_msg = rig_history.pop().unwrap_or_else(|| Message::user(""));

        let tool_defs = build_tool_definitions(&tools);

        if debug_llm_enabled() {
            log::debug!(
                "[DEBUG_LLM/rig] === REQUEST === model={} preamble?={} history_len={} tools={}",
                model_name,
                preamble.is_some(),
                rig_history.len(),
                tool_defs.len()
            );
        }

        let request = CompletionRequest {
            preamble,
            chat_history: OneOrMany::many([rig_history, vec![prompt_msg]].concat())
                .map_err(|e| format!("rig OneOrMany build failed: {}", e))?,
            documents: Vec::new(),
            tools: tool_defs,
            temperature: None,
            max_tokens: None,
            tool_choice: None,
            additional_params: None,
        };

        let assistant_items = chat_model
            .complete(request)
            .await
            .map_err(|e| format!("rig completion failed: {}", e))?;
        let (text, tool_calls) = split_assistant_content(assistant_items);

        if debug_llm_enabled() {
            log::debug!("[DEBUG_LLM/rig] === RESPONSE ===");
            log::debug!("content: {}", text);
            log::debug!("tool_calls: {}", tool_calls.len());
            for (i, tc) in tool_calls.iter().enumerate() {
                log::debug!("  [{}] {} args={}", i, tc.name, tc.arguments);
            }
        }

        if !text.is_empty() {
            for chunk in text.chars().collect::<Vec<_>>().chunks(64) {
                let s: String = chunk.iter().collect();
                emit(&sink, events::message_chunk(&s));
            }
            let assistant_msg_id = Uuid::new_v4().to_string();
            db::insert_message(&pool, &assistant_msg_id, &chat_id, "assistant", &text, None)
                .await
                .map_err(|e| e.to_string())?;
            emit(&sink, events::message_complete(&assistant_msg_id));
        }

        if tool_calls.is_empty() {
            break;
        }

        let mut seen: HashSet<(String, String)> = HashSet::new();
        let tool_calls: Vec<ParsedToolCall> = tool_calls
            .into_iter()
            .filter(|tc| seen.insert((tc.name.clone(), tc.arguments.clone())))
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
            &real_tool_calls
                .iter()
                .map(|t| t.name.clone())
                .collect::<Vec<_>>(),
        );
        let phase_name_ref = phase_name.as_deref();

        let multi_tool_batch = real_tool_calls.len() > 1;

        let execution_plan: Vec<Value> = real_tool_calls
            .iter()
            .map(|tc| {
                let (args_str, target) = parse_tool_args(&tc.arguments);
                let risk_cat = tools.risk_category(&tc.name);
                let requires_approval =
                    governance::requires_approval(&execution_mode, &risk_cat, multi_tool_batch);
                events::execution_plan_entry(&tc.name, &args_str, &target, &risk_cat, requires_approval)
            })
            .collect();

        emit(
            &sink,
            events::confidence_preview(&text, &text, tool_calls.len(), execution_plan),
        );

        struct ApprovedTool {
            invocation_id: String,
            tool_name: String,
            target: String,
            args_str: String,
            approval_id: String,
            risk_category: String,
            tool_call_index: usize,
        }
        let mut approved_to_run: Vec<ApprovedTool> = Vec::new();

        // (invocation_id, decision_rx, tool_name, target, args_str, risk_category, real_idx)
        type PendingApprovalRx = (
            String,
            oneshot::Receiver<String>,
            String,
            String,
            String,
            String,
            usize,
        );
        let mut pending_approval_rxs: Vec<PendingApprovalRx> = Vec::new();

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
                            std::slice::from_ref(&f),
                        )
                        .await?;
                        for id in &ids {
                            emit(
                                &sink,
                                events::finding_found(id, &f.title, &f.severity, &f.description),
                            );
                        }
                        if let Err(e) = context::add_finding_to_battle_map(
                            &pool, &chat_id, &f.title, &f.severity,
                        )
                        .await
                        {
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

        for (j, tc) in real_tool_calls.iter().enumerate() {
            let tool_call_index = real_indices[j];
            let invocation_id = Uuid::new_v4().to_string();
            let tool_name = tc.name.clone();
            let args_json = tc.arguments.clone();
            let (args_str, target) = parse_tool_args(&args_json);
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
                    pending_approvals
                        .write()
                        .unwrap()
                        .insert(invocation_id.clone(), tx);
                }
                emit(
                    &sink,
                    events::approval_required(
                        &invocation_id,
                        &tool_name,
                        &args_str,
                        &target,
                        &risk_category,
                    ),
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
                    invocation_id,
                    tool_name,
                    target,
                    args_str,
                    approval_id,
                    risk_category,
                    tool_call_index,
                });
            }
        }

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
                emit(&sink, events::tool_denied(&invocation_id, "User denied"));
                if let Err(e) = context::mark_tool_skipped(&pool, &chat_id, &tool_name).await {
                    log::error!("[battle_map] skipped-tool update failed: {}", e);
                }
                ordered_tool_results[tool_call_index] =
                    ("Tool denied by user.".to_string(), Some(invocation_id.clone()));
                continue;
            }

            if decision == "dry_run" {
                let cmd_preview =
                    format!("{} {} {}", tool_name, args_str, target).trim().to_string();
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
                emit(
                    &sink,
                    events::tool_complete_simple(&invocation_id, &cmd_preview, Some(0)),
                );
                if let Err(e) = context::mark_tool_skipped(&pool, &chat_id, &tool_name).await {
                    log::error!("[battle_map] skipped-tool update failed: {}", e);
                }
                ordered_tool_results[tool_call_index] = (skip_message, Some(invocation_id.clone()));
                continue;
            }

            approved_to_run.push(ApprovedTool {
                invocation_id,
                tool_name,
                target,
                args_str,
                approval_id,
                risk_category,
                tool_call_index,
            });
        }

        for item in &approved_to_run {
            db::update_tool_invocation_status(&pool, &item.invocation_id, "running")
                .await
                .map_err(|e| e.to_string())?;
            emit(
                &sink,
                events::tool_running(
                    &item.invocation_id,
                    &item.tool_name,
                    &item.args_str,
                    &item.risk_category,
                    phase_name_ref,
                ),
            );
            let inv_id = item.invocation_id.clone();
            let name = item.tool_name.clone();
            let tgt = item.target.clone();
            let args = item.args_str.clone();

            if tools.is_mcp_tool(&item.tool_name) {
                let server_name = tools
                    .get_mcp_server_name(&item.tool_name)
                    .ok_or_else(|| {
                        format!("MCP server name unknown for tool: {}", item.tool_name)
                    })?;
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
                            let arguments = json!({ "args": args, "target": tgt });
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
                    tool_runner::run_local_with_cancel(
                        &tools_clone,
                        &inv_id,
                        &name,
                        &tgt,
                        &args,
                        timeout,
                        pid_clone,
                        cancel_rx,
                    )
                    .await
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

            emit(
                &sink,
                events::tool_complete(
                    &item.invocation_id,
                    output_str,
                    duration_ms,
                    Some(&status),
                    phase_name_ref,
                ),
            );

            if let Err(e) =
                context::update_battle_map(&pool, &chat_id, &item.tool_name, &item.target, output_str)
                    .await
            {
                log::error!("[battle_map] update failed for {}: {}", item.tool_name, e);
            }

            let tool_content = format!(
                "Tool {}: status={}, output={}",
                item.tool_name, status, output_str
            );
            ordered_tool_results[item.tool_call_index] =
                (tool_content, Some(item.invocation_id.clone()));
        }

        for (content, inv_id) in ordered_tool_results.iter() {
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

