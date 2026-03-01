use sqlx::SqlitePool;

const INIT_SQL: &str = include_str!("001_init.sql");

/// Run schema migration. Idempotent (CREATE TABLE IF NOT EXISTS).
/// Drops legacy blast_radius_score columns from existing DBs (ignores "no such column").
pub async fn run(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let sql: String = INIT_SQL
        .lines()
        .filter(|line| !line.trim_start().starts_with("--"))
        .collect::<Vec<_>>()
        .join("\n");
    let statements: Vec<String> = sql
        .split(';')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    for stmt in &statements {
        sqlx::query(stmt).execute(pool).await?;
    }
    // Drop legacy columns from existing DBs (SQLite 3.35+). Ignore if column missing.
    for drop_sql in [
        "ALTER TABLE tool_invocations DROP COLUMN blast_radius_score",
        "ALTER TABLE approvals DROP COLUMN blast_radius_score",
    ] {
        if let Err(e) = sqlx::query(drop_sql).execute(pool).await {
            let msg = e.to_string();
            if !msg.contains("no such column") {
                return Err(e);
            }
        }
    }
    Ok(())
}
