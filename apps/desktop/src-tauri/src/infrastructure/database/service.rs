use std::{path::Path, time::Duration};

use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    SqlitePool,
};

use crate::{
    application::config_repository::{ConfigRepository, ConfigRepositoryFuture},
    utils::AppError,
};

const DATABASE_MAX_CONNECTIONS: u32 = 5;
const DATABASE_BUSY_TIMEOUT: Duration = Duration::from_secs(5);
const WAL_AUTOCHECKPOINT_PAGES: u32 = 1_000;

/// SQLx/SQLite adapter for application-owned persistence ports.
#[derive(Debug, Clone)]
pub struct DatabaseService {
    pool: SqlitePool,
}

impl DatabaseService {
    /// Opens the local database, creates it when missing, and runs SQLx migrations.
    pub async fn connect(database_path: &Path) -> Result<Self, AppError> {
        let options = SqliteConnectOptions::new()
            .filename(database_path)
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(DATABASE_BUSY_TIMEOUT)
            .pragma("wal_autocheckpoint", WAL_AUTOCHECKPOINT_PAGES.to_string());

        Self::connect_with_options(options, DATABASE_MAX_CONNECTIONS).await
    }

    async fn connect_with_options(
        options: SqliteConnectOptions,
        max_connections: u32,
    ) -> Result<Self, AppError> {
        let pool = SqlitePoolOptions::new()
            .max_connections(max_connections)
            .connect_with(options)
            .await
            .map_err(|error| AppError::new(format!("failed to open SQLite database: {error}")))?;

        sqlx::migrate!("./src/infrastructure/database/migrations")
            .run(&pool)
            .await
            .map_err(|error| {
                AppError::new(format!("failed to run database migrations: {error}"))
            })?;

        Ok(Self { pool })
    }

    pub async fn shutdown(&self) -> Result<(), AppError> {
        let checkpoint_result = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .fetch_optional(&self.pool)
            .await
            .map(|_| ())
            .map_err(|error| AppError::new(format!("failed to checkpoint SQLite WAL: {error}")));

        self.pool.close().await;
        checkpoint_result
    }

    async fn get_config_value(&self, key: &str) -> Result<Option<String>, AppError> {
        sqlx::query_scalar::<_, String>("SELECT value FROM app_config WHERE key = ?1")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| {
                AppError::new(format!("failed to read configuration '{key}': {error}"))
            })
    }

    async fn set_config_value(&self, key: &str, value: &str) -> Result<(), AppError> {
        sqlx::query(
            r#"
            INSERT INTO app_config (key, value, updated_at)
            VALUES (?1, ?2, CURRENT_TIMESTAMP)
            ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                updated_at = CURRENT_TIMESTAMP
            "#,
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await
        .map_err(|error| AppError::new(format!("failed to save configuration '{key}': {error}")))?;

        Ok(())
    }

    async fn delete_config_value(&self, key: &str) -> Result<(), AppError> {
        sqlx::query("DELETE FROM app_config WHERE key = ?1")
            .bind(key)
            .execute(&self.pool)
            .await
            .map_err(|error| {
                AppError::new(format!("failed to delete configuration '{key}': {error}"))
            })?;

        Ok(())
    }
}

impl ConfigRepository for DatabaseService {
    fn get<'a>(&'a self, key: &'a str) -> ConfigRepositoryFuture<'a, Option<String>> {
        Box::pin(async move { self.get_config_value(key).await })
    }

    fn set<'a>(&'a self, key: &'a str, value: &'a str) -> ConfigRepositoryFuture<'a, ()> {
        Box::pin(async move { self.set_config_value(key, value).await })
    }

    fn delete<'a>(&'a self, key: &'a str) -> ConfigRepositoryFuture<'a, ()> {
        Box::pin(async move { self.delete_config_value(key).await })
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, time::SystemTime};

    use super::*;

    #[test]
    fn configures_file_database_for_concurrent_desktop_workloads() {
        tauri::async_runtime::block_on(async {
            let database_path = unique_test_database_path();
            let database = DatabaseService::connect(&database_path)
                .await
                .expect("file database should initialize");

            let journal_mode = sqlx::query_scalar::<_, String>("PRAGMA journal_mode")
                .fetch_one(&database.pool)
                .await
                .expect("journal mode should be readable");
            let synchronous = sqlx::query_scalar::<_, i64>("PRAGMA synchronous")
                .fetch_one(&database.pool)
                .await
                .expect("synchronous mode should be readable");
            let busy_timeout = sqlx::query_scalar::<_, i64>("PRAGMA busy_timeout")
                .fetch_one(&database.pool)
                .await
                .expect("busy timeout should be readable");
            let foreign_keys = sqlx::query_scalar::<_, i64>("PRAGMA foreign_keys")
                .fetch_one(&database.pool)
                .await
                .expect("foreign key mode should be readable");
            let wal_autocheckpoint = sqlx::query_scalar::<_, i64>("PRAGMA wal_autocheckpoint")
                .fetch_one(&database.pool)
                .await
                .expect("WAL checkpoint threshold should be readable");

            assert_eq!(journal_mode.to_ascii_lowercase(), "wal");
            assert_eq!(synchronous, 1, "NORMAL synchronous mode should be active");
            assert_eq!(busy_timeout, DATABASE_BUSY_TIMEOUT.as_millis() as i64);
            assert_eq!(foreign_keys, 1);
            assert_eq!(wal_autocheckpoint, i64::from(WAL_AUTOCHECKPOINT_PAGES));

            database.shutdown().await.expect("database should shut down");
            assert!(database.pool.is_closed());
            remove_database_files(&database_path);
        });
    }

    #[test]
    fn accepts_concurrent_short_writes_without_busy_errors() {
        tauri::async_runtime::block_on(async {
            let database_path = unique_test_database_path();
            let database = DatabaseService::connect(&database_path)
                .await
                .expect("file database should initialize");

            let mut handles = Vec::new();
            for index in 0..20 {
                let database = database.clone();
                handles.push(tauri::async_runtime::spawn(async move {
                    let key = format!("concurrency.test.{index}");
                    database.set_config_value(&key, "ok").await
                }));
            }

            for handle in handles {
                handle
                    .await
                    .expect("write task should complete")
                    .expect("short write should not fail with SQLITE_BUSY");
            }

            database.shutdown().await.expect("database should shut down");
            assert!(database.pool.is_closed());
            remove_database_files(&database_path);
        });
    }

    fn unique_test_database_path() -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();

        std::env::temp_dir().join(format!("shendesk-sqlite-{nonce}.sqlite"))
    }

    fn remove_database_files(database_path: &Path) {
        let _ = fs::remove_file(database_path);
        let _ = fs::remove_file(database_path.with_extension("sqlite-wal"));
        let _ = fs::remove_file(database_path.with_extension("sqlite-shm"));
    }
}
