use tauri::State;

use crate::{
    app::state::AppState,
    application::auth_service::AuthServiceError,
    domain::auth::{LoginRequest, LoginResponse},
};

use super::error::{IpcError, IpcResult};

#[tauri::command]
pub async fn login(
    state: State<'_, AppState>,
    request: LoginRequest,
) -> IpcResult<LoginResponse> {
    state
        .auth_service()
        .login(request)
        .await
        .map_err(|error| {
            tracing::error!(error = %error, "IPC login command failed");
            map_auth_error(&error)
        })
}

fn map_auth_error(error: &AuthServiceError) -> IpcError {
    IpcError::for_auth_operation(error)
}
