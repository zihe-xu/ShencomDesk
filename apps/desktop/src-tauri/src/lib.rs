pub mod app;
pub mod application;
pub mod commands;
pub mod domain;
pub mod infrastructure;
pub mod utils;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .setup(crate::app::bootstrap::initialize)
        .invoke_handler(tauri::generate_handler![
            crate::commands::health::health_check,
            crate::commands::config::get_config,
            crate::commands::config::save_config,
            crate::commands::config::reset_config,
        ])
        .build(tauri::generate_context!())
        .expect("error while building ShenDesk");

    app.run(|app_handle, event| {
        crate::app::lifecycle::handle_run_event(app_handle, &event);
    });
}
