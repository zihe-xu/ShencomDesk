use tauri::State;

use crate::{
    app::state::AppState,
    application::auth_service::AuthServiceError,
    domain::auth::{AuthState, LoginRequest},
};

use super::error::{IpcError, IpcResult};

#[tauri::command]
pub async fn login(state: State<'_, AppState>, request: LoginRequest) -> IpcResult<AuthState> {
    state.auth_service().login(request).await.map_err(|error| {
        tracing::error!(error = %error, "IPC login command failed");
        map_auth_error(&error)
    })
}

#[tauri::command]
pub async fn get_auth_state(state: State<'_, AppState>) -> IpcResult<AuthState> {
    state.auth_service().state().await.map_err(|error| {
        tracing::error!(error = %error, "IPC get_auth_state command failed");
        map_auth_error(&error)
    })
}

#[tauri::command]
pub async fn logout(state: State<'_, AppState>) -> IpcResult<AuthState> {
    let service = state.auth_service().clone();
    let result = tauri::async_runtime::spawn_blocking(move || service.logout())
        .await
        .map_err(|error| {
            tracing::error!(error = %error, "IPC logout worker failed");
            IpcError::auth_unavailable()
        })?;

    result.map_err(|error| {
        tracing::error!(error = %error, "IPC logout command failed");
        map_auth_error(&error)
    })
}

fn map_auth_error(error: &AuthServiceError) -> IpcError {
    IpcError::for_auth_operation(error)
}
