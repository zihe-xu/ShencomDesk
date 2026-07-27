use serde::Deserialize;
use tauri::State;

use crate::{
    app::state::AppState,
    application::file_service::FileServiceError,
    domain::file::{FileIndex, FileReadResult, FileWatch, FileWatchId},
    infrastructure::logging,
};

use super::error::{IpcError, IpcResult};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadTextFileRequest {
    path: String,
    max_bytes: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexFilesRequest {
    root: String,
    max_entries: Option<usize>,
    max_depth: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartFileWatchRequest {
    path: String,
    #[serde(default)]
    recursive: bool,
}

/// Reads a bounded UTF-8 text file through the application FileService.
#[tauri::command]
pub async fn read_text_file(
    state: State<'_, AppState>,
    request: ReadTextFileRequest,
) -> IpcResult<FileReadResult> {
    let service = state.file_service().clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        service.read_text_file(request.path, request.max_bytes)
    })
    .await
    .map_err(|error| map_worker_error("ipc.file.read", error))?;

    result
        .inspect(|_| logging::record_operation("ipc.file.read", "success"))
        .map_err(|error| map_file_error("ipc.file.read", error))
}

/// Builds a deterministic, bounded recursive index for a local directory.
#[tauri::command]
pub async fn index_files(
    state: State<'_, AppState>,
    request: IndexFilesRequest,
) -> IpcResult<FileIndex> {
    let service = state.file_service().clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        service.index_directory(request.root, request.max_entries, request.max_depth)
    })
    .await
    .map_err(|error| map_worker_error("ipc.file.index", error))?;

    result
        .inspect(|_| logging::record_operation("ipc.file.index", "success"))
        .map_err(|error| map_file_error("ipc.file.index", error))
}

/// Starts a platform filesystem watch and publishes changes through EventBus.
#[tauri::command]
pub async fn start_file_watch(
    state: State<'_, AppState>,
    request: StartFileWatchRequest,
) -> IpcResult<FileWatch> {
    let service = state.file_service().clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        service.start_watch(request.path, request.recursive)
    })
    .await
    .map_err(|error| map_worker_error("ipc.file.watch.start", error))?;

    result
        .inspect(|_| logging::record_operation("ipc.file.watch.start", "success"))
        .map_err(|error| map_file_error("ipc.file.watch.start", error))
}

/// Stops one active filesystem watch registration.
#[tauri::command]
pub fn stop_file_watch(state: State<'_, AppState>, watch_id: String) -> IpcResult<FileWatchId> {
    state
        .file_service()
        .stop_watch(watch_id)
        .inspect(|_| logging::record_operation("ipc.file.watch.stop", "success"))
        .map_err(|error| map_file_error("ipc.file.watch.stop", error))
}

/// Invalidates every cached text-file value.
#[tauri::command]
pub fn clear_file_cache(state: State<'_, AppState>) {
    state.file_service().clear_cache();
    logging::record_operation("ipc.file.cache.clear", "success");
}

fn map_file_error(operation: &'static str, error: FileServiceError) -> IpcError {
    tracing::error!(operation, error = %error, "IPC file command failed");
    logging::record_operation(operation, "failed");
    IpcError::for_file_operation(&error)
}

fn map_worker_error(operation: &'static str, error: impl std::fmt::Display) -> IpcError {
    tracing::error!(operation, error = %error, "IPC file worker failed");
    logging::record_operation(operation, "failed");
    IpcError::file_operation_failed()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_defaults_to_non_recursive_watch() {
        let request: StartFileWatchRequest = serde_json::from_value(serde_json::json!({
            "path": "/tmp"
        }))
        .expect("request should deserialize");

        assert!(!request.recursive);
    }

    #[test]
    fn file_requests_use_camel_case_fields() {
        let read: ReadTextFileRequest = serde_json::from_value(serde_json::json!({
            "path": "/tmp/example.txt",
            "maxBytes": 1024
        }))
        .expect("read request should deserialize");
        let index: IndexFilesRequest = serde_json::from_value(serde_json::json!({
            "root": "/tmp",
            "maxEntries": 100,
            "maxDepth": 3
        }))
        .expect("index request should deserialize");

        assert_eq!(read.max_bytes, Some(1024));
        assert_eq!(index.max_entries, Some(100));
        assert_eq!(index.max_depth, Some(3));
    }
}
