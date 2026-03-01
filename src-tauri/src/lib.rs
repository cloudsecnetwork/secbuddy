mod agent_loop;
mod audit;
mod context;
mod db;
mod evidence;
mod governance;
mod mcp_client;
mod llm_client;
mod prompts;
mod tool_registry;
mod tool_runner;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
// use sha2::Digest; // only used by attach_file_to_chat (commented out)
use tokio::sync::oneshot;
use tauri::{Emitter, Manager};
use tool_registry::ToolRegistry;
use tool_runner::ToolResult;

/// (cancel_sender, join_handle, child_pid). PID is 0 until the child spawns.
pub type RunningToolEntry = (
    Option<oneshot::Sender<()>>,
    tokio::task::JoinHandle<ToolResult>,
    Arc<std::sync::atomic::AtomicU32>,
);

pub struct AppState {
    pub db: sqlx::sqlite::SqlitePool,
    pub app_data_dir: PathBuf,
    pub app_handle: tauri::AppHandle,
    pub tools: Arc<ToolRegistry>,
    pub mcp_runtime: Arc<mcp_client::McpRuntime>,
    /// invocation_id -> sender to send decision (approved | denied | dry_run)
    pub pending_approvals: Arc<std::sync::RwLock<HashMap<String, oneshot::Sender<String>>>>,
    /// invocation_id -> (cancel_tx, JoinHandle, pid). Sending on cancel_tx kills the tool's child process.
    pub running_tool_handles: Arc<tokio::sync::Mutex<HashMap<String, RunningToolEntry>>>,
    /// chat_id -> JoinHandle for the agent loop task. Used to abort the entire session.
    pub active_agent_loops: Arc<tokio::sync::Mutex<HashMap<String, tauri::async_runtime::JoinHandle<()>>>>,
}

#[tauri::command]
fn get_app_data_dir(state: tauri::State<AppState>) -> Result<String, String> {
    state
        .app_data_dir
        .to_str()
        .map(String::from)
        .ok_or_else(|| "Invalid app data path".to_string())
}

#[tauri::command]
fn save_setting(
    state: tauri::State<AppState>,
    key: String,
    value: String,
) -> Result<(), String> {
    tauri::async_runtime::block_on(async move {
        db::save_setting(&state.db, &key, &value)
            .await
            .map_err(|e| e.to_string())
    })
}

#[tauri::command]
fn get_setting(state: tauri::State<AppState>, key: String) -> Result<Option<String>, String> {
    tauri::async_runtime::block_on(async move {
        db::get_setting(&state.db, &key)
            .await
            .map_err(|e| e.to_string())
    })
}

#[tauri::command]
fn delete_setting(state: tauri::State<AppState>, key: String) -> Result<(), String> {
    tauri::async_runtime::block_on(async move {
        db::delete_setting(&state.db, &key)
            .await
            .map_err(|e| e.to_string())
    })
}

#[tauri::command]
fn list_tools(state: tauri::State<AppState>) -> Result<Vec<tool_registry::ToolInfo>, String> {
    Ok(state.tools.list_tools())
}

#[tauri::command]
fn refresh_local_tools(state: tauri::State<AppState>) -> Result<(), String> {
    state.tools.refresh_detection_embedded()
}

#[tauri::command]
fn test_connection(state: tauri::State<AppState>) -> Result<(), String> {
    let db = state.db.clone();
    tauri::async_runtime::block_on(async move {
        let config = llm_client::get_llm_config_from_pool(&db).await?;
        llm_client::test_connection(&config).await
    })
}

#[tauri::command]
fn get_mcp_config(state: tauri::State<AppState>) -> Result<mcp_client::McpConfig, String> {
    let pool = state.db.clone();
    let app_data_dir = state.app_data_dir.clone();
    tauri::async_runtime::block_on(async move {
        mcp_client::load_mcp_config(&pool, &app_data_dir).await
    })
}

#[tauri::command]
fn save_mcp_config(
    state: tauri::State<AppState>,
    config: mcp_client::McpConfig,
    write_file: bool,
) -> Result<(), String> {
    let pool = state.db.clone();
    let app_data_dir = state.app_data_dir.clone();
    let tools = state.tools.clone();
    let mcp_runtime = state.mcp_runtime.clone();
    tauri::async_runtime::block_on(async move {
        mcp_client::save_mcp_config(&pool, &app_data_dir, &config, write_file).await?;
        mcp_runtime.reload(&pool, &app_data_dir, &config, &tools)?;
        Ok(())
    })
}

