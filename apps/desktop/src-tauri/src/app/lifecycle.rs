use tauri::{AppHandle, Manager, RunEvent};

use crate::{
    app::state::AppState,
    domain::event::AppEvent,
    infrastructure::{
        database::service::DatabaseService,
        logging::{self, LoggingGuards},
    },
};

/// Called once the Tauri runtime has registered shared state.
pub fn on_ready(app: &AppHandle) {
    app.state::<AppState>()
        .event_bus()
        .publish(AppEvent::ApplicationReady);
    tracing::info!("application runtime ready");
    logging::record_operation("application.ready", "success");
}

pub fn handle_run_event(app: &AppHandle, event: &RunEvent) {
    if is_exit_event(event) {
        on_exit(app);
    }
}

/// Coordinates background work, persistent storage shutdown, and log flushing before process exit.
pub fn on_exit(app: &AppHandle) {
    tracing::info!("application shutdown requested");
    logging::record_operation("application.exit", "requested");

    let state = app.state::<AppState>();
    state.event_bus().publish(AppEvent::ApplicationExiting);

    // Plugin hooks run before shared file and task services are stopped. ABI v1
    // has no host imports, but this ordering preserves room for future explicit capabilities.
    let stopped_plugins = state.plugin_service().shutdown();
    tracing::info!(stopped_plugins, "plugin service stopped");
    logging::record_operation("plugin_service.shutdown", "success");

    let stopped_watches = state.file_service().shutdown();
    tracing::info!(stopped_watches, "file service stopped");
    logging::record_operation("file_service.shutdown", "success");

    let cancelled_tasks = state.task_manager().shutdown();
    tracing::info!(cancelled_tasks, "task manager stopped");
    logging::record_operation("task_manager.shutdown", "success");

    let database = app.state::<DatabaseService>();
    match tauri::async_runtime::block_on(database.shutdown()) {
        Ok(()) => logging::record_operation("database.shutdown", "success"),
        Err(error) => {
            tracing::error!(error = %error, "database shutdown failed");
            logging::record_operation("database.shutdown", "failed");
        }
    }

    logging::record_operation("application.exit", "success");
    tracing::info!("application resources released");

    if let Err(error) = app.state::<LoggingGuards>().shutdown() {
        eprintln!("failed to flush ShenDesk logs during shutdown: {error}");
    }
}

fn is_exit_event(event: &RunEvent) -> bool {
    matches!(event, RunEvent::Exit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_exit_event_triggers_resource_shutdown() {
        assert!(is_exit_event(&RunEvent::Exit));
        assert!(!is_exit_event(&RunEvent::Ready));
    }
}
