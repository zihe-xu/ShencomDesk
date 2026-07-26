use std::fs;

use tauri::{App, Manager};

use crate::{
    application::config_service::ConfigService,
    infrastructure::database::service::DatabaseService,
};

use super::{lifecycle, state::AppState};

/// Initializes shared runtime resources before the main window is used.
pub fn initialize(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let app_data_dir = app.path().app_data_dir()?;
    fs::create_dir_all(&app_data_dir)?;

    let database_path = app_data_dir.join("app.sqlite");
    let database = tauri::async_runtime::block_on(DatabaseService::connect(&database_path))?;

    // Loading once at startup creates defaults and persists any schema migration.
    tauri::async_runtime::block_on(ConfigService::load(&database))?;

    app.manage(database);
    app.manage(AppState::new());
    lifecycle::on_ready(app.handle());

    Ok(())
}
