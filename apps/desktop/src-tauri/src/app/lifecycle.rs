use tauri::AppHandle;

use crate::infrastructure::logging;

/// Called once the Tauri runtime has registered shared state.
pub fn on_ready(_app: &AppHandle) {
    tracing::info!("application runtime ready");
    logging::record_operation("application.ready", "success");
}

/// Reserved for coordinated shutdown and resource cleanup.
pub fn on_exit() {
    tracing::info!("application shutdown requested");
    logging::record_operation("application.exit", "success");
}
