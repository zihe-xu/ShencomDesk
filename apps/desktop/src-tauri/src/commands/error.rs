use serde::Serialize;

use crate::utils::{AppError, AppErrorKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IpcErrorCode {
    DatabaseUnavailable,
    ConfigLoadFailed,
    ConfigSaveFailed,
    ConfigResetFailed,
    TaskNotFound,
    TaskQueueUnavailable,
    ValidationFailed,
    UnknownError,
}

/// Stable, redacted error payload returned to the React IPC client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpcError {
    pub code: IpcErrorCode,
    pub message: String,
}

impl IpcError {
    pub fn for_config_load(error: &AppError) -> Self {
        Self::from_app_error(error, IpcErrorCode::ConfigLoadFailed)
    }

    pub fn for_config_save(error: &AppError) -> Self {
        Self::from_app_error(error, IpcErrorCode::ConfigSaveFailed)
    }

    pub fn for_config_reset(error: &AppError) -> Self {
        Self::from_app_error(error, IpcErrorCode::ConfigResetFailed)
    }

    pub fn task_not_found() -> Self {
        Self::new(IpcErrorCode::TaskNotFound, "未找到指定任务。")
    }

    pub fn task_queue_unavailable() -> Self {
        Self::new(
            IpcErrorCode::TaskQueueUnavailable,
            "后台任务服务暂时不可用，请重试。",
        )
    }

    pub fn validation() -> Self {
        Self::new(
            IpcErrorCode::ValidationFailed,
            "提交的数据无效，请检查后重试。",
        )
    }

    pub fn unknown() -> Self {
        Self::new(IpcErrorCode::UnknownError, "操作失败，请重试。")
    }

    fn from_app_error(error: &AppError, fallback: IpcErrorCode) -> Self {
        match error.kind() {
            AppErrorKind::Database => Self::new(
                IpcErrorCode::DatabaseUnavailable,
                "本地数据服务暂时不可用，请重试。",
            ),
            AppErrorKind::Validation => Self::validation(),
            AppErrorKind::Configuration | AppErrorKind::Internal => match fallback {
                IpcErrorCode::ConfigLoadFailed => Self::new(fallback, "无法读取应用配置，请重试。"),
                IpcErrorCode::ConfigSaveFailed => Self::new(fallback, "无法保存应用配置，请重试。"),
                IpcErrorCode::ConfigResetFailed => {
                    Self::new(fallback, "无法恢复默认配置，请重试。")
                }
                _ => Self::unknown(),
            },
        }
    }

    fn new(code: IpcErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

pub type IpcResult<T> = Result<T, IpcError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_database_details_from_ipc_payload() {
        let internal = AppError::database(
            "failed to open SQLite database /Users/example/private/app.sqlite: permission denied",
        );

        let payload = IpcError::for_config_load(&internal);
        let serialized = serde_json::to_string(&payload).expect("IPC error should serialize");

        assert_eq!(payload.code, IpcErrorCode::DatabaseUnavailable);
        assert!(!serialized.contains("/Users/example/private"));
        assert!(!serialized.contains("SQLite"));
        assert!(serialized.contains("database_unavailable"));
    }

    #[test]
    fn keeps_operation_specific_configuration_codes() {
        let internal = AppError::configuration("JSON parser failed at byte 17");

        assert_eq!(
            IpcError::for_config_load(&internal).code,
            IpcErrorCode::ConfigLoadFailed
        );
        assert_eq!(
            IpcError::for_config_save(&internal).code,
            IpcErrorCode::ConfigSaveFailed
        );
        assert_eq!(
            IpcError::for_config_reset(&internal).code,
            IpcErrorCode::ConfigResetFailed
        );
    }

    #[test]
    fn exposes_stable_task_errors_without_internal_details() {
        let not_found = IpcError::task_not_found();
        let unavailable = IpcError::task_queue_unavailable();

        assert_eq!(not_found.code, IpcErrorCode::TaskNotFound);
        assert_eq!(unavailable.code, IpcErrorCode::TaskQueueUnavailable);
        assert_eq!(not_found.message, "未找到指定任务。");
        assert_eq!(unavailable.message, "后台任务服务暂时不可用，请重试。");
    }
}