#[tauri::command]
fn reload_mcp_servers(state: tauri::State<AppState>) -> Result<(), String> {
    let pool = state.db.clone();
    let app_data_dir = state.app_data_dir.clone();
    let tools = state.tools.clone();
    let mcp_runtime = state.mcp_runtime.clone();
    tauri::async_runtime::block_on(async move {
        mcp_client::load_and_reload(&pool, &app_data_dir, &tools, &mcp_runtime).await
    })
}

#[tauri::command]
fn test_mcp_server(entry: mcp_client::McpServerEntry) -> Result<u32, String> {
    mcp_client::test_mcp_server(&entry)
}

// Attach files / artifacts in chat deferred to later phase (can be hefty).
// #[tauri::command]
// fn attach_file_to_chat(
//     state: tauri::State<AppState>,
//     chat_id: String,
//     file_path: String,
// ) -> Result<String, String> {
//     let path = std::path::Path::new(&file_path);
//     let filename = path
//         .file_name()
//         .and_then(|p| p.to_str())
//         .ok_or("Invalid file path")?
//         .to_string();
//     let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
//     let size_bytes = meta.len() as i64;
//     let content = std::fs::read(path).map_err(|e| e.to_string())?;
//     let hash = sha2::Sha256::digest(&content);
//     let sha256 = hex::encode(hash);
//     let file_type = path
//         .extension()
//         .and_then(|e| e.to_str())
//         .unwrap_or("bin")
//         .to_string();
//     let id = uuid::Uuid::new_v4().to_string();
//     let uploads_dir = state.app_data_dir.join("uploads").join(&chat_id);
//     std::fs::create_dir_all(&uploads_dir).map_err(|e| e.to_string())?;
//     let dest_path = uploads_dir.join(format!("{}_{}", id, filename));
//     std::fs::write(&dest_path, &content).map_err(|e| e.to_string())?;
//     let dest_str = dest_path.to_str().ok_or("Invalid path")?.to_string();
//     let pool = state.db.clone();
//     tauri::async_runtime::block_on(async move {
//         db::insert_file_attachment(
//             &pool,
//             &id,
//             &chat_id,
//             &filename,
//             &file_type,
//             &dest_str,
//             size_bytes,
//             &sha256,
//         )
//         .await
//         .map_err(|e| e.to_string())?;
//         Ok(id)
//     })
// }

#[tauri::command]
fn send_message(
    state: tauri::State<AppState>,
    app_handle: tauri::AppHandle,
    chat_id: String,
    content: String,
    // attachment_ids: Option<Vec<String>> — attach files deferred to later phase
) -> Result<(), String> {
    let pool = state.db.clone();
    let tools = state.tools.clone();
    let pending_approvals = state.pending_approvals.clone();
    let app_handle_emit = app_handle.clone();
    // let content_with_attachments = if let Some(ids) = &attachment_ids {
    //     let pool_att = pool.clone();
    //     let cid = chat_id.clone();
    //     tauri::async_runtime::block_on(async move {
    //         let mut msg = content;
    //         for id in ids {
    //             let rows = sqlx::query_as::<_, (String, String)>(
    //                 "SELECT filename, file_path FROM file_attachments WHERE id = ? AND chat_id = ?",
    //             )
    //             .bind(id)
    //             .bind(&cid)
    //             .fetch_all(&pool_att)
    //             .await
    //             .map_err(|e| e.to_string())?;
    //             if let Some((name, path)) = rows.into_iter().next() {
    //                 let data = std::fs::read_to_string(&path).unwrap_or_else(|_| "[could not read file]".to_string());
    //                 let preview = if data.len() > 8000 { format!("{}...", &data[..8000]) } else { data };
    //                 msg.push_str(&format!("\n\n[Attached: {}]\n{}", name, preview));
    //             }
    //         }
    //         Ok::<String, String>(msg)
    //     })?
    // } else {
    //     content
    // };
    let running_handles = state.running_tool_handles.clone();
    let active_loops = state.active_agent_loops.clone();
    let mcp_runtime = state.mcp_runtime.clone();
    let pool_err = pool.clone();
    let chat_id_err = chat_id.clone();
    let chat_id_loop = chat_id.clone();
    let chat_id_key = chat_id.clone();
    let active_loops_cleanup = active_loops.clone();
    let handle = tauri::async_runtime::spawn(async move {
        if let Err(e) = agent_loop::run_agent_loop(pool, tools, mcp_runtime, pending_approvals, running_handles, app_handle, chat_id, content).await {
            let error_msg_id = uuid::Uuid::new_v4().to_string();
            let error_content = format!("[error] {}", e);
            let _ = db::insert_message(&pool_err, &error_msg_id, &chat_id_err, "assistant", &error_content, None).await;
            let _ = app_handle_emit.emit(
                "chat_event",
                serde_json::json!({ "type": "Error", "message": e, "message_id": error_msg_id }),
            );
        }
        active_loops_cleanup.lock().await.remove(&chat_id_loop);
    });
    active_loops.blocking_lock().insert(chat_id_key, handle);
    Ok(())
}

