const COMMANDS: &[&str] = &["health_check", "get_config", "save_config", "reset_config"];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to build ShenDesk Tauri manifest");
}
