use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fmt, fs,
    future::Future,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde_json::Value;
use tokio::sync::{Mutex as AsyncMutex, Notify};

use crate::domain::office::{
    OfficeDocument, OfficeDocumentFormat, OfficeDocumentOperation, OfficeEngineStatus,
    OfficeInspection, OfficeOperationResult, OfficePreview,
};

const MAX_BATCH_OPERATIONS: usize = 100;
const MAX_OPERATION_TEXT_BYTES: usize = 16 * 1024;
const MAX_BATCH_TEXT_BYTES: usize = 16 * 1024;
const MAX_PREVIEW_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfficeRuntimeErrorKind {
    MissingSidecar,
    VersionMismatch,
    Spawn,
    Timeout,
    Cancelled,
    NonZeroExit,
    Crashed,
    OutputLimit,
    InvalidJson,
    Io,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OfficeRuntimeError {
    kind: OfficeRuntimeErrorKind,
}

impl OfficeRuntimeError {
    pub fn new(kind: OfficeRuntimeErrorKind) -> Self {
        Self { kind }
    }

    pub fn kind(&self) -> OfficeRuntimeErrorKind {
        self.kind
    }
}

impl fmt::Display for OfficeRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.kind {
            OfficeRuntimeErrorKind::MissingSidecar => "Office document engine is unavailable",
            OfficeRuntimeErrorKind::VersionMismatch => {
                "Office document engine version is incompatible"
            }
            OfficeRuntimeErrorKind::Spawn => "Office document engine could not be started",
            OfficeRuntimeErrorKind::Timeout => "Office document operation timed out",
            OfficeRuntimeErrorKind::Cancelled => "Office document operation was cancelled",
            OfficeRuntimeErrorKind::NonZeroExit | OfficeRuntimeErrorKind::Crashed => {
                "Office document operation failed"
            }
            OfficeRuntimeErrorKind::OutputLimit => {
                "Office document engine produced too much output"
            }
            OfficeRuntimeErrorKind::InvalidJson => "Office document engine returned invalid data",
            OfficeRuntimeErrorKind::Io => "Office document engine communication failed",
        })
    }
}

impl Error for OfficeRuntimeError {}

#[derive(Debug, Clone)]
pub struct OfficeCancellationToken {
    cancelled: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl Default for OfficeCancellationToken {
    fn default() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            notify: Arc::new(Notify::new()),
        }
    }
}

impl OfficeCancellationToken {
    pub fn cancel(&self) {
        if !self.cancelled.swap(true, Ordering::AcqRel) {
            self.notify.notify_waiters();
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub async fn cancelled(&self) {
        loop {
            let notified = self.notify.notified();
            if self.is_cancelled() {
                return;
            }
            notified.await;
        }
    }
}

#[async_trait]
pub trait OfficeRuntime: Send + Sync {
    async fn probe(&self) -> Result<OfficeEngineStatus, OfficeRuntimeError>;

    async fn open(
        &self,
        document: &OfficeDocument,
        cancellation: &OfficeCancellationToken,
    ) -> Result<OfficeOperationResult, OfficeRuntimeError>;

    async fn close(
        &self,
        document: &OfficeDocument,
    ) -> Result<OfficeOperationResult, OfficeRuntimeError>;

    async fn create(
        &self,
        document: &OfficeDocument,
        cancellation: &OfficeCancellationToken,
    ) -> Result<OfficeOperationResult, OfficeRuntimeError>;

    async fn inspect(
        &self,
        document: &OfficeDocument,
        cancellation: &OfficeCancellationToken,
    ) -> Result<Value, OfficeRuntimeError>;

    async fn apply_batch(
        &self,
        document: &OfficeDocument,
        operations: &[OfficeDocumentOperation],
        cancellation: &OfficeCancellationToken,
    ) -> Result<(), OfficeRuntimeError>;

    async fn render_preview(
        &self,
        document: &OfficeDocument,
        page: u32,
        output: &Path,
        cancellation: &OfficeCancellationToken,
    ) -> Result<(), OfficeRuntimeError>;

    /// Cancels only transient child processes started by this runtime.
    fn cancel_all(&self) -> usize;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfficeServiceErrorKind {
    EngineUnavailable,
    FormatUnsupported,
    DocumentNotFound,
    DocumentLocked,
    OutputConflict,
    Timeout,
    Cancelled,
    OperationFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OfficeServiceError {
    kind: OfficeServiceErrorKind,
}

impl OfficeServiceError {
    pub fn new(kind: OfficeServiceErrorKind) -> Self {
        Self { kind }
    }

    pub fn kind(&self) -> OfficeServiceErrorKind {
        self.kind
    }

    pub fn safe_message(&self) -> &'static str {
        match self.kind {
            OfficeServiceErrorKind::EngineUnavailable => "Office 文档引擎不可用。",
            OfficeServiceErrorKind::FormatUnsupported => "仅支持 DOCX、XLSX 和 PPTX 文档。",
            OfficeServiceErrorKind::DocumentNotFound => "找不到指定的 Office 文档。",
            OfficeServiceErrorKind::DocumentLocked => "Office 文档正在被其他程序使用。",
            OfficeServiceErrorKind::OutputConflict => "目标文件已存在。",
            OfficeServiceErrorKind::Timeout => "Office 文档操作超时。",
            OfficeServiceErrorKind::Cancelled => "Office 文档操作已取消。",
            OfficeServiceErrorKind::OperationFailed => "Office 文档操作失败。",
        }
    }
}

impl fmt::Display for OfficeServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.safe_message())
    }
}

