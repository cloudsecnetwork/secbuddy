mod migrations;
mod queries;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};
use std::path::Path;
use std::str::FromStr;

pub use queries::*;

/// Initialize SQLite at the given path (e.g. app_data_dir/db.sqlite).
/// Creates parent dirs and runs migrations if needed.
pub async fn init_db(db_path: &Path) -> Result<SqlitePool, sqlx::Error> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let opts = SqliteConnectOptions::from_str(&format!("sqlite:{}", db_path.display()))?
        .create_if_missing(true);
    let pool = SqlitePool::connect_with(opts).await?;
    migrations::run(&pool).await?;
    Ok(pool)
}
