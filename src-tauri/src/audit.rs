//! Append-only audit log. Every significant action writes an entry.
//! Hash = SHA-256(timestamp | action_type | object_id | summary). No update/delete.

use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

/// Write an audit log entry. Call for every approval decision, tool run, denial, error, and significant assistant action.
/// Entry hash is SHA-256 of concatenated timestamp, action_type, object_id, summary.
pub async fn write_audit(
    pool: &SqlitePool,
    timestamp_ms: i64,
    action_type: &str,
    object_id: &str,
    summary: &str,
    metadata: Option<&str>,
) -> Result<(), sqlx::Error> {
    let mut hasher = Sha256::new();
    hasher.update(timestamp_ms.to_string().as_bytes());
    hasher.update(action_type.as_bytes());
    hasher.update(object_id.as_bytes());
    hasher.update(summary.as_bytes());
    let entry_hash = format!("{:x}", hasher.finalize());

    sqlx::query(
        "INSERT INTO audit_log (timestamp, action_type, object_id, summary, metadata, entry_hash) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(timestamp_ms)
    .bind(action_type)
    .bind(object_id)
    .bind(summary)
    .bind(metadata)
    .bind(entry_hash)
    .execute(pool)
    .await?;
    Ok(())
}
