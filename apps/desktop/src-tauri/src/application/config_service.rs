use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    domain::config::AppConfig, infrastructure::database::service::DatabaseService, utils::AppError,
};

const APP_CONFIG_KEY: &str = "app.settings";
const CORRUPT_CONFIG_KEY_PREFIX: &str = "app.settings.corrupt.";

/// Coordinates configuration defaults, migration, serialization, and persistence.
pub struct ConfigService;

impl ConfigService {
    pub async fn load(database: &DatabaseService) -> Result<AppConfig, AppError> {
        let Some(raw_config) = database.get_config_value(APP_CONFIG_KEY).await? else {
            let defaults = AppConfig::default();
            Self::save(database, &defaults).await?;
            return Ok(defaults);
        };

        let stored: AppConfig = match serde_json::from_str(&raw_config) {
            Ok(config) => config,
            Err(parse_error) => {
                return Self::recover_corrupt(database, &raw_config, &parse_error).await;
            }
        };
        let migrated = stored.clone().migrate();

        if migrated != stored {
            Self::save(database, &migrated).await?;
        }

        Ok(migrated)
    }

    pub async fn save(
        database: &DatabaseService,
        config: &AppConfig,
    ) -> Result<AppConfig, AppError> {
        let normalized = config.clone().migrate();
        let serialized = serde_json::to_string_pretty(&normalized).map_err(|error| {
            AppError::new(format!(
                "failed to serialize application configuration: {error}"
            ))
        })?;

        database
            .set_config_value(APP_CONFIG_KEY, &serialized)
            .await?;

        Ok(normalized)
    }

    pub async fn reset(database: &DatabaseService) -> Result<AppConfig, AppError> {
        database.delete_config_value(APP_CONFIG_KEY).await?;
        Self::load(database).await
    }

    async fn recover_corrupt(
        database: &DatabaseService,
        raw_config: &str,
        parse_error: &serde_json::Error,
    ) -> Result<AppConfig, AppError> {
        let backup_key = corrupt_backup_key();

        match database.set_config_value(&backup_key, raw_config).await {
            Ok(()) => tracing::error!(
                error = %parse_error,
                backup_key,
                "application configuration was corrupt and has been backed up"
            ),
            Err(backup_error) => tracing::error!(
                error = %parse_error,
                backup_error = %backup_error,
                "application configuration was corrupt and backup failed"
            ),
        }

        let defaults = AppConfig::default();
        let recovered = Self::save(database, &defaults).await?;
        tracing::warn!("application configuration recovered with current defaults");

        Ok(recovered)
    }
}

fn corrupt_backup_key() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    format!("{CORRUPT_CONFIG_KEY_PREFIX}{timestamp}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovers_corrupt_configuration_and_preserves_backup() {
        tauri::async_runtime::block_on(async {
            let database = DatabaseService::connect_in_memory()
                .await
                .expect("in-memory database should initialize");
            database
                .set_config_value(APP_CONFIG_KEY, "{not-valid-json")
                .await
                .expect("corrupt fixture should be stored");

            let recovered = ConfigService::load(&database)
                .await
                .expect("corrupt configuration should recover");

            assert_eq!(recovered, AppConfig::default());

            let stored = database
                .get_config_value(APP_CONFIG_KEY)
                .await
                .expect("recovered configuration should be readable")
                .expect("recovered configuration should exist");
            let persisted: AppConfig = serde_json::from_str(&stored)
                .expect("recovered configuration should be valid JSON");
            assert_eq!(persisted, AppConfig::default());

            assert!(
                database
                    .has_config_key_prefix(CORRUPT_CONFIG_KEY_PREFIX)
                    .await
                    .expect("backup lookup should succeed"),
                "corrupt configuration should be preserved under a backup key"
            );
        });
    }
}
