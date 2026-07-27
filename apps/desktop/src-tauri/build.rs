const COMMANDS: &[&str] = &[
    "health_check",
    "get_config",
    "save_config",
    "reset_config",
    "create_task",
    "get_task_status",
    "list_tasks",
    "cancel_task",
    "read_text_file",
    "index_files",
    "start_file_watch",
    "stop_file_watch",
    "clear_file_cache",
    "install_plugin",
    "list_plugins",
    "get_plugin",
    "enable_plugin",
    "disable_plugin",
    "execute_plugin_command",
    "uninstall_plugin",
    "check_for_updates",
    "install_update",
];

fn main() {
    // Release builds embed this value through option_env!. Explicitly tracking it
    // prevents a cached unsigned development build from being reused after the
    // repository public-key variable is configured or rotated.
    println!("cargo:rerun-if-env-changed=SHENDESK_UPDATER_PUBLIC_KEY");

    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to build ShenDesk Tauri manifest");
}
