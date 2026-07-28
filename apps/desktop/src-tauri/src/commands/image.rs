use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use tauri::{ipc::Channel, State};

use crate::{
    app::state::AppState,
    application::image_service::{CompressionProgressHandler, ImageServiceError},
    domain::image::{CompressImagesRequest, CompressImagesResult, CompressionProgress},
    infrastructure::logging,
};

use super::error::{IpcError, IpcResult};

#[tauri::command]
pub async fn compress_images(
    state: State<'_, AppState>,
    request: CompressImagesRequest,
    on_progress: Channel<CompressionProgress>,
) -> IpcResult<CompressImagesResult> {
    let service = state.image_service().clone();
    let send_enabled = Arc::new(AtomicBool::new(true));
    let channel_open = send_enabled.clone();
    let progress: CompressionProgressHandler = Arc::new(move |event| {
        if channel_open.load(Ordering::Relaxed) && on_progress.send(event).is_err() {
            channel_open.store(false, Ordering::Relaxed);
            tracing::debug!("image compression progress channel closed");
        }
    });

    let result =
        tauri::async_runtime::spawn_blocking(move || service.compress_images(request, progress))
            .await
            .map_err(|error| {
                tracing::error!(error = %error, "IPC image worker failed");
                logging::record_operation("ipc.image.compress", "failed");
                IpcError::image_operation_failed()
            })?;

    result
        .inspect(|_| logging::record_operation("ipc.image.compress", "success"))
        .map_err(map_image_error)
}

fn map_image_error(error: ImageServiceError) -> IpcError {
    tracing::error!(error = %error, "IPC image command failed");
    logging::record_operation("ipc.image.compress", "failed");
    IpcError::for_image_operation(&error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{application::image_service::ImageServiceErrorKind, commands::error::IpcErrorCode};

    #[test]
    fn request_deserializes_camel_case_fields() {
        let request: CompressImagesRequest = serde_json::from_value(serde_json::json!({
            "items": ["/tmp/photo.jpg"],
            "outputDir": "/tmp/output",
            "quality": 75
        }))
        .expect("request should deserialize");

        assert_eq!(request.output_dir, "/tmp/output");
        assert_eq!(request.quality, 75);
    }

    #[test]
    fn command_error_mapping_is_redacted() {
        let error = ImageServiceError::new(
            ImageServiceErrorKind::Output,
            "permission denied: /Users/example/private/output.jpg",
        );
        let mapped = map_image_error(error);

        assert_eq!(mapped.code, IpcErrorCode::ImageOutputFailed);
        assert!(!mapped.message.contains("/Users/example"));
    }
}