#[tauri::command]
fn record_approval_and_execute(
    state: tauri::State<AppState>,
    invocation_id: String,
    decision: String,
) -> Result<(), String> {
    let tx = state
        .pending_approvals
        .write()
        .unwrap()
        .remove(&invocation_id)
        .ok_or_else(|| "No pending approval for this invocation".to_string())?;
    tx.send(decision).map_err(|_| "Failed to send decision".to_string())?;
    Ok(())
}

#[tauri::command]
async fn cancel_tool_invocation(
    state: tauri::State<'_, AppState>,
    invocation_id: String,
) -> Result<(), String> {
    const STOPPED_MSG: &str = "Stopped by user.";
    let pool = state.db.clone();

    // 1. Resolve chat_id so we can stop the entire session.
    let chat_id = db::get_chat_id_for_invocation(&pool, &invocation_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Invocation not found".to_string())?;

    // 2. Abort the agent loop for this chat (stops LLM calls, approval waits, everything).
    if let Some(loop_handle) = state.active_agent_loops.lock().await.remove(&chat_id) {
        loop_handle.abort();
    }

    // 3. Kill all running tool processes and drain their handles.
    {
        let mut handles = state.running_tool_handles.lock().await;
        let keys: Vec<String> = handles.keys().cloned().collect();
        for key in keys {
            if let Some((cancel_tx, handle, pid_holder)) = handles.remove(&key) {
                if let Some(tx) = cancel_tx {
                    let _ = tx.send(());
                }
                let pid = pid_holder.load(std::sync::atomic::Ordering::SeqCst);
                if pid != 0 {
                    tool_runner::kill_process_tree(pid);
                }
                handle.abort();
            }
        }
    }

    // 4. Drop all pending approval channels (unblocks any waiting receivers with an error).
    {
        state.pending_approvals.write().unwrap().clear();
    }

    // 5. Mark all pending/running invocations for this chat as failed in DB.
    let stopped_ids = db::stop_all_active_for_chat(&pool, &chat_id, STOPPED_MSG)
        .await
        .map_err(|e| e.to_string())?;

    // 6. Emit ToolComplete for every stopped invocation so the UI updates each card.
    for id in &stopped_ids {
        let _ = state.app_handle.emit(
            "chat_event",
            serde_json::json!({
                "type": "ToolComplete",
                "invocation_id": id,
                "output": STOPPED_MSG,
                "duration_ms": null,
                "status": "failed",
                "phase_name": null
            }),
        );
    }

    // 7. Tell the UI the agent has stopped so it clears the waiting/loading state.
    let _ = state.app_handle.emit(
        "chat_event",
        serde_json::json!({ "type": "AgentStopped" }),
    );

    Ok(())
}

#[tauri::command]
fn create_chat(state: tauri::State<AppState>, title: String, mode: Option<String>) -> Result<String, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let mode_value = mode.as_deref().unwrap_or("recon");
    tauri::async_runtime::block_on(async move {
        db::create_chat(&state.db, &id, &title, mode_value).await.map_err(|e| e.to_string())?;
        Ok(id)
    })
}

#[tauri::command]
fn list_chats(state: tauri::State<AppState>) -> Result<Vec<(String, String, String, i64)>, String> {
    tauri::async_runtime::block_on(async move {
        db::list_chats(&state.db, 50).await.map_err(|e| e.to_string())
    })
}

