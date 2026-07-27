use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use serde::Deserialize;
use tauri::{ipc::Channel, AppHandle, State};

use crate::{
    app::state::AppState,
    application::update_service::{UpdateProgressHandler, UpdateServiceError},
    domain::update::{UpdateInfo, UpdateInstallResult, UpdateProgress},
    infrastructure::logging,
};

use super::error::{IpcError, IpcResult};

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallUpdateRequest {
    #[serde(default)]
    restart: bool,
}

/// Checks the fixed HTTPS release channel for a newer signed version.
#[tauri::command]
pub async fn check_for_updates(state: State<'_, AppState>) -> IpcResult<Option<UpdateInfo>> {
    let service = state.update_service().clone();
    service
        .check()
        .await
        .inspect(|_| logging::record_operation("ipc.update.check", "success"))
        .map_err(|error| map_update_error("ipc.update.check", error))
}

/// Downloads, verifies, and installs the update retained by the latest check.
#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    state: State<'_, AppState>,
    request: InstallUpdateRequest,
    on_progress: Channel<UpdateProgress>,
) -> IpcResult<UpdateInstallResult> {
    let service = state.update_service().clone();
    let channel_open = Arc::new(AtomicBool::new(true));
    let send_enabled = channel_open.clone();
    let progress: UpdateProgressHandler = Arc::new(move |event| {
        if send_enabled.load(Ordering::Relaxed) && on_progress.send(event).is_err() {
            send_enabled.store(false, Ordering::Relaxed);
            tracing::debug!("update progress channel closed");
        }
    });

    service
        .install(progress)
        .await
        .inspect(|_| logging::record_operation("ipc.update.install", "success"))
        .map_err(|error| map_update_error("ipc.update.install", error))?;

    let result = UpdateInstallResult {
        installed: true,
        restart_requested: request.restart,
    };
    if request.restart {
        logging::record_operation("application.restart", "requested_by_update");
        app.request_restart();
    }

    Ok(result)
}

fn map_update_error(operation: &'static str, error: UpdateServiceError) -> IpcError {
    tracing::error!(operation, error = %error, "IPC update command failed");
    logging::record_operation(operation, "failed");
    IpcError::for_update_operation(&error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::update_service::UpdateServiceErrorKind, commands::error::IpcErrorCode,
    };

    #[test]
    fn install_request_defaults_to_no_restart() {
        let request: InstallUpdateRequest =
            serde_json::from_value(serde_json::json!({})).expect("request should deserialize");
        assert!(!request.restart);

        let request: InstallUpdateRequest =
            serde_json::from_value(serde_json::json!({ "restart": true }))
                .expect("request should deserialize");
        assert!(request.restart);
    }

    #[test]
    fn update_errors_are_redacted_and_operation_specific() {
        let internal = UpdateServiceError::new(
            UpdateServiceErrorKind::CheckFailed,
            "GET https://private.example/latest.json failed with secret header",
        );
        let mapped = map_update_error("ipc.update.check", internal);

        assert_eq!(mapped.code, IpcErrorCode::UpdateCheckFailed);
        assert!(!mapped.message.contains("private.example"));
        assert!(!mapped.message.contains("secret"));
    }
}
