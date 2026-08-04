use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::{ipc::Channel, State};

use crate::{
    app::state::AppState, application::office_service::OfficeServiceError,
    domain::office::OfficeEngineStatus, infrastructure::logging,
};

use super::error::{IpcError, IpcResult};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CloseOfficeDocumentRequest {
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OfficeProgressStage {
    Closing,
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficeProgress {
    pub stage: OfficeProgressStage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloseOfficeDocumentResult {
    pub succeeded: bool,
}

#[tauri::command]
pub async fn get_office_engine_status(state: State<'_, AppState>) -> IpcResult<OfficeEngineStatus> {
    Ok(state.office_service().engine_status().await)
}

#[tauri::command]
pub async fn close_office_document(
    state: State<'_, AppState>,
    request: CloseOfficeDocumentRequest,
    on_progress: Channel<OfficeProgress>,
) -> IpcResult<CloseOfficeDocumentResult> {
    send_progress(&on_progress, OfficeProgressStage::Closing);
    let result = state
        .office_service()
        .close_document(Path::new(&request.path))
        .await
        .map_err(map_office_error)?;
    send_progress(&on_progress, OfficeProgressStage::Completed);
    logging::record_operation("ipc.office.close", "success");
    Ok(CloseOfficeDocumentResult {
        succeeded: result.succeeded,
    })
}

fn send_progress(channel: &Channel<OfficeProgress>, stage: OfficeProgressStage) {
    if channel.send(OfficeProgress { stage }).is_err() {
        tracing::debug!(?stage, "Office progress channel closed");
    }
}

fn map_office_error(error: OfficeServiceError) -> IpcError {
    tracing::error!(error_kind = ?error.kind(), "IPC Office command failed");
    logging::record_operation("ipc.office.close", "failed");
    IpcError::for_office_operation(&error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::office_service::OfficeServiceErrorKind, commands::error::IpcErrorCode,
    };

    #[test]
    fn request_uses_camel_case_and_rejects_process_controls() {
        let request: CloseOfficeDocumentRequest = serde_json::from_value(serde_json::json!({
            "path": "/tmp/report.docx"
        }))
        .expect("request should deserialize");
        assert_eq!(request.path, "/tmp/report.docx");

        for forbidden in ["binaryPath", "environment", "argv"] {
            let mut value = serde_json::json!({ "path": "/tmp/report.docx" });
            value[forbidden] = serde_json::json!(["unsafe"]);
            assert!(serde_json::from_value::<CloseOfficeDocumentRequest>(value).is_err());
        }
    }

    #[test]
    fn progress_and_result_use_stable_envelopes() {
        let progress = serde_json::to_value(OfficeProgress {
            stage: OfficeProgressStage::Closing,
        })
        .expect("progress should serialize");
        let result = serde_json::to_value(CloseOfficeDocumentResult { succeeded: true })
            .expect("result should serialize");

        assert_eq!(progress, serde_json::json!({ "stage": "closing" }));
        assert_eq!(result, serde_json::json!({ "succeeded": true }));
    }

    #[test]
    fn maps_all_office_errors_without_internal_details() {
        let cases = [
            (
                OfficeServiceErrorKind::EngineUnavailable,
                IpcErrorCode::OfficeEngineUnavailable,
            ),
            (
                OfficeServiceErrorKind::FormatUnsupported,
                IpcErrorCode::OfficeFormatUnsupported,
            ),
            (
                OfficeServiceErrorKind::DocumentNotFound,
                IpcErrorCode::OfficeDocumentNotFound,
            ),
            (
                OfficeServiceErrorKind::DocumentLocked,
                IpcErrorCode::OfficeDocumentLocked,
            ),
            (
                OfficeServiceErrorKind::OutputConflict,
                IpcErrorCode::OfficeOutputConflict,
            ),
            (
                OfficeServiceErrorKind::Timeout,
                IpcErrorCode::OfficeOperationTimeout,
            ),
            (
                OfficeServiceErrorKind::Cancelled,
                IpcErrorCode::OfficeOperationCancelled,
            ),
            (
                OfficeServiceErrorKind::OperationFailed,
                IpcErrorCode::OfficeOperationFailed,
            ),
        ];

        for (kind, expected) in cases {
            let mapped = map_office_error(OfficeServiceError::new(kind));
            let serialized = serde_json::to_string(&mapped).expect("error should serialize");
            assert_eq!(mapped.code, expected);
            assert!(!serialized.contains("/Users/example"));
            assert!(!serialized.contains("stderr"));
        }
    }
}
