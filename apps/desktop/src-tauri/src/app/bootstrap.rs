use std::{fs, sync::Arc};

use tauri::{App, Manager};

use crate::{
    application::config_service::ConfigService,
    infrastructure::{
        database::service::DatabaseService, filesystem::LocalFileRepository, logging,
    },
};

use super::{lifecycle, state::AppState};

/// Initializes shared runtime resources before the main window is used.
pub fn initialize(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let app_data_dir = app.path().app_data_dir()?;
    fs::create_dir_all(&app_data_dir)?;

    let log_dir = app_data_dir.join("logs");
    let logging_guards = logging::initialize(&log_dir)?;
    app.manage(logging_guards);

    tracing::info!(app_data_dir = %app_data_dir.display(), "initializing application resources");

    let database_path = app_data_dir.join("app.sqlite");
    let database = tauri::async_runtime::block_on(DatabaseService::connect(&database_path))
        .map_err(|error| {
            tracing::error!(error = %error, "database initialization failed");
            error
        })?;
    tracing::info!(database_path = %database_path.display(), "database initialized");

    // Loading once at startup creates defaults and persists any schema migration.
    tauri::async_runtime::block_on(ConfigService::load(&database)).map_err(|error| {
        tracing::error!(error = %error, "configuration initialization failed");
        error
    })?;
    logging::record_operation("config.load", "success");

    app.manage(database);
    app.manage(AppState::new(Arc::new(LocalFileRepository::default())));
    lifecycle::on_ready(app.handle());

    Ok(())
}
