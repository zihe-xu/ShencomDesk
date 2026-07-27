use serde::Deserialize;
use tauri::State;

use crate::{
    app::state::AppState,
    application::plugin_service::PluginServiceError,
    domain::plugin::{PluginExecution, PluginId, PluginSnapshot},
    infrastructure::logging,
};

use super::error::{IpcError, IpcResult};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallPluginRequest {
    manifest_path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutePluginCommandRequest {
    plugin_id: String,
    command: String,
}

/// Installs and validates a local `plugin.json` + WASM package.
#[tauri::command]
pub async fn install_plugin(
    state: State<'_, AppState>,
    request: InstallPluginRequest,
) -> IpcResult<PluginSnapshot> {
    let service = state.plugin_service().clone();
    run_blocking("ipc.plugin.install", move || {
        service.install(request.manifest_path)
    })
    .await
}

/// Lists all installed plugins in deterministic plugin-id order.
#[tauri::command]
pub fn list_plugins(state: State<'_, AppState>) -> IpcResult<Vec<PluginSnapshot>> {
    state
        .plugin_service()
        .list()
        .inspect(|_| logging::record_operation("ipc.plugin.list", "success"))
        .map_err(|error| map_plugin_error("ipc.plugin.list", error))
}

/// Returns one installed plugin by its stable manifest identifier.
#[tauri::command]
pub fn get_plugin(state: State<'_, AppState>, plugin_id: String) -> IpcResult<PluginSnapshot> {
    state
        .plugin_service()
        .get(plugin_id)
        .inspect(|_| logging::record_operation("ipc.plugin.get", "success"))
        .map_err(|error| map_plugin_error("ipc.plugin.get", error))
}

/// Runs the optional enable hook, then persists the enabled state.
#[tauri::command]
pub async fn enable_plugin(
    state: State<'_, AppState>,
    plugin_id: String,
) -> IpcResult<PluginSnapshot> {
    let service = state.plugin_service().clone();
    run_blocking("ipc.plugin.enable", move || service.enable(plugin_id)).await
}

/// Runs the optional disable hook before persisting the disabled state.
#[tauri::command]
pub async fn disable_plugin(
    state: State<'_, AppState>,
    plugin_id: String,
) -> IpcResult<PluginSnapshot> {
    let service = state.plugin_service().clone();
    run_blocking("ipc.plugin.disable", move || service.disable(plugin_id)).await
}

/// Executes one manifest-declared export inside the resource-limited WASM sandbox.
#[tauri::command]
pub async fn execute_plugin_command(
    state: State<'_, AppState>,
    request: ExecutePluginCommandRequest,
) -> IpcResult<PluginExecution> {
    let service = state.plugin_service().clone();
    run_blocking("ipc.plugin.execute", move || {
        service.execute(request.plugin_id, request.command)
    })
    .await
}

/// Disables an enabled plugin, removes its managed package, and returns its id.
#[tauri::command]
pub async fn uninstall_plugin(
    state: State<'_, AppState>,
    plugin_id: String,
) -> IpcResult<PluginId> {
    let service = state.plugin_service().clone();
    run_blocking("ipc.plugin.uninstall", move || service.uninstall(plugin_id)).await
}

async fn run_blocking<T: Send + 'static>(
    operation: &'static str,
    work: impl FnOnce() -> Result<T, PluginServiceError> + Send + 'static,
) -> IpcResult<T> {
    let result = tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| map_worker_error(operation, error))?;

    result
        .inspect(|_| logging::record_operation(operation, "success"))
        .map_err(|error| map_plugin_error(operation, error))
}

fn map_plugin_error(operation: &'static str, error: PluginServiceError) -> IpcError {
    tracing::error!(operation, error = %error, "IPC plugin command failed");
    logging::record_operation(operation, "failed");
    IpcError::for_plugin_operation(&error)
}

fn map_worker_error(operation: &'static str, error: impl std::fmt::Display) -> IpcError {
    tracing::error!(operation, error = %error, "IPC plugin worker failed");
    logging::record_operation(operation, "failed");
    IpcError::plugin_operation_failed()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::plugin_service::PluginServiceErrorKind;

    #[test]
    fn plugin_requests_use_camel_case_fields() {
        let install: InstallPluginRequest = serde_json::from_value(serde_json::json!({
            "manifestPath": "/tmp/plugin.json"
        }))
        .expect("install request should deserialize");
        let execute: ExecutePluginCommandRequest = serde_json::from_value(serde_json::json!({
            "pluginId": "com.shencom.hello",
            "command": "hello"
        }))
        .expect("execute request should deserialize");

        assert_eq!(install.manifest_path, "/tmp/plugin.json");
        assert_eq!(execute.plugin_id, "com.shencom.hello");
        assert_eq!(execute.command, "hello");
    }

    #[test]
    fn plugin_errors_remain_operation_specific() {
        let not_found = PluginServiceError::new(
            PluginServiceErrorKind::NotFound,
            "/private/path must not reach IPC",
        );
        let mapped = map_plugin_error("ipc.plugin.get", not_found);

        assert_eq!(
            mapped.code,
            super::super::error::IpcErrorCode::PluginNotFound
        );
        assert!(!mapped.message.contains("/private/path"));
    }
}
