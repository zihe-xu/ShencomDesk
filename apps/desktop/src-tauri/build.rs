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
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to build ShenDesk Tauri manifest");
}