impl Error for OfficeServiceError {}

impl From<OfficeRuntimeError> for OfficeServiceError {
    fn from(error: OfficeRuntimeError) -> Self {
        let kind = match error.kind() {
            OfficeRuntimeErrorKind::MissingSidecar
            | OfficeRuntimeErrorKind::VersionMismatch
            | OfficeRuntimeErrorKind::Spawn => OfficeServiceErrorKind::EngineUnavailable,
            OfficeRuntimeErrorKind::Timeout => OfficeServiceErrorKind::Timeout,
            OfficeRuntimeErrorKind::Cancelled => OfficeServiceErrorKind::Cancelled,
            OfficeRuntimeErrorKind::NonZeroExit
            | OfficeRuntimeErrorKind::Crashed
            | OfficeRuntimeErrorKind::OutputLimit
            | OfficeRuntimeErrorKind::InvalidJson
            | OfficeRuntimeErrorKind::Io => OfficeServiceErrorKind::OperationFailed,
        };
        Self::new(kind)
    }
}

pub struct OfficeService {
    runtime: Arc<dyn OfficeRuntime>,
    path_locks: Mutex<HashMap<PathBuf, Arc<AsyncMutex<()>>>>,
    owned_sessions: Mutex<HashSet<PathBuf>>,
    accepting: AtomicBool,
}

impl fmt::Debug for OfficeService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OfficeService")
            .field("accepting", &self.accepting.load(Ordering::Acquire))
            .finish_non_exhaustive()
    }
}

impl OfficeService {
    pub fn new(runtime: Arc<dyn OfficeRuntime>) -> Self {
        Self {
            runtime,
            path_locks: Mutex::new(HashMap::new()),
            owned_sessions: Mutex::new(HashSet::new()),
            accepting: AtomicBool::new(true),
        }
    }

    pub async fn engine_status(&self) -> OfficeEngineStatus {
        self.runtime
            .probe()
            .await
            .unwrap_or_else(|_| OfficeEngineStatus::unavailable())
    }

    pub async fn open_document(
        &self,
        path: &Path,
        cancellation: &OfficeCancellationToken,
    ) -> Result<OfficeOperationResult, OfficeServiceError> {
        self.ensure_accepting()?;
        let document = normalize_document(path)?;
        let path_lock = self.path_lock(&document.path);
        let _guard = path_lock.lock().await;
        self.open_locked(&document, cancellation).await
    }

    pub async fn close_document(
        &self,
        path: &Path,
    ) -> Result<OfficeOperationResult, OfficeServiceError> {
        let document = normalize_document(path)?;
        let path_lock = self.path_lock(&document.path);
        let _guard = path_lock.lock().await;
        self.close_locked(&document).await
    }

    pub async fn create_document(
        &self,
        path: &Path,
        cancellation: OfficeCancellationToken,
    ) -> Result<OfficeOperationResult, OfficeServiceError> {
        self.ensure_accepting()?;
        let target = normalize_new_document(path)?;
        let path_lock = self.path_lock(&target.path);
        let _guard = path_lock.lock().await;
        ensure_output_available(&target.path)?;

        let staging = tempfile::Builder::new()
            .prefix(".shendesk-office-create-")
            .tempdir_in(target.path.parent().ok_or_else(operation_failed)?)
            .map_err(|_| operation_failed())?;
        let staged_document = OfficeDocument {
            path: staging
                .path()
                .join(format!("document.{}", extension_for(target.format))),
            format: target.format,
        };
        let staged_path_lock = self.path_lock(&staged_document.path);
        let _staged_guard = staged_path_lock.lock().await;

        let create_result = self
            .runtime
            .create(&staged_document, &cancellation)
            .await
            .map_err(OfficeServiceError::from)?;
        if create_result.owns_session {
            self.owned_sessions
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .insert(staged_document.path.clone());
        }
        let close_result = self.close_locked(&staged_document).await;
        if cancellation.is_cancelled() {
            return Err(OfficeServiceError::new(OfficeServiceErrorKind::Cancelled));
        }
        close_result?;
        self.ensure_accepting()?;
        commit_staged_file(&staged_document.path, &target.path).await?;
        Ok(create_result)
    }