#[tauri::command]
fn get_chat_info(
    state: tauri::State<AppState>,
    chat_id: String,
) -> Result<(Option<String>, Option<String>), String> {
    tauri::async_runtime::block_on(async move {
        let title = db::get_chat_title(&state.db, &chat_id).await.map_err(|e| e.to_string())?;
        let mode = db::get_chat_mode(&state.db, &chat_id).await.map_err(|e| e.to_string())?;
        Ok((title, mode))
    })
}

#[tauri::command]
fn get_chat_history(
    state: tauri::State<AppState>,
    chat_id: String,
) -> Result<Vec<(String, String, String, String, Option<String>, i64)>, String> {
    tauri::async_runtime::block_on(async move {
        db::get_chat_messages(&state.db, &chat_id).await.map_err(|e| e.to_string())
    })
}

#[tauri::command]
fn get_tool_invocations_for_chat(
    state: tauri::State<AppState>,
    chat_id: String,
) -> Result<
    Vec<(
        String,
        String,
        String,
        String,
        String,
        String,
        Option<String>,
        Option<i32>,
        Option<i64>,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        i64,
    )>,
    String,
> {
    tauri::async_runtime::block_on(async move {
        db::fix_stale_running_for_chat(&state.db, &chat_id)
            .await
            .map_err(|e| e.to_string())?;
        db::get_tool_invocations_for_chat(&state.db, &chat_id)
            .await
            .map_err(|e| e.to_string())
    })
}

#[tauri::command]
fn get_findings_for_chat(
    state: tauri::State<AppState>,
    chat_id: String,
) -> Result<
    Vec<(
        String,
        String,
        Option<String>,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        i64,
    )>,
    String,
> {
    tauri::async_runtime::block_on(async move {
        db::get_findings_for_chat(&state.db, &chat_id)
            .await
            .map_err(|e| e.to_string())
    })
}

#[tauri::command]
fn delete_chat(state: tauri::State<AppState>, chat_id: String) -> Result<(), String> {
    tauri::async_runtime::block_on(async move {
        db::delete_chat(&state.db, &chat_id).await.map_err(|e| e.to_string())
    })
}

