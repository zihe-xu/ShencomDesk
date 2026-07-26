use tauri::State;

use crate::{
    application::config_service::ConfigService,
    domain::config::AppConfig,
    infrastructure::{database::service::DatabaseService, logging},
    utils::AppError,
};

use super::error::{IpcError, IpcResult};

/// Loads the normalized application configuration from SQLite.
#[tauri::command]
pub async fn get_config(database: State<'_, DatabaseService>) -> IpcResult<AppConfig> {
    ConfigService::load(database.inner())
        .await
        .map(|config| {
            logging::record_operation("ipc.config.get", "success");
            config
        })
        .map_err(|error| map_config_error("ipc.config.get", error))
}

/// Validates, migrates, and persists configuration supplied by React.
#[tauri::command]
pub async fn save_config(
    database: State<'_, DatabaseService>,
    config: AppConfig,
) -> IpcResult<AppConfig> {
    ConfigService::save(database.inner(), &config)
        .await
        .map(|saved| {
            logging::record_operation("ipc.config.save", "success");
            saved
        })
        .map_err(|error| map_config_error("ipc.config.save", error))
}

/// Removes the stored configuration and recreates the current defaults.
#[tauri::command]
pub async fn reset_config(database: State<'_, DatabaseService>) -> IpcResult<AppConfig> {
    ConfigService::reset(database.inner())
        .await
        .map(|config| {
            logging::record_operation("ipc.config.reset", "success");
            config
        })
        .map_err(|error| map_config_error("ipc.config.reset", error))
}

fn map_config_error(operation: &str, error: AppError) -> IpcError {
    tracing::error!(operation, error = %error, "IPC configuration command failed");
    logging::record_operation(operation, "failed");
    error.into()
}
