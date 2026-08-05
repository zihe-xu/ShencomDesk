const COMMANDS: &[&str] = &[
    "login",
    "get_auth_state",
    "logout",
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
    "compress_images",
    "get_office_engine_status",
    "create_office_document",
    "inspect_office_document",
    "apply_office_operations",
    "render_office_preview",
    "close_office_document",
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

    for (source, embedded) in [
        (
            "SHENDESK_AUTH_ENVIRONMENT",
            "SHENDESK_EMBEDDED_AUTH_ENVIRONMENT",
        ),
        ("SHENDESK_AUTH_HOST", "SHENDESK_EMBEDDED_AUTH_HOST"),
        ("SHENDESK_AUTH_SCID", "SHENDESK_EMBEDDED_AUTH_SCID"),
    ] {
        println!("cargo:rerun-if-env-changed={source}");
        if let Ok(value) = std::env::var(source) {
            println!("cargo:rustc-env={embedded}={value}");
        }
    }

    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to build ShenDesk Tauri manifest");
}