#[tauri::command]
fn clear_all_chat_history(state: tauri::State<AppState>) -> Result<(), String> {
    tauri::async_runtime::block_on(async move {
        db::delete_all_chats(&state.db).await.map_err(|e| e.to_string())
    })
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[tauri::command]
fn export_chat_html(state: tauri::State<AppState>, chat_id: String) -> Result<String, String> {
    let pool = state.db.clone();
    tauri::async_runtime::block_on(async move {
        let title = db::get_chat_title(&pool, &chat_id)
            .await
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| "SecBuddy Chat".to_string());
        let messages = db::get_chat_messages(&pool, &chat_id)
            .await
            .map_err(|e| e.to_string())?;
        let invocations = db::get_tool_invocations_for_chat(&pool, &chat_id)
            .await
            .map_err(|e| e.to_string())?;
        let findings = db::get_findings_for_chat(&pool, &chat_id)
            .await
            .map_err(|e| e.to_string())?;

        let mut inv_map: std::collections::HashMap<String, (String, String, Option<String>, String)> =
            std::collections::HashMap::new();
        for inv in &invocations {
            inv_map.insert(
                inv.0.clone(),
                (
                    inv.2.clone(),
                    inv.4.clone(),
                    inv.12.clone(),
                    inv.6.clone().unwrap_or_default(),
                ),
            );
        }

        let mut html = format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="UTF-8"/><title>{}</title>
<style>
body{{font-family:system-ui,sans-serif;margin:1rem;max-width:800px}}
h1{{font-size:1.25rem}}
.msg{{margin:0.5rem 0;padding:0.5rem;border-radius:6px}}
.user{{background:#e5e7eb}}
.assistant{{background:#f3f4f6}}
.tool{{background:#fef3c7;font-size:0.9rem}}
.tool pre{{white-space:pre-wrap;overflow-x:auto}}
.finding{{margin:0.5rem 0;padding:0.5rem;border-left:4px solid #6b7280}}
.critical{{border-color:#dc2626}}
.high{{border-color:#ea580c}}
.medium{{border-color:#ca8a04}}
.low{{border-color:#2563eb}}
.info{{border-color:#6b7280}}
</style>
</head>
<body><h1>{}</h1>
"#,
            escape_html(&title),
            escape_html(&title)
        );

        for msg in &messages {
            let (role, content) = (&msg.2, &msg.3);
            let class = match role.as_str() {
                "user" => "user",
                "assistant" => "assistant",
                _ => "tool",
            };
            html.push_str(&format!(
                "<div class=\"msg {}\"><strong>{}</strong><br/>\n<pre>{}</pre></div>\n",
                class,
                escape_html(role),
                escape_html(content)
            ));
            if let Some(ref inv_id) = msg.4 {
                if let Some((name, params, phase, output)) = inv_map.get(inv_id) {
                    html.push_str(&format!(
                        "<div class=\"tool\"><strong>Tool: {}</strong>",
                        escape_html(name)
                    ));
                    if phase.as_deref().unwrap_or("").len() > 0 {
                        html.push_str(&format!(" [{}]", escape_html(phase.as_deref().unwrap_or(""))));
                    }
                    html.push_str(&format!("<br/>Args: {}<br/><pre>{}</pre></div>\n", escape_html(params), escape_html(output)));
                }
            }
        }

        for f in &findings {
            let sev = &f.4;
            let class = if sev.eq_ignore_ascii_case("critical") {
                "critical"
            } else if sev.eq_ignore_ascii_case("high") {
                "high"
            } else if sev.eq_ignore_ascii_case("medium") {
                "medium"
            } else if sev.eq_ignore_ascii_case("low") {
                "low"
            } else {
                "info"
            };
            html.push_str(&format!(
                "<div class=\"finding {}\"><strong>{}: {}</strong><p>{}</p></div>\n",
                class,
                escape_html(sev),
                escape_html(&f.3),
                escape_html(&f.5)
            ));
        }

        html.push_str("</body></html>");
        Ok(html)
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if std::env::var("DEBUG_LLM")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes"))
        .unwrap_or(false)
    {
        std::env::set_var("RUST_LOG", "secbuddy_lib=debug");
    }
    let _ = env_logger::try_init();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data_dir = app
                .handle()
                .path()
                .app_data_dir()
                .map_err(|e| e.to_string())?;
            std::fs::create_dir_all(&app_data_dir).map_err(|e| e.to_string())?;
            let db_path = app_data_dir.join("secbuddy.db");
            // One-time migration: carry over existing SecProof DB for upgraders
            let legacy_db = app_data_dir.join("secproof.db");
            if !db_path.exists() && legacy_db.exists() {
                let _ = std::fs::copy(&legacy_db, &db_path);
            }
            let pool = tauri::async_runtime::block_on(async move {
                db::init_db(&db_path).await.map_err(|e| e.to_string())
            })?;
            let tools = Arc::new(ToolRegistry::new());
            tools
                .load_local_tools_from_str(include_str!("../tools.json"))
                .map_err(|e| e.to_string())?;
            let mcp_runtime = Arc::new(mcp_client::McpRuntime::new());
            let _ = tauri::async_runtime::block_on(mcp_client::load_and_reload(
                &pool,
                &app_data_dir,
                &tools,
                &mcp_runtime,
            ));
            app.manage(AppState {
                db: pool,
                app_data_dir,
                app_handle: app.handle().clone(),
                tools,
                mcp_runtime,
                pending_approvals: Arc::new(std::sync::RwLock::new(HashMap::new())),
                running_tool_handles: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
                active_agent_loops: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
        get_app_data_dir,
        save_setting,
        get_setting,
        delete_setting,
        list_tools,
        refresh_local_tools,
        test_connection,
        get_mcp_config,
        save_mcp_config,
        reload_mcp_servers,
        test_mcp_server,
        send_message,
        // attach_file_to_chat,
        create_chat,
        list_chats,
        get_chat_info,
        get_chat_history,
        get_tool_invocations_for_chat,
        get_findings_for_chat,
        delete_chat,
        clear_all_chat_history,
        export_chat_html,
        record_approval_and_execute,
        cancel_tool_invocation
    ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app_handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                if let Some(state) = app_handle.try_state::<AppState>() {
                    state.mcp_runtime.stop_all();
                }
            }
        });
}