    pub async fn inspect_document(
        &self,
        path: &Path,
        cancellation: OfficeCancellationToken,
    ) -> Result<OfficeInspection, OfficeServiceError> {
        let format = normalize_document(path)?.format;
        let structure = self
            .with_document(
                path,
                cancellation,
                |runtime, document, cancellation| async move {
                    runtime
                        .inspect(&document, &cancellation)
                        .await
                        .map_err(OfficeServiceError::from)
                },
            )
            .await?;
        Ok(OfficeInspection { format, structure })
    }

    pub async fn apply_operations(
        &self,
        path: &Path,
        output_path: &Path,
        operations: &[OfficeDocumentOperation],
        cancellation: OfficeCancellationToken,
    ) -> Result<OfficeOperationResult, OfficeServiceError> {
        self.ensure_accepting()?;
        let source = normalize_document(path)?;
        let target = normalize_new_document(output_path)?;
        if source.format != target.format {
            return Err(OfficeServiceError::new(
                OfficeServiceErrorKind::FormatUnsupported,
            ));
        }
        validate_operations(source.format, operations)?;

        let path_lock = self.path_lock(&source.path);
        let _guard = path_lock.lock().await;
        ensure_output_available(&target.path)?;
        let staging = tempfile::Builder::new()
            .prefix(".shendesk-office-edit-")
            .tempdir_in(target.path.parent().ok_or_else(operation_failed)?)
            .map_err(|_| operation_failed())?;
        let staged_document = OfficeDocument {
            path: staging
                .path()
                .join(format!("document.{}", extension_for(source.format))),
            format: source.format,
        };
        let staged_path_lock = self.path_lock(&staged_document.path);
        let _staged_guard = staged_path_lock.lock().await;
        tokio::fs::copy(&source.path, &staged_document.path)
            .await
            .map_err(|_| operation_failed())?;

        let open_result = self.open_locked(&staged_document, &cancellation).await?;
        let operation_result = self
            .runtime
            .apply_batch(&staged_document, operations, &cancellation)
            .await
            .map_err(OfficeServiceError::from);
        let close_result = if open_result.owns_session {
            self.close_locked(&staged_document).await
        } else {
            Ok(OfficeOperationResult::succeeded(
                crate::domain::office::OfficeLifecycleOperation::Close,
            ))
        };
        operation_result?;
        close_result?;
        if cancellation.is_cancelled() {
            return Err(OfficeServiceError::new(OfficeServiceErrorKind::Cancelled));
        }
        self.ensure_accepting()?;
        commit_staged_file(&staged_document.path, &target.path).await?;
        Ok(OfficeOperationResult::succeeded(
            crate::domain::office::OfficeLifecycleOperation::Close,
        ))
    }

