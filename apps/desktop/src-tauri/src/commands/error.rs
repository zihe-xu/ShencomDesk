use serde::Serialize;

use crate::utils::AppError;

/// Stable error payload returned to the React IPC client.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpcError {
    pub code: String,
    pub message: String,
}

impl IpcError {
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            code: "internal_error".to_owned(),
            message: message.into(),
        }
    }
}

impl From<AppError> for IpcError {
    fn from(error: AppError) -> Self {
        Self::internal(error.to_string())
    }
}

pub type IpcResult<T> = Result<T, IpcError>;
