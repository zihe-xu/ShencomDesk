use tauri::AppHandle;

/// Called once the Tauri runtime has registered shared state.
pub fn on_ready(_app: &AppHandle) {
    // Lifecycle subscribers will be connected here as capabilities are added.
}

/// Reserved for coordinated shutdown and resource cleanup.
pub fn on_exit() {
    // Database pools, background tasks, and plugin runtimes will close here.
}