    pub async fn render_preview(
        &self,
        path: &Path,
        page: u32,
        cancellation: OfficeCancellationToken,
    ) -> Result<OfficePreview, OfficeServiceError> {
        if !(1..=10_000).contains(&page) {
            return Err(operation_failed());
        }
        let preview_dir = tempfile::Builder::new()
            .prefix("shendesk-office-preview-")
            .tempdir()
            .map_err(|_| operation_failed())?;
        let output = preview_dir.path().join("preview.png");
        self.with_document(path, cancellation, |runtime, document, cancellation| {
            let output = output.clone();
            async move {
                runtime
                    .render_preview(&document, page, &output, &cancellation)
                    .await
                    .map_err(OfficeServiceError::from)
            }
        })
        .await?;
        let metadata = tokio::fs::metadata(&output)
            .await
            .map_err(|_| operation_failed())?;
        if !metadata.is_file() || metadata.len() > MAX_PREVIEW_BYTES {
            return Err(operation_failed());
        }
        let bytes = tokio::fs::read(&output)
            .await
            .map_err(|_| operation_failed())?;
        if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Err(operation_failed());
        }
        let bytes = tokio::task::spawn_blocking(move || {
            image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
                .map_err(|_| operation_failed())?;
            Ok::<_, OfficeServiceError>(bytes)
        })
        .await
        .map_err(|_| operation_failed())??;
        Ok(OfficePreview {
            mime_type: "image/png".to_owned(),
            data_url: format!("data:image/png;base64,{}", BASE64.encode(bytes)),
        })
    }

    /// Runs an application operation inside an owned OfficeCLI document session.
    /// The close step is attempted after success, failure, or cancellation.
    pub async fn with_document<T, F, Fut>(
        &self,
        path: &Path,
        cancellation: OfficeCancellationToken,
        operation: F,
    ) -> Result<T, OfficeServiceError>
    where
        F: FnOnce(Arc<dyn OfficeRuntime>, OfficeDocument, OfficeCancellationToken) -> Fut,
        Fut: Future<Output = Result<T, OfficeServiceError>>,
    {
        self.ensure_accepting()?;
        let document = normalize_document(path)?;
        let path_lock = self.path_lock(&document.path);
        let _guard = path_lock.lock().await;

        let open_result = self.open_locked(&document, &cancellation).await?;
        let operation_result = if cancellation.is_cancelled() {
            Err(OfficeServiceError::new(OfficeServiceErrorKind::Cancelled))
        } else {
            operation(Arc::clone(&self.runtime), document.clone(), cancellation).await
        };
        let close_result = if open_result.owns_session {
            self.close_locked(&document).await
        } else {
            Ok(OfficeOperationResult::succeeded(
                crate::domain::office::OfficeLifecycleOperation::Close,
            ))
        };

        match (operation_result, close_result) {
            (Ok(value), Ok(_)) => Ok(value),
            (Ok(_), Err(close_error)) => Err(close_error),
            (Err(operation_error), Ok(_)) => Err(operation_error),
            (Err(operation_error), Err(close_error)) => {
                tracing::error!(
                    operation_error_kind = ?operation_error.kind(),
                    close_error_kind = ?close_error.kind(),
                    "Office operation and best-effort close failed"
                );
                Err(operation_error)
            }
        }
    }

    pub async fn shutdown(&self) -> usize {
        if !self.accepting.swap(false, Ordering::AcqRel) {
            return 0;
        }

        let cancelled_children = self.runtime.cancel_all();
        let paths = self
            .owned_sessions
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let mut closed_sessions = 0;

        for path in paths {
            let Some(format) = path
                .extension()
                .and_then(|extension| extension.to_str())
                .and_then(OfficeDocumentFormat::from_extension)
            else {
                continue;
            };
            let document = OfficeDocument { path, format };
            let path_lock = self.path_lock(&document.path);
            let _guard = path_lock.lock().await;
            if self.close_locked(&document).await.is_ok() {
                closed_sessions += 1;
            }
        }

        cancelled_children + closed_sessions
    }

    fn ensure_accepting(&self) -> Result<(), OfficeServiceError> {
        if self.accepting.load(Ordering::Acquire) {
            Ok(())
        } else {
            Err(OfficeServiceError::new(
                OfficeServiceErrorKind::OperationFailed,
            ))
        }
    }

    fn path_lock(&self, path: &Path) -> Arc<AsyncMutex<()>> {
        let mut locks = self
            .path_locks
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        Arc::clone(
            locks
                .entry(path.to_path_buf())
                .or_insert_with(|| Arc::new(AsyncMutex::new(()))),
        )
    }

    async fn open_locked(
        &self,
        document: &OfficeDocument,
        cancellation: &OfficeCancellationToken,
    ) -> Result<OfficeOperationResult, OfficeServiceError> {
        if cancellation.is_cancelled() {
            return Err(OfficeServiceError::new(OfficeServiceErrorKind::Cancelled));
        }
        let result = self.runtime.open(document, cancellation).await?;
        if !self.accepting.load(Ordering::Acquire) {
            if result.owns_session {
                let _ = self.runtime.close(document).await;
            }
            return Err(OfficeServiceError::new(
                OfficeServiceErrorKind::OperationFailed,
            ));
        }
        if result.owns_session {
            self.owned_sessions
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .insert(document.path.clone());
        }
        Ok(result)
    }

    async fn close_locked(
        &self,
        document: &OfficeDocument,
    ) -> Result<OfficeOperationResult, OfficeServiceError> {
        let is_owned = self
            .owned_sessions
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .contains(&document.path);
        if !is_owned {
            return Ok(OfficeOperationResult::succeeded(
                crate::domain::office::OfficeLifecycleOperation::Close,
            ));
        }
        let result = self.runtime.close(document).await?;
        self.owned_sessions
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&document.path);
        Ok(result)
    }
}

