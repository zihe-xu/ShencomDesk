pub mod app;
pub mod application;
pub mod commands;
pub mod domain;
pub mod infrastructure;
pub mod utils;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    std::env::set_var("OFFICECLI_SKIP_UPDATE", "1");

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(crate::app::bootstrap::initialize)
        .invoke_handler(tauri::generate_handler![
            crate::commands::auth::login,
            crate::commands::auth::get_auth_state,
            crate::commands::auth::logout,
            crate::commands::health::health_check,
            crate::commands::config::get_config,
            crate::commands::config::save_config,
            crate::commands::config::reset_config,
            crate::commands::task::create_task,
            crate::commands::task::get_task_status,
            crate::commands::task::list_tasks,
            crate::commands::task::cancel_task,
            crate::commands::file::read_text_file,
            crate::commands::file::index_files,
            crate::commands::file::start_file_watch,
            crate::commands::file::stop_file_watch,
            crate::commands::file::clear_file_cache,
            crate::commands::image::compress_images,
            crate::commands::office::get_office_engine_status,
            crate::commands::office::create_office_document,
            crate::commands::office::inspect_office_document,
            crate::commands::office::apply_office_operations,
            crate::commands::office::render_office_preview,
            crate::commands::office::close_office_document,
            crate::commands::plugin::install_plugin,
            crate::commands::plugin::list_plugins,
            crate::commands::plugin::get_plugin,
            crate::commands::plugin::enable_plugin,
            crate::commands::plugin::disable_plugin,
            crate::commands::plugin::execute_plugin_command,
            crate::commands::plugin::uninstall_plugin,
            crate::commands::update::check_for_updates,
            crate::commands::update::install_update,
        ])
        .build(tauri::generate_context!())
        .expect("error while building ShenDesk");

    app.run(|app_handle, event| {
        crate::app::lifecycle::handle_run_event(app_handle, &event);
    });
}
