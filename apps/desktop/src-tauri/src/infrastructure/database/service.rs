use std::path::Path;

use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    SqlitePool,
};

use crate::utils::AppError;

/// SQLite access boundary used by application services.
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
            .foreign_keys(true);

        Self::connect_with_options(options, 5).await
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

    pub async fn get_config_value(&self, key: &str) -> Result<Option<String>, AppError> {
        sqlx::query_scalar::<_, String>("SELECT value FROM app_config WHERE key = ?1")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| {
                AppError::new(format!("failed to read configuration '{key}': {error}"))
            })
    }

    pub async fn set_config_value(&self, key: &str, value: &str) -> Result<(), AppError> {
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

    pub async fn delete_config_value(&self, key: &str) -> Result<(), AppError> {
        sqlx::query("DELETE FROM app_config WHERE key = ?1")
            .bind(key)
            .execute(&self.pool)
            .await
            .map_err(|error| {
                AppError::new(format!("failed to delete configuration '{key}': {error}"))
            })?;

        Ok(())
    }

    #[cfg(test)]
    pub async fn connect_in_memory() -> Result<Self, AppError> {
        let options = SqliteConnectOptions::new()
            .filename(":memory:")
            .create_if_missing(true)
            .foreign_keys(true);

        Self::connect_with_options(options, 1).await
    }

    #[cfg(test)]
    pub async fn has_config_key_prefix(&self, prefix: &str) -> Result<bool, AppError> {
        let pattern = format!("{prefix}%");
        let count =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM app_config WHERE key LIKE ?1")
                .bind(pattern)
                .fetch_one(&self.pool)
                .await
                .map_err(|error| {
                    AppError::new(format!("failed to inspect configuration backups: {error}"))
                })?;

        Ok(count > 0)
    }
}