fn normalize_document(path: &Path) -> Result<OfficeDocument, OfficeServiceError> {
    let format = path
        .extension()
        .and_then(|extension| extension.to_str())
        .and_then(OfficeDocumentFormat::from_extension)
        .ok_or_else(|| OfficeServiceError::new(OfficeServiceErrorKind::FormatUnsupported))?;
    let path = path
        .canonicalize()
        .map_err(|_| OfficeServiceError::new(OfficeServiceErrorKind::DocumentNotFound))?;
    if !path.is_file() {
        return Err(OfficeServiceError::new(
            OfficeServiceErrorKind::DocumentNotFound,
        ));
    }
    Ok(OfficeDocument { path, format })
}

fn normalize_new_document(path: &Path) -> Result<OfficeDocument, OfficeServiceError> {
    let format = path
        .extension()
        .and_then(|extension| extension.to_str())
        .and_then(OfficeDocumentFormat::from_extension)
        .ok_or_else(|| OfficeServiceError::new(OfficeServiceErrorKind::FormatUnsupported))?;
    if !path.is_absolute() {
        return Err(OfficeServiceError::new(
            OfficeServiceErrorKind::DocumentNotFound,
        ));
    }
    let file_name = path.file_name().ok_or_else(operation_failed)?;
    let parent = path
        .parent()
        .and_then(|parent| parent.canonicalize().ok())
        .filter(|parent| parent.is_dir())
        .ok_or_else(|| OfficeServiceError::new(OfficeServiceErrorKind::DocumentNotFound))?;
    Ok(OfficeDocument {
        path: parent.join(file_name),
        format,
    })
}

fn ensure_output_available(path: &Path) -> Result<(), OfficeServiceError> {
    if path.exists() {
        Err(OfficeServiceError::new(
            OfficeServiceErrorKind::OutputConflict,
        ))
    } else {
        Ok(())
    }
}

fn validate_operations(
    format: OfficeDocumentFormat,
    operations: &[OfficeDocumentOperation],
) -> Result<(), OfficeServiceError> {
    if operations.is_empty() || operations.len() > MAX_BATCH_OPERATIONS {
        return Err(operation_failed());
    }
    let mut total_text_bytes = 0_usize;
    for operation in operations {
        if !operation.supports_format(format) {
            return Err(OfficeServiceError::new(
                OfficeServiceErrorKind::FormatUnsupported,
            ));
        }
        let (valid, text_bytes) = match operation {
            OfficeDocumentOperation::AddWordParagraph { text }
            | OfficeDocumentOperation::AddPresentationSlide { title: text }
            | OfficeDocumentOperation::AddPresentationText { text, .. } => (
                !text.is_empty() && text.len() <= MAX_OPERATION_TEXT_BYTES,
                text.len(),
            ),
            OfficeDocumentOperation::SetSpreadsheetCell { cell, value } => (
                valid_cell_reference(cell) && value.len() <= MAX_OPERATION_TEXT_BYTES,
                value.len(),
            ),
        };
        total_text_bytes = total_text_bytes.saturating_add(text_bytes);
        if !valid
            || total_text_bytes > MAX_BATCH_TEXT_BYTES
            || matches!(
                operation,
                OfficeDocumentOperation::AddPresentationText { slide: 0, .. }
            )
        {
            return Err(operation_failed());
        }
    }
    Ok(())
}

fn valid_cell_reference(cell: &str) -> bool {
    let split = cell
        .bytes()
        .position(|byte| byte.is_ascii_digit())
        .unwrap_or(cell.len());
    let (column, row) = cell.split_at(split);
    let column_number = column.bytes().fold(0_u32, |value, byte| {
        value
            .saturating_mul(26)
            .saturating_add(u32::from(byte.saturating_sub(b'A')) + 1)
    });
    !column.is_empty()
        && column.len() <= 3
        && column.bytes().all(|byte| byte.is_ascii_uppercase())
        && column_number <= 16_384
        && !row.is_empty()
        && row.len() <= 7
        && row.bytes().all(|byte| byte.is_ascii_digit())
        && row
            .parse::<u32>()
            .is_ok_and(|row| (1..=1_048_576).contains(&row))
}

fn extension_for(format: OfficeDocumentFormat) -> &'static str {
    match format {
        OfficeDocumentFormat::Word => "docx",
        OfficeDocumentFormat::Spreadsheet => "xlsx",
        OfficeDocumentFormat::Presentation => "pptx",
    }
}

