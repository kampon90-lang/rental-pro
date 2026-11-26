use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    SqlitePool,
};
use std::{path::Path, str::FromStr, time::Duration};

/// Embedded SQLx migrations from workspace `./migrations`.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../migrations");

/// Initialize the SQLite connection pool with WAL journal mode, foreign keys enforced,
/// busy timeout, and run all pending migrations from `./migrations`.
pub async fn init_db(database_url: &str) -> Result<SqlitePool, sqlx::Error> {
    if let Some(file_path) = database_url.strip_prefix("sqlite://") {
        let clean_path = file_path.split('?').next().unwrap_or(file_path);
        if clean_path != ":memory:" && !clean_path.is_empty() {
            if let Some(parent) = Path::new(clean_path).parent() {
                if !parent.as_os_str().is_empty() {
                    let _ = std::fs::create_dir_all(parent);
                }
            }
        }
    } else if let Some(file_path) = database_url.strip_prefix("sqlite:") {
        let clean_path = file_path.split('?').next().unwrap_or(file_path);
        if clean_path != ":memory:" && !clean_path.is_empty() {
            if let Some(parent) = Path::new(clean_path).parent() {
                if !parent.as_os_str().is_empty() {
                    let _ = std::fs::create_dir_all(parent);
                }
            }
        }
    }

    let connect_options = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(10));

    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(connect_options)
        .await?;

    MIGRATOR.run(&pool).await?;

    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_initial_migration_creates_all_core_tables() {
        let pool = init_db("sqlite::memory:")
            .await
            .expect("in-memory SQLite migration should succeed");

        let expected_tables = [
            "admin_users",
            "admin_sessions",
            "system_settings",
            "property_groups",
            "units",
            "tenants",
            "contracts",
            "contract_attachments",
            "meter_readings",
            "invoices",
            "invoice_items",
            "payments",
            "receipts",
            "move_out_settlements",
            "expenses",
        ];

        for table in expected_tables {
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?",
            )
            .bind(table)
            .fetch_one(&pool)
            .await
            .expect("query sqlite_master");

            assert_eq!(count, 1, "expected table `{table}` to exist after 0001_init.sql");
        }

        // Verify singleton system_settings row exists with id = 1
        let settings_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM system_settings WHERE id = 1")
                .fetch_one(&pool)
                .await
                .expect("query system_settings");
        assert_eq!(settings_count, 1);
    }
}
