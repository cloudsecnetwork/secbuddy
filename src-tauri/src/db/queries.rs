use sqlx::SqlitePool;
use std::time::{SystemTime, UNIX_EPOCH};

/// Current time in milliseconds since Unix epoch. Uses 0 on clock skew to avoid panic.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ---- Chats ----
pub async fn create_chat(pool: &SqlitePool, id: &str, title: &str, mode: &str) -> Result<(), sqlx::Error> {
    let now = now_ms();
    sqlx::query(
        "INSERT INTO chats (id, title, mode, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(title)
    .bind(mode)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_chat_title(pool: &SqlitePool, chat_id: &str) -> Result<Option<String>, sqlx::Error> {
    let row = sqlx::query_scalar::<_, String>("SELECT title FROM chats WHERE id = ?")
        .bind(chat_id)
        .fetch_optional(pool)
        .await?;
    Ok(row)
}

pub async fn list_chats(pool: &SqlitePool, limit: i32) -> Result<Vec<(String, String, String, i64)>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (String, String, String, i64)>(
        "SELECT id, title, mode, updated_at FROM chats ORDER BY updated_at DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[allow(dead_code)]
pub async fn get_chat_mode(pool: &SqlitePool, chat_id: &str) -> Result<Option<String>, sqlx::Error> {
    let row = sqlx::query_scalar::<_, String>("SELECT mode FROM chats WHERE id = ?")
        .bind(chat_id)
        .fetch_optional(pool)
        .await?;
    Ok(row)
}

pub async fn update_chat_updated(pool: &SqlitePool, chat_id: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE chats SET updated_at = ? WHERE id = ?")
        .bind(now_ms())
        .bind(chat_id)
        .execute(pool)
        .await?;
    Ok(())
}

#[allow(dead_code)]
pub async fn update_chat_title(pool: &SqlitePool, chat_id: &str, title: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE chats SET title = ?, updated_at = ? WHERE id = ?")
        .bind(title)
        .bind(now_ms())
        .bind(chat_id)
        .execute(pool)
        .await?;
    Ok(())
}

#[allow(dead_code)]
pub async fn update_chat_mode(pool: &SqlitePool, chat_id: &str, mode: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE chats SET mode = ?, updated_at = ? WHERE id = ?")
        .bind(mode)
        .bind(now_ms())
        .bind(chat_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_chat(pool: &SqlitePool, chat_id: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM chats WHERE id = ?")
        .bind(chat_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Permanently delete all chats and their messages, tool invocations, findings, and attachments (CASCADE).
/// Does not clear audit_log or settings.
pub async fn delete_all_chats(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM chats").execute(pool).await?;
    Ok(())
}

// ---- Messages ----
pub async fn insert_message(
    pool: &SqlitePool,
    id: &str,
    chat_id: &str,
    role: &str,
    content: &str,
    tool_invocation_id: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO messages (id, chat_id, role, content, tool_invocation_id, created_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(chat_id)
    .bind(role)
    .bind(content)
    .bind(tool_invocation_id)
    .bind(now_ms())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_chat_messages(
    pool: &SqlitePool,
    chat_id: &str,
) -> Result<Vec<(String, String, String, String, Option<String>, i64)>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (String, String, String, String, Option<String>, i64)>(
        "SELECT id, chat_id, role, content, tool_invocation_id, created_at FROM messages WHERE chat_id = ? ORDER BY created_at ASC",
    )
    .bind(chat_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// ---- Tool invocations ----
pub async fn insert_tool_invocation(
    pool: &SqlitePool,
    id: &str,
    chat_id: &str,
    tool_name: &str,
    tool_source: &str,
    input_params: &str,
    target: &str,
    status: &str,
    phase_name: Option<&str>,
    risk_category: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO tool_invocations (id, chat_id, tool_name, tool_source, input_params, target, status, phase_name, risk_category, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(chat_id)
    .bind(tool_name)
    .bind(tool_source)
    .bind(input_params)
    .bind(target)
    .bind(status)
    .bind(phase_name)
    .bind(risk_category)
    .bind(now_ms())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_tool_invocation_result(
    pool: &SqlitePool,
    id: &str,
    raw_output: Option<&str>,
    exit_code: Option<i32>,
    duration_ms: Option<i64>,
    status: &str,
    approval_id: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE tool_invocations SET raw_output = ?, exit_code = ?, duration_ms = ?, status = ?, approval_id = ? WHERE id = ?",
    )
    .bind(raw_output)
    .bind(exit_code)
    .bind(duration_ms)
    .bind(status)
    .bind(approval_id)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_tool_invocation_status(pool: &SqlitePool, id: &str, status: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE tool_invocations SET status = ? WHERE id = ?")
        .bind(status)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Mark any tool invocations still 'running' for this chat as failed (e.g. after app restart).
pub async fn fix_stale_running_for_chat(pool: &SqlitePool, chat_id: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE tool_invocations SET status = 'failed', raw_output = 'Interrupted by app restart.' WHERE chat_id = ? AND status = 'running'",
    )
    .bind(chat_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Look up the chat_id that owns a given tool invocation.
pub async fn get_chat_id_for_invocation(pool: &SqlitePool, invocation_id: &str) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>("SELECT chat_id FROM tool_invocations WHERE id = ?")
        .bind(invocation_id)
        .fetch_optional(pool)
        .await
}

/// Mark all pending/running invocations for a chat as failed. Returns the IDs that were stopped.
pub async fn stop_all_active_for_chat(pool: &SqlitePool, chat_id: &str, message: &str) -> Result<Vec<String>, sqlx::Error> {
    let ids = sqlx::query_scalar::<_, String>(
        "SELECT id FROM tool_invocations WHERE chat_id = ? AND status IN ('pending', 'running')",
    )
    .bind(chat_id)
    .fetch_all(pool)
    .await?;

    if !ids.is_empty() {
        sqlx::query(
            "UPDATE tool_invocations SET status = 'failed', raw_output = ? WHERE chat_id = ? AND status IN ('pending', 'running')",
        )
        .bind(message)
        .bind(chat_id)
        .execute(pool)
        .await?;
    }
    Ok(ids)
}

pub async fn get_tool_invocations_for_chat(
    pool: &SqlitePool,
    chat_id: &str,
) ->     Result<
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
        sqlx::Error,
    > {
    let rows = sqlx::query_as::<_, (
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
    )>(
        "SELECT id, chat_id, tool_name, tool_source, input_params, target, raw_output, exit_code, duration_ms, approval_id, status, phase_name, risk_category, created_at FROM tool_invocations WHERE chat_id = ? ORDER BY created_at ASC",
    )
    .bind(chat_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[allow(dead_code)]
pub async fn get_tool_invocation(
    pool: &SqlitePool,
    id: &str,
) -> Result<
    Option<(
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
    sqlx::Error,
> {
    let row = sqlx::query_as::<_, (
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
    )>(
        "SELECT id, chat_id, tool_name, tool_source, raw_output, exit_code, duration_ms, approval_id, status, phase_name, risk_category, created_at FROM tool_invocations WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

// ---- Approvals ----
pub async fn insert_approval(
    pool: &SqlitePool,
    id: &str,
    tool_invocation_id: &str,
    decision: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO approvals (id, tool_invocation_id, decision, created_at) VALUES (?, ?, ?, ?)",
    )
    .bind(id)
    .bind(tool_invocation_id)
    .bind(decision)
    .bind(now_ms())
    .execute(pool)
    .await?;
    Ok(())
}

// ---- Findings ----
pub async fn insert_finding(
    pool: &SqlitePool,
    id: &str,
    chat_id: &str,
    tool_invocation_id: Option<&str>,
    title: &str,
    severity: &str,
    description: &str,
    mitre_ref: Option<&str>,
    owasp_ref: Option<&str>,
    cwe_ref: Option<&str>,
    recommended_action: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO findings (id, chat_id, tool_invocation_id, title, severity, description, mitre_ref, owasp_ref, cwe_ref, recommended_action, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(chat_id)
    .bind(tool_invocation_id)
    .bind(title)
    .bind(severity)
    .bind(description)
    .bind(mitre_ref)
    .bind(owasp_ref)
    .bind(cwe_ref)
    .bind(recommended_action)
    .bind(now_ms())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_findings_for_chat(
    pool: &SqlitePool,
    chat_id: &str,
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
    sqlx::Error,
> {
    let rows = sqlx::query_as::<_, (
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
    )>(
        "SELECT id, chat_id, tool_invocation_id, title, severity, description, mitre_ref, owasp_ref, cwe_ref, recommended_action, created_at FROM findings WHERE chat_id = ? ORDER BY created_at ASC",
    )
    .bind(chat_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// ---- File attachments (deferred to later phase - artifacts in chat can be hefty) ----
// pub async fn insert_file_attachment(
//     pool: &SqlitePool,
//     id: &str,
//     chat_id: &str,
//     filename: &str,
//     file_type: &str,
//     file_path: &str,
//     size_bytes: i64,
//     sha256: &str,
// ) -> Result<(), sqlx::Error> {
//     sqlx::query(
//         "INSERT INTO file_attachments (id, chat_id, filename, file_type, file_path, size_bytes, sha256, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
//     )
//     .bind(id)
//     .bind(chat_id)
//     .bind(filename)
//     .bind(file_type)
//     .bind(file_path)
//     .bind(size_bytes)
//     .bind(sha256)
//     .bind(now_ms())
//     .execute(pool)
//     .await?;
//     Ok(())
// }
//
// #[allow(dead_code)]
// pub async fn get_file_attachments_for_chat(
//     pool: &SqlitePool,
//     chat_id: &str,
// ) -> Result<Vec<(String, String, String, String, String, i64, String, i64)>, sqlx::Error> {
//     let rows = sqlx::query_as::<_, (String, String, String, String, String, i64, String, i64)>(
//         "SELECT id, chat_id, filename, file_type, file_path, size_bytes, sha256, created_at FROM file_attachments WHERE chat_id = ? ORDER BY created_at ASC",
//     )
//     .bind(chat_id)
//     .fetch_all(pool)
//     .await?;
//     Ok(rows)
// }

// ---- Battle Map ----
pub async fn get_battle_map(pool: &SqlitePool, chat_id: &str) -> Result<Option<String>, sqlx::Error> {
    let row = sqlx::query_scalar::<_, Option<String>>("SELECT battle_map FROM chats WHERE id = ?")
        .bind(chat_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.flatten())
}

pub async fn update_battle_map(pool: &SqlitePool, chat_id: &str, json_str: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE chats SET battle_map = ? WHERE id = ?")
        .bind(json_str)
        .bind(chat_id)
        .execute(pool)
        .await?;
    Ok(())
}

// ---- Settings ----
pub async fn save_setting(pool: &SqlitePool, key: &str, value: &str) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)")
        .bind(key)
        .bind(value)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_setting(pool: &SqlitePool, key: &str) -> Result<Option<String>, sqlx::Error> {
    let row = sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await?;
    Ok(row)
}

pub async fn delete_setting(pool: &SqlitePool, key: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM settings WHERE key = ?")
        .bind(key)
        .execute(pool)
        .await?;
    Ok(())
}