async fn commit_staged_file(source: &Path, target: &Path) -> Result<(), OfficeServiceError> {
    let source = source.to_path_buf();
    let target = target.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let parent = target.parent().ok_or_else(operation_failed)?;
        let mut staged = tempfile::NamedTempFile::new_in(parent).map_err(|_| operation_failed())?;
        let mut input = fs::File::open(source).map_err(|_| operation_failed())?;
        std::io::copy(&mut input, staged.as_file_mut()).map_err(|_| operation_failed())?;
        staged
            .as_file()
            .sync_all()
            .map_err(|_| operation_failed())?;
        staged.persist_noclobber(target).map_err(|error| {
            if error.error.kind() == std::io::ErrorKind::AlreadyExists {
                OfficeServiceError::new(OfficeServiceErrorKind::OutputConflict)
            } else {
                operation_failed()
            }
        })?;
        Ok(())
    })
    .await
    .map_err(|_| operation_failed())?
}

fn operation_failed() -> OfficeServiceError {
    OfficeServiceError::new(OfficeServiceErrorKind::OperationFailed)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };

    use crate::domain::office::OfficeLifecycleOperation;

    use super::*;

    #[derive(Debug, Default)]
    struct RecordingRuntime {
        calls: Mutex<Vec<OfficeLifecycleOperation>>,
        operations: Mutex<Vec<OfficeDocumentOperation>>,
        preview_paths: Mutex<Vec<PathBuf>>,
        reuses_existing_session: bool,
        fail_batch: bool,
    }

    #[async_trait]
    impl OfficeRuntime for RecordingRuntime {
        async fn probe(&self) -> Result<OfficeEngineStatus, OfficeRuntimeError> {
            Ok(OfficeEngineStatus::ready("1.0.143"))
        }

        async fn open(
            &self,
            _document: &OfficeDocument,
            cancellation: &OfficeCancellationToken,
        ) -> Result<OfficeOperationResult, OfficeRuntimeError> {
            if cancellation.is_cancelled() {
                return Err(OfficeRuntimeError::new(OfficeRuntimeErrorKind::Cancelled));
            }
            self.calls
                .lock()
                .expect("calls should lock")
                .push(OfficeLifecycleOperation::Open);
            Ok(OfficeOperationResult::opened(!self.reuses_existing_session))
        }

        async fn close(
            &self,
            _document: &OfficeDocument,
        ) -> Result<OfficeOperationResult, OfficeRuntimeError> {
            self.calls
                .lock()
                .expect("calls should lock")
                .push(OfficeLifecycleOperation::Close);
            Ok(OfficeOperationResult::succeeded(
                OfficeLifecycleOperation::Close,
            ))
        }

        async fn create(
            &self,
            document: &OfficeDocument,
            cancellation: &OfficeCancellationToken,
        ) -> Result<OfficeOperationResult, OfficeRuntimeError> {
            if cancellation.is_cancelled() {
                return Err(OfficeRuntimeError::new(OfficeRuntimeErrorKind::Cancelled));
            }
            fs::write(&document.path, b"created")
                .map_err(|_| OfficeRuntimeError::new(OfficeRuntimeErrorKind::Io))?;
            self.calls
                .lock()
                .expect("calls should lock")
                .push(OfficeLifecycleOperation::Open);
            Ok(OfficeOperationResult::opened(true))
        }

        async fn inspect(
            &self,
            _document: &OfficeDocument,
            _cancellation: &OfficeCancellationToken,
        ) -> Result<Value, OfficeRuntimeError> {
            Ok(serde_json::json!({ "matches": 1 }))
        }

        async fn apply_batch(
            &self,
            document: &OfficeDocument,
            operations: &[OfficeDocumentOperation],
            _cancellation: &OfficeCancellationToken,
        ) -> Result<(), OfficeRuntimeError> {
            self.operations
                .lock()
                .expect("operations should lock")
                .extend_from_slice(operations);
            fs::write(&document.path, b"edited")
                .map_err(|_| OfficeRuntimeError::new(OfficeRuntimeErrorKind::Io))?;
            if self.fail_batch {
                Err(OfficeRuntimeError::new(OfficeRuntimeErrorKind::NonZeroExit))
            } else {
                Ok(())
            }
        }

        async fn render_preview(
            &self,
            _document: &OfficeDocument,
            _page: u32,
            output: &Path,
            _cancellation: &OfficeCancellationToken,
        ) -> Result<(), OfficeRuntimeError> {
            self.preview_paths
                .lock()
                .expect("preview paths should lock")
                .push(output.to_path_buf());
            image::RgbImage::new(1, 1)
                .save_with_format(output, image::ImageFormat::Png)
                .map_err(|_| OfficeRuntimeError::new(OfficeRuntimeErrorKind::Io))
        }

        fn cancel_all(&self) -> usize {
            0
        }
    }

    #[test]
    fn recording_runtime_supports_application_tests_without_officecli() {
        let fixture =
            tempfile::NamedTempFile::with_suffix(".docx").expect("fixture should be created");
        let runtime = Arc::new(RecordingRuntime::default());
        let service = OfficeService::new(runtime.clone());

        let value = tauri::async_runtime::block_on(service.with_document(
            fixture.path(),
            OfficeCancellationToken::default(),
            |_runtime, _document, _cancellation| async { Ok(42) },
        ))
        .expect("operation should succeed");

        assert_eq!(value, 42);
        assert_eq!(
            *runtime.calls.lock().expect("calls should lock"),
            vec![
                OfficeLifecycleOperation::Open,
                OfficeLifecycleOperation::Close
            ]
        );
    }

    #[test]
    fn failure_and_cancellation_still_close_owned_session() {
        for expected_kind in [
            OfficeServiceErrorKind::OperationFailed,
            OfficeServiceErrorKind::Cancelled,
        ] {
            let root = tempfile::tempdir().expect("temporary directory should exist");
            let path = root.path().join("fixture.docx");
            fs::write(&path, b"fixture").expect("fixture should be written");
            let runtime = Arc::new(RecordingRuntime::default());
            let service = OfficeService::new(runtime.clone());

            let error = tauri::async_runtime::block_on(service.with_document(
                &path,
                OfficeCancellationToken::default(),
                move |_runtime, _document, cancellation| async move {
                    if expected_kind == OfficeServiceErrorKind::Cancelled {
                        cancellation.cancel();
                    }
                    Err::<(), _>(OfficeServiceError::new(expected_kind))
                },
            ))
            .expect_err("operation should fail");

            assert_eq!(error.kind(), expected_kind);
            assert_eq!(
                *runtime.calls.lock().expect("calls should lock"),
                vec![
                    OfficeLifecycleOperation::Open,
                    OfficeLifecycleOperation::Close
                ]
            );
        }
    }

    #[test]
    fn rejects_bad_paths_before_starting_runtime() {
        let runtime = Arc::new(RecordingRuntime::default());
        let service = OfficeService::new(runtime.clone());
        let error = tauri::async_runtime::block_on(service.open_document(
            Path::new("missing.pdf"),
            &OfficeCancellationToken::default(),
        ))
        .expect_err("unsupported format should fail");

        assert_eq!(error.kind(), OfficeServiceErrorKind::FormatUnsupported);
        assert!(runtime.calls.lock().expect("calls should lock").is_empty());
    }

    #[test]
    fn serializes_operations_for_the_same_normalized_document() {
        let root = tempfile::tempdir().expect("temporary directory should exist");
        let path = root.path().join("fixture.docx");
        fs::write(&path, b"fixture").expect("fixture should be written");
        let runtime = Arc::new(RecordingRuntime::default());
        let service = Arc::new(OfficeService::new(runtime));
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));

        tauri::async_runtime::block_on(async {
            let mut tasks = Vec::new();
            for _ in 0..2 {
                let service = Arc::clone(&service);
                let path = path.clone();
                let active = Arc::clone(&active);
                let maximum = Arc::clone(&maximum);
                tasks.push(tokio::spawn(async move {
                    service
                        .with_document(
                            &path,
                            OfficeCancellationToken::default(),
                            move |_runtime, _document, _cancellation| async move {
                                let current = active.fetch_add(1, Ordering::AcqRel) + 1;
                                maximum.fetch_max(current, Ordering::AcqRel);
                                tokio::time::sleep(Duration::from_millis(25)).await;
                                active.fetch_sub(1, Ordering::AcqRel);
                                Ok(())
                            },
                        )
                        .await
                }));
            }
            for task in tasks {
                task.await
                    .expect("operation task should join")
                    .expect("operation should succeed");
            }
        });

        assert_eq!(maximum.load(Ordering::Acquire), 1);
    }

    #[test]
    fn does_not_take_ownership_of_a_reused_session() {
        let fixture = tempfile::Builder::new()
            .suffix(".docx")
            .tempfile()
            .expect("fixture should be created");
        let runtime = Arc::new(RecordingRuntime {
            reuses_existing_session: true,
            ..RecordingRuntime::default()
        });
        let service = OfficeService::new(runtime.clone());

        tauri::async_runtime::block_on(service.with_document(
            fixture.path(),
            OfficeCancellationToken::default(),
            |_runtime, _document, _cancellation| async { Ok(()) },
        ))
        .expect("operation should use existing session");

        assert_eq!(
            *runtime.calls.lock().expect("calls should lock"),
            vec![OfficeLifecycleOperation::Open]
        );
    }

    #[test]
    fn creates_without_overwriting_and_closes_before_commit() {
        let root = tempfile::tempdir().expect("temporary directory should exist");
        let path = root.path().join("created.docx");
        let runtime = Arc::new(RecordingRuntime::default());
        let service = OfficeService::new(runtime.clone());

        tauri::async_runtime::block_on(
            service.create_document(&path, OfficeCancellationToken::default()),
        )
        .expect("document should be created");
        assert_eq!(
            fs::read(&path).expect("document should be readable"),
            b"created"
        );
        assert_eq!(
            *runtime.calls.lock().expect("calls should lock"),
            vec![
                OfficeLifecycleOperation::Open,
                OfficeLifecycleOperation::Close
            ]
        );

        let error = tauri::async_runtime::block_on(
            service.create_document(&path, OfficeCancellationToken::default()),
        )
        .expect_err("existing output must not be replaced");
        assert_eq!(error.kind(), OfficeServiceErrorKind::OutputConflict);
    }

    #[test]
    fn applies_ordered_operations_to_a_copy_and_preserves_the_original() {
        let root = tempfile::tempdir().expect("temporary directory should exist");
        let source = root.path().join("source.docx");
        let output = root.path().join("output.docx");
        fs::write(&source, b"original").expect("source should exist");
        let operations = vec![
            OfficeDocumentOperation::AddWordParagraph {
                text: "first".to_owned(),
            },
            OfficeDocumentOperation::AddWordParagraph {
                text: "second".to_owned(),
            },
        ];
        let runtime = Arc::new(RecordingRuntime::default());
        let service = OfficeService::new(runtime.clone());

        tauri::async_runtime::block_on(service.apply_operations(
            &source,
            &output,
            &operations,
            OfficeCancellationToken::default(),
        ))
        .expect("operations should succeed");

        assert_eq!(
            fs::read(&source).expect("source should remain"),
            b"original"
        );
        assert_eq!(fs::read(&output).expect("output should exist"), b"edited");
        assert_eq!(
            *runtime.operations.lock().expect("operations should lock"),
            operations
        );
    }

    #[test]
    fn failed_batch_never_publishes_its_staged_output() {
        let root = tempfile::tempdir().expect("temporary directory should exist");
        let source = root.path().join("source.docx");
        let output = root.path().join("output.docx");
        fs::write(&source, b"original").expect("source should exist");
        let runtime = Arc::new(RecordingRuntime {
            fail_batch: true,
            ..RecordingRuntime::default()
        });
        let service = OfficeService::new(runtime);

        let error = tauri::async_runtime::block_on(service.apply_operations(
            &source,
            &output,
            &[OfficeDocumentOperation::AddWordParagraph {
                text: "partial".to_owned(),
            }],
            OfficeCancellationToken::default(),
        ))
        .expect_err("failed batch should not commit");

        assert_eq!(error.kind(), OfficeServiceErrorKind::OperationFailed);
        assert_eq!(
            fs::read(&source).expect("source should remain"),
            b"original"
        );
        assert!(!output.exists());
    }

    #[test]
    fn validates_excel_bounds_before_runtime_execution() {
        assert!(valid_cell_reference("A1"));
        assert!(valid_cell_reference("XFD1048576"));
        for invalid in ["A0", "XFE1", "a1", "A1048577", "A1:B2"] {
            assert!(!valid_cell_reference(invalid));
        }
    }

    #[test]
    fn returns_structured_inspection_and_cleans_preview_files() {
        let fixture = tempfile::Builder::new()
            .suffix(".pptx")
            .tempfile()
            .expect("fixture should exist");
        let runtime = Arc::new(RecordingRuntime::default());
        let service = OfficeService::new(runtime.clone());

        let inspection = tauri::async_runtime::block_on(
            service.inspect_document(fixture.path(), OfficeCancellationToken::default()),
        )
        .expect("inspection should succeed");
        assert_eq!(inspection.structure, serde_json::json!({ "matches": 1 }));

        let preview = tauri::async_runtime::block_on(service.render_preview(
            fixture.path(),
            1,
            OfficeCancellationToken::default(),
        ))
        .expect("preview should succeed");
        assert_eq!(preview.mime_type, "image/png");
        assert!(preview.data_url.starts_with("data:image/png;base64,"));
        assert!(runtime
            .preview_paths
            .lock()
            .expect("preview paths should lock")
            .iter()
            .all(|path| !path.exists()));
    }
}
