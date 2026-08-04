use std::{fs, sync::Arc};

use tauri::{App, Manager};

use crate::{
    application::config_service::ConfigService,
    infrastructure::{
        auth::{AuthEnvironment, KeyringAuthSessionStore, ShencomAuthBackend},
        database::service::DatabaseService,
        filesystem::LocalFileRepository,
        image::LocalImageProcessor,
        logging,
        office::OfficeCliRuntime,
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
    let office_runtime = Arc::new(OfficeCliRuntime::bundled().inspect_err(|error| {
        tracing::error!(error_kind = ?error.kind(), "OfficeCLI runtime initialization failed");
    })?);
    let office_status = tauri::async_runtime::block_on(
        crate::application::office_service::OfficeRuntime::probe(office_runtime.as_ref()),
    );
    match office_status {
        Ok(status) => tracing::info!(version = status.version, "OfficeCLI runtime ready"),
        Err(error) => tracing::warn!(error_kind = ?error.kind(), "OfficeCLI runtime unavailable"),
    }
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
    let auth_environment = AuthEnvironment::from_process_environment()?;
    let auth_backend = Arc::new(ShencomAuthBackend::new(auth_environment));
    let auth_session_store = Arc::new(match KeyringAuthSessionStore::new(auth_environment) {
        Ok(store) => store,
        Err(error) => {
            tracing::warn!(
                error = %error,
                "authentication session store is unavailable; starting without persistence"
            );
            KeyringAuthSessionStore::disabled()
        }
    });
    let state = AppState::new(
        Arc::new(LocalFileRepository::default()),
        Arc::new(LocalImageProcessor),
        office_runtime,
        plugin_repository,
        plugin_runtime,
        update_backend,
        auth_backend,
        auth_session_store,
    )
    .map_err(|error| {
        tracing::error!(error = %error, "authentication service initialization failed");
        error
    })?;
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
    logging::record_operation("image_service.initialize", "success");
    logging::record_operation("office_service.initialize", "success");

    app.manage(database);
    app.manage(state);
    lifecycle::on_ready(app.handle());

    Ok(())
}
