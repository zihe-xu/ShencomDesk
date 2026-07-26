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
        .inspect(|_| logging::record_operation("ipc.config.get", "success"))
        .map_err(|error| map_config_error(ConfigOperation::Load, error))
}

/// Validates, migrates, and persists configuration supplied by React.
#[tauri::command]
pub async fn save_config(
    database: State<'_, DatabaseService>,
    config: AppConfig,
) -> IpcResult<AppConfig> {
    ConfigService::save(database.inner(), &config)
        .await
        .inspect(|_| logging::record_operation("ipc.config.save", "success"))
        .map_err(|error| map_config_error(ConfigOperation::Save, error))
}

/// Removes the stored configuration and recreates the current defaults.
#[tauri::command]
pub async fn reset_config(database: State<'_, DatabaseService>) -> IpcResult<AppConfig> {
    ConfigService::reset(database.inner())
        .await
        .inspect(|_| logging::record_operation("ipc.config.reset", "success"))
        .map_err(|error| map_config_error(ConfigOperation::Reset, error))
}

#[derive(Clone, Copy)]
enum ConfigOperation {
    Load,
    Save,
    Reset,
}

impl ConfigOperation {
    fn log_name(self) -> &'static str {
        match self {
            Self::Load => "ipc.config.get",
            Self::Save => "ipc.config.save",
            Self::Reset => "ipc.config.reset",
        }
    }
}

fn map_config_error(operation: ConfigOperation, error: AppError) -> IpcError {
    let operation_name = operation.log_name();
    tracing::error!(operation = operation_name, error = %error, "IPC configuration command failed");
    logging::record_operation(operation_name, "failed");

    match operation {
        ConfigOperation::Load => IpcError::for_config_load(&error),
        ConfigOperation::Save => IpcError::for_config_save(&error),
        ConfigOperation::Reset => IpcError::for_config_reset(&error),
    }
}
