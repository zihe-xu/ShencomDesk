use serde::Serialize;

use crate::{
    application::{
        file_service::{FileServiceError, FileServiceErrorKind},
        plugin_service::{PluginServiceError, PluginServiceErrorKind},
        update_service::{UpdateServiceError, UpdateServiceErrorKind},
    },
    utils::{AppError, AppErrorKind},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IpcErrorCode {
    DatabaseUnavailable,
    ConfigLoadFailed,
    ConfigSaveFailed,
    ConfigResetFailed,
    TaskNotFound,
    TaskQueueUnavailable,
    FileNotFound,
    FileAccessDenied,
    FileTooLarge,
    FileNotText,
    FileWatchUnavailable,
    FileWatchNotFound,
    FileOperationFailed,
    PluginNotFound,
    PluginAlreadyInstalled,
    PluginInvalidPackage,
    PluginConflict,
    PluginExecutionFailed,
    PluginOperationFailed,
    UpdateNotConfigured,
    UpdateBusy,
    UpdateNotAvailable,
    UpdateCheckFailed,
    UpdateInstallFailed,
    UpdateOperationFailed,
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

    pub fn for_file_operation(error: &FileServiceError) -> Self {
        match error.kind() {
            FileServiceErrorKind::InvalidInput
            | FileServiceErrorKind::NotAFile
            | FileServiceErrorKind::NotADirectory => Self::validation(),
            FileServiceErrorKind::NotFound => {
                Self::new(IpcErrorCode::FileNotFound, "未找到指定文件或目录。")
            }
            FileServiceErrorKind::PermissionDenied => Self::new(
                IpcErrorCode::FileAccessDenied,
                "没有权限访问指定文件或目录。",
            ),
            FileServiceErrorKind::TooLarge => {
                Self::new(IpcErrorCode::FileTooLarge, "文件超过允许读取的大小。")
            }
            FileServiceErrorKind::NonUtf8 => {
                Self::new(IpcErrorCode::FileNotText, "该文件不是可读取的 UTF-8 文本。")
            }
            FileServiceErrorKind::WatchUnavailable => Self::new(
                IpcErrorCode::FileWatchUnavailable,
                "文件监听服务暂时不可用，请重试。",
            ),
            FileServiceErrorKind::WatchNotFound => {
                Self::new(IpcErrorCode::FileWatchNotFound, "未找到指定文件监听。")
            }
            FileServiceErrorKind::Io => Self::file_operation_failed(),
        }
    }

    pub fn file_operation_failed() -> Self {
        Self::new(IpcErrorCode::FileOperationFailed, "文件操作失败，请重试。")
    }

    pub fn for_plugin_operation(error: &PluginServiceError) -> Self {
        match error.kind() {
            PluginServiceErrorKind::InvalidInput => Self::validation(),
            PluginServiceErrorKind::InvalidManifest
            | PluginServiceErrorKind::PackageTooLarge
            | PluginServiceErrorKind::RuntimeRejected => Self::new(
                IpcErrorCode::PluginInvalidPackage,
                "插件包无效或与当前版本不兼容。",
            ),
            PluginServiceErrorKind::NotFound => {
                Self::new(IpcErrorCode::PluginNotFound, "未找到指定插件。")
            }
            PluginServiceErrorKind::AlreadyInstalled => {
                Self::new(IpcErrorCode::PluginAlreadyInstalled, "该插件已经安装。")
            }
            PluginServiceErrorKind::Conflict => {
                Self::new(IpcErrorCode::PluginConflict, "插件当前状态不允许此操作。")
            }
            PluginServiceErrorKind::ExecutionFailed => Self::new(
                IpcErrorCode::PluginExecutionFailed,
                "插件执行失败，请重试。",
            ),
            PluginServiceErrorKind::Io => Self::plugin_operation_failed(),
        }
    }

    pub fn plugin_operation_failed() -> Self {
        Self::new(
            IpcErrorCode::PluginOperationFailed,
            "插件操作失败，请重试。",
        )
    }

    pub fn for_update_operation(error: &UpdateServiceError) -> Self {
        match error.kind() {
            UpdateServiceErrorKind::NotConfigured => Self::new(
                IpcErrorCode::UpdateNotConfigured,
                "当前构建未配置安全更新通道。",
            ),
            UpdateServiceErrorKind::Busy => {
                Self::new(IpcErrorCode::UpdateBusy, "另一个更新操作正在进行中。")
            }
            UpdateServiceErrorKind::NoPendingUpdate => Self::new(
                IpcErrorCode::UpdateNotAvailable,
                "没有可安装的更新，请先检查新版本。",
            ),
            UpdateServiceErrorKind::CheckFailed => Self::new(
                IpcErrorCode::UpdateCheckFailed,
                "检查更新失败，请稍后重试。",
            ),
            UpdateServiceErrorKind::InstallFailed => Self::new(
                IpcErrorCode::UpdateInstallFailed,
                "更新下载、验证或安装失败，请重试。",
            ),
            UpdateServiceErrorKind::Internal => Self::update_operation_failed(),
        }
    }

    pub fn update_operation_failed() -> Self {
        Self::new(
            IpcErrorCode::UpdateOperationFailed,
            "更新服务暂时不可用，请重试。",
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
                IpcErrorCode::ConfigLoadFailed => {
                    Self::new(fallback, "无法读取应用配置，请重试。")
                }
                IpcErrorCode::ConfigSaveFailed => {
                    Self::new(fallback, "无法保存应用配置，请重试。")
                }
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

    #[test]
    fn maps_file_errors_to_stable_redacted_payloads() {
        let internal = FileServiceError::new(
            FileServiceErrorKind::PermissionDenied,
            "permission denied: /Users/example/private.txt",
        );
        let payload = IpcError::for_file_operation(&internal);
        let serialized = serde_json::to_string(&payload).expect("error should serialize");

        assert_eq!(payload.code, IpcErrorCode::FileAccessDenied);
        assert!(!serialized.contains("/Users/example"));
        assert_eq!(payload.message, "没有权限访问指定文件或目录。");
    }

    #[test]
    fn maps_plugin_errors_without_exposing_paths_or_runtime_details() {
        let internal = PluginServiceError::new(
            PluginServiceErrorKind::RuntimeRejected,
            "failed to compile /Users/example/private/evil.wasm at offset 19",
        );
        let payload = IpcError::for_plugin_operation(&internal);
        let serialized = serde_json::to_string(&payload).expect("error should serialize");

        assert_eq!(payload.code, IpcErrorCode::PluginInvalidPackage);
        assert!(!serialized.contains("/Users/example"));
        assert!(!serialized.contains("offset"));
        assert_eq!(payload.message, "插件包无效或与当前版本不兼容。");
    }

    #[test]
    fn maps_update_errors_without_exposing_endpoints_or_signatures() {
        let internal = UpdateServiceError::check_failed(
            "GET https://private.example/latest.json rejected signature SECRET",
        );
        let payload = IpcError::for_update_operation(&internal);
        let serialized = serde_json::to_string(&payload).expect("error should serialize");

        assert_eq!(payload.code, IpcErrorCode::UpdateCheckFailed);
        assert!(!serialized.contains("private.example"));
        assert!(!serialized.contains("SECRET"));
        assert_eq!(payload.message, "检查更新失败，请稍后重试。");
    }
}
