use std::{fs, sync::Arc};

use tauri::{App, Manager};

use crate::{
    application::config_service::ConfigService,
    infrastructure::{
        auth::ShencomAuthBackend,
        database::service::DatabaseService,
        filesystem::LocalFileRepository,
        logging,
        plugins::{LocalPluginRepository, WasmtimePluginRuntime},
        updater::TauriUpdateBackend,
    },
};

use super::{lifecycle, state::AppState};

/// Initializes shared runtime resources before the main window is used.
pub fn initialize(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    app.handle()
        .plugin(tauri_plugin_updater::Builder::new().build())?;

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

    let plugin_root = app_data_dir.join("plugins");
    let plugin_repository =
        Arc::new(LocalPluginRepository::new(&plugin_root).map_err(|error| {
            tracing::error!(error = %error, "plugin repository initialization failed");
            error
        })?);
    let plugin_runtime = Arc::new(WasmtimePluginRuntime::new().map_err(|error| {
        tracing::error!(error = %error, "plugin runtime initialization failed");
        error
    })?);
    let update_backend = Arc::new(TauriUpdateBackend::new(app.handle().clone()));
    let auth_backend = Arc::new(ShencomAuthBackend::test_environment());
    let state = AppState::new(
        Arc::new(LocalFileRepository::default()),
        plugin_repository,
        plugin_runtime,
        update_backend,
        auth_backend,
    );
    let plugin_report = state.plugin_service().restore_enabled_plugins();
    tracing::info!(
        restored = plugin_report.restored,
        disabled_after_failure = plugin_report.disabled_after_failure,
        plugin_root = %plugin_root.display(),
        "plugin service initialized"
    );
    logging::record_operation("plugin_service.initialize", "success");
    logging::record_operation("update_service.initialize", "success");
    logging::record_operation("auth_service.initialize", "success");

    app.manage(database);
    app.manage(state);
    lifecycle::on_ready(app.handle());

    Ok(())
}
