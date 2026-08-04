use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::{ipc::Channel, State};

use crate::{
    app::state::AppState,
    application::office_service::{OfficeCancellationToken, OfficeServiceError},
    domain::office::{
        OfficeDocumentOperation, OfficeEngineStatus, OfficeInspection, OfficePreview,
    },
    infrastructure::logging,
};

use super::error::{IpcError, IpcResult};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CloseOfficeDocumentRequest {
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateOfficeDocumentRequest {
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InspectOfficeDocumentRequest {
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyOfficeOperationsRequest {
    pub path: String,
    pub output_path: String,
    pub operations: Vec<OfficeDocumentOperation>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RenderOfficePreviewRequest {
    pub path: String,
    #[serde(default = "default_preview_page")]
    pub page: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OfficeProgressStage {
    Creating,
    Inspecting,
    Applying,
    Rendering,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateOfficeDocumentResult {
    pub succeeded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyOfficeOperationsResult {
    pub succeeded: bool,
    pub operation_count: usize,
}

#[tauri::command]
pub async fn get_office_engine_status(state: State<'_, AppState>) -> IpcResult<OfficeEngineStatus> {
    Ok(state.office_service().engine_status().await)
}

#[tauri::command]
pub async fn create_office_document(
    state: State<'_, AppState>,
    request: CreateOfficeDocumentRequest,
    on_progress: Channel<OfficeProgress>,
) -> IpcResult<CreateOfficeDocumentResult> {
    send_progress(&on_progress, OfficeProgressStage::Creating);
    state
        .office_service()
        .create_document(Path::new(&request.path), OfficeCancellationToken::default())
        .await
        .map_err(|error| map_office_error("ipc.office.create", error))?;
    send_progress(&on_progress, OfficeProgressStage::Completed);
    logging::record_operation("ipc.office.create", "success");
    Ok(CreateOfficeDocumentResult { succeeded: true })
}

#[tauri::command]
pub async fn inspect_office_document(
    state: State<'_, AppState>,
    request: InspectOfficeDocumentRequest,
    on_progress: Channel<OfficeProgress>,
) -> IpcResult<OfficeInspection> {
    send_progress(&on_progress, OfficeProgressStage::Inspecting);
    let result = state
        .office_service()
        .inspect_document(Path::new(&request.path), OfficeCancellationToken::default())
        .await
        .map_err(|error| map_office_error("ipc.office.inspect", error))?;
    send_progress(&on_progress, OfficeProgressStage::Completed);
    logging::record_operation("ipc.office.inspect", "success");
    Ok(result)
}

#[tauri::command]
pub async fn apply_office_operations(
    state: State<'_, AppState>,
    request: ApplyOfficeOperationsRequest,
    on_progress: Channel<OfficeProgress>,
) -> IpcResult<ApplyOfficeOperationsResult> {
    send_progress(&on_progress, OfficeProgressStage::Applying);
    let operation_count = request.operations.len();
    state
        .office_service()
        .apply_operations(
            Path::new(&request.path),
            Path::new(&request.output_path),
            &request.operations,
            OfficeCancellationToken::default(),
        )
        .await
        .map_err(|error| map_office_error("ipc.office.apply", error))?;
    send_progress(&on_progress, OfficeProgressStage::Completed);
    logging::record_operation("ipc.office.apply", "success");
    Ok(ApplyOfficeOperationsResult {
        succeeded: true,
        operation_count,
    })
}

#[tauri::command]
pub async fn render_office_preview(
    state: State<'_, AppState>,
    request: RenderOfficePreviewRequest,
    on_progress: Channel<OfficeProgress>,
) -> IpcResult<OfficePreview> {
    send_progress(&on_progress, OfficeProgressStage::Rendering);
    let result = state
        .office_service()
        .render_preview(
            Path::new(&request.path),
            request.page,
            OfficeCancellationToken::default(),
        )
        .await
        .map_err(|error| map_office_error("ipc.office.preview", error))?;
    send_progress(&on_progress, OfficeProgressStage::Completed);
    logging::record_operation("ipc.office.preview", "success");
    Ok(result)
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
        .map_err(|error| map_office_error("ipc.office.close", error))?;
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

fn default_preview_page() -> u32 {
    1
}

fn map_office_error(operation: &'static str, error: OfficeServiceError) -> IpcError {
    tracing::error!(error_kind = ?error.kind(), "IPC Office command failed");
    logging::record_operation(operation, "failed");
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

        let apply: ApplyOfficeOperationsRequest = serde_json::from_value(serde_json::json!({
            "path": "/tmp/source.xlsx",
            "outputPath": "/tmp/output.xlsx",
            "operations": [{
                "type": "set_spreadsheet_cell",
                "cell": "A1",
                "value": "fixture"
            }]
        }))
        .expect("camelCase apply request should deserialize");
        assert_eq!(apply.output_path, "/tmp/output.xlsx");
        assert_eq!(apply.operations.len(), 1);

        let preview: RenderOfficePreviewRequest = serde_json::from_value(serde_json::json!({
            "path": "/tmp/source.xlsx"
        }))
        .expect("preview should use the first page by default");
        assert_eq!(preview.page, 1);
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
        assert_eq!(
            serde_json::to_value(OfficePreview {
                mime_type: "image/png".to_owned(),
                data_url: "data:image/png;base64,iVBORw0KGgo=".to_owned(),
            })
            .expect("preview should serialize"),
            serde_json::json!({
                "mimeType": "image/png",
                "dataUrl": "data:image/png;base64,iVBORw0KGgo="
            })
        );
        assert_eq!(
            serde_json::to_value(OfficeProgress {
                stage: OfficeProgressStage::Rendering,
            })
            .expect("progress should serialize"),
            serde_json::json!({ "stage": "rendering" })
        );
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
            let mapped = map_office_error("ipc.office.test", OfficeServiceError::new(kind));
            let serialized = serde_json::to_string(&mapped).expect("error should serialize");
            assert_eq!(mapped.code, expected);
            assert!(!serialized.contains("/Users/example"));
            assert!(!serialized.contains("stderr"));
        }
    }
}
