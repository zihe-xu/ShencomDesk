use crate::{
    domain::config::AppConfig, infrastructure::database::service::DatabaseService, utils::AppError,
};

const APP_CONFIG_KEY: &str = "app.settings";

/// Coordinates configuration defaults, migration, serialization, and persistence.
pub struct ConfigService;

impl ConfigService {
    pub async fn load(database: &DatabaseService) -> Result<AppConfig, AppError> {
        let Some(raw_config) = database.get_config_value(APP_CONFIG_KEY).await? else {
            let defaults = AppConfig::default();
            Self::save(database, &defaults).await?;
            return Ok(defaults);
        };

        let stored: AppConfig = serde_json::from_str(&raw_config).map_err(|error| {
            AppError::new(format!(
                "failed to parse application configuration: {error}"
            ))
        })?;
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
}
