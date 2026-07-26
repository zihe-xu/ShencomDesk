use std::time::{SystemTime, UNIX_EPOCH};

use crate::{domain::config::AppConfig, utils::AppError};

use super::config_repository::ConfigRepository;

const APP_CONFIG_KEY: &str = "app.settings";
const CORRUPT_CONFIG_KEY_PREFIX: &str = "app.settings.corrupt.";

/// Coordinates configuration defaults, migration, serialization, and persistence.
pub struct ConfigService;

impl ConfigService {
    pub async fn load<R>(repository: &R) -> Result<AppConfig, AppError>
    where
        R: ConfigRepository + ?Sized,
    {
        let Some(raw_config) = repository.get(APP_CONFIG_KEY).await? else {
            let defaults = AppConfig::default();
            Self::save(repository, &defaults).await?;
            return Ok(defaults);
        };

        let stored: AppConfig = match serde_json::from_str(&raw_config) {
            Ok(config) => config,
            Err(parse_error) => {
                return Self::recover_corrupt(repository, &raw_config, &parse_error).await;
            }
        };
        let migrated = stored.clone().migrate();

        if migrated != stored {
            Self::save(repository, &migrated).await?;
        }

        Ok(migrated)
    }

    pub async fn save<R>(repository: &R, config: &AppConfig) -> Result<AppConfig, AppError>
    where
        R: ConfigRepository + ?Sized,
    {
        let normalized = config.clone().migrate();
        let serialized = serde_json::to_string_pretty(&normalized).map_err(|error| {
            AppError::new(format!(
                "failed to serialize application configuration: {error}"
            ))
        })?;

        repository.set(APP_CONFIG_KEY, &serialized).await?;

        Ok(normalized)
    }

    pub async fn reset<R>(repository: &R) -> Result<AppConfig, AppError>
    where
        R: ConfigRepository + ?Sized,
    {
        repository.delete(APP_CONFIG_KEY).await?;
        Self::load(repository).await
    }

    async fn recover_corrupt<R>(
        repository: &R,
        raw_config: &str,
        parse_error: &serde_json::Error,
    ) -> Result<AppConfig, AppError>
    where
        R: ConfigRepository + ?Sized,
    {
        let backup_key = corrupt_backup_key();

        match repository.set(&backup_key, raw_config).await {
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
        let recovered = Self::save(repository, &defaults).await?;
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
    use std::{collections::HashMap, future::ready, sync::Mutex};

    use super::*;
    use crate::application::config_repository::ConfigRepositoryFuture;

    #[derive(Default)]
    struct MemoryConfigRepository {
        values: Mutex<HashMap<String, String>>,
    }

    impl MemoryConfigRepository {
        fn value(&self, key: &str) -> Option<String> {
            self.values.lock().ok()?.get(key).cloned()
        }

        fn has_key_prefix(&self, prefix: &str) -> bool {
            self.values
                .lock()
                .map(|values| values.keys().any(|key| key.starts_with(prefix)))
                .unwrap_or(false)
        }
    }

    impl ConfigRepository for MemoryConfigRepository {
        fn get<'a>(&'a self, key: &'a str) -> ConfigRepositoryFuture<'a, Option<String>> {
            let result = self
                .values
                .lock()
                .map(|values| values.get(key).cloned())
                .map_err(|error| AppError::new(format!("memory repository lock failed: {error}")));
            Box::pin(ready(result))
        }

        fn set<'a>(&'a self, key: &'a str, value: &'a str) -> ConfigRepositoryFuture<'a, ()> {
            let result = self
                .values
                .lock()
                .map(|mut values| {
                    values.insert(key.to_owned(), value.to_owned());
                })
                .map_err(|error| AppError::new(format!("memory repository lock failed: {error}")));
            Box::pin(ready(result))
        }

        fn delete<'a>(&'a self, key: &'a str) -> ConfigRepositoryFuture<'a, ()> {
            let result = self
                .values
                .lock()
                .map(|mut values| {
                    values.remove(key);
                })
                .map_err(|error| AppError::new(format!("memory repository lock failed: {error}")));
            Box::pin(ready(result))
        }
    }

    #[test]
    fn creates_defaults_when_configuration_is_missing() {
        tauri::async_runtime::block_on(async {
            let repository = MemoryConfigRepository::default();

            let loaded = ConfigService::load(&repository)
                .await
                .expect("missing configuration should initialize");

            assert_eq!(loaded, AppConfig::default());
            assert!(repository.value(APP_CONFIG_KEY).is_some());
        });
    }

    #[test]
    fn recovers_corrupt_configuration_and_preserves_backup() {
        tauri::async_runtime::block_on(async {
            let repository = MemoryConfigRepository::default();
            repository
                .set(APP_CONFIG_KEY, "{not-valid-json")
                .await
                .expect("corrupt fixture should be stored");

            let recovered = ConfigService::load(&repository)
                .await
                .expect("corrupt configuration should recover");

            assert_eq!(recovered, AppConfig::default());

            let stored = repository
                .value(APP_CONFIG_KEY)
                .expect("recovered configuration should exist");
            let persisted: AppConfig = serde_json::from_str(&stored)
                .expect("recovered configuration should be valid JSON");
            assert_eq!(persisted, AppConfig::default());
            assert!(repository.has_key_prefix(CORRUPT_CONFIG_KEY_PREFIX));
        });
    }
}
