//! Execute local tools via argv only (no shell interpolation). Timeout and capture output.

use crate::tool_registry::ToolRegistry;
use std::process::Stdio;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::oneshot;

/// Parse a single string of arguments into a vector. Handles quoted segments (double-quote).
/// Simple: split by space, but treat "..." as one token.
pub fn parse_args_string(args_str: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    for c in args_str.chars() {
        match (c, in_quote) {
            ('"', false) => in_quote = true,
            ('"', true) => {
                in_quote = false;
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
            (' ' | '\t', false) => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
            (_, true) => current.push(c),
            (_, false) => current.push(c),
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

pub struct ToolResult {
    #[allow(dead_code)]
    pub invocation_id: String,
    pub status: String, // "complete" | "failed" | "denied" | "dry_run"
    pub raw_output: Option<String>,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<i64>,
}

pub const DEFAULT_TOOL_TIMEOUT_SECS: u64 = 900;

fn cancelled_result(invocation_id: &str) -> ToolResult {
    ToolResult {
        invocation_id: invocation_id.to_string(),
        status: "failed".to_string(),
        raw_output: Some("Cancelled by user.".to_string()),
        exit_code: None,
        duration_ms: None,
    }
}

/// Run a local tool by name. Publishes the child PID to `pid_out` so callers can force-kill it.
/// The cancel channel triggers a graceful stop; `kill_process_tree` handles the hard kill.
pub async fn run_local_with_cancel(
    registry: &ToolRegistry,
    invocation_id: &str,
    tool_name: &str,
    target: &str,
    args: &str,
    timeout_secs: u64,
    pid_out: Arc<AtomicU32>,
    mut cancel_rx: oneshot::Receiver<()>,
) -> ToolResult {
    let path = match registry.resolve_local_path(tool_name) {
        Some(p) => p,
        None => {
            return ToolResult {
                invocation_id: invocation_id.to_string(),
                status: "failed".to_string(),
                raw_output: Some("Tool not available or not found.".to_string()),
                exit_code: None,
                duration_ms: None,
            };
        }
    };

    let mut argv = parse_args_string(args);
    if !target.is_empty() {
        argv.push(target.to_string());
    }

    let child = match Command::new(&path)
        .args(&argv)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            return ToolResult {
                invocation_id: invocation_id.to_string(),
                status: "failed".to_string(),
                raw_output: Some(e.to_string()),
                exit_code: None,
                duration_ms: None,
            };
        }
    };

    if let Some(pid) = child.id() {
        pid_out.store(pid, Ordering::SeqCst);
    }

    let start = std::time::Instant::now();
    // cancel_rx fires when the user clicks Stop; the caller kills the process tree by PID.
    // Dropping the wait branch drops the child, and kill_on_drop(true) cleans up from this side.
    let output = tokio::select! {
        result = tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            child.wait_with_output(),
        ) => match result {
            Ok(Ok(out)) => Ok(out),
            Ok(Err(e)) => Err(e.to_string()),
            Err(_) => Err(format!("Tool timed out after {} seconds", timeout_secs)),
        },
        _ = &mut cancel_rx => {
            return cancelled_result(invocation_id);
        }
    };

    let duration_ms = start.elapsed().as_millis() as i64;
    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
            let raw_output = if stderr.is_empty() {
                stdout
            } else {
                format!("{stdout}\n{stderr}")
            };
            let status = if out.status.success() {
                "complete"
            } else {
                "failed"
            };
            let exit_code = out.status.code();
            ToolResult {
                invocation_id: invocation_id.to_string(),
                status: status.to_string(),
                raw_output: Some(raw_output),
                exit_code,
                duration_ms: Some(duration_ms),
            }
        }
        Err(e) => ToolResult {
            invocation_id: invocation_id.to_string(),
            status: "failed".to_string(),
            raw_output: Some(e),
            exit_code: None,
            duration_ms: Some(duration_ms),
        },
    }
}

/// Force-kill a process and its children by PID.
/// On Windows uses `taskkill /F /T /PID` (kills entire process tree).
/// On Unix uses `kill -9` on the process.
pub fn kill_process_tree(pid: u32) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
    #[cfg(not(target_os = "windows"))]
    {
        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
        }
    }
}
