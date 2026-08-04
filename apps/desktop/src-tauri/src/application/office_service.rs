use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fmt,
    future::Future,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use async_trait::async_trait;
use tokio::sync::{Mutex as AsyncMutex, Notify};

use crate::domain::office::{
    OfficeDocument, OfficeDocumentFormat, OfficeEngineStatus, OfficeOperationResult,
};

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

    /// Cancels only transient child processes started by this runtime.
    fn cancel_all(&self) -> usize;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfficeServiceErrorKind {
    EngineUnavailable,
    FormatUnsupported,
    DocumentNotFound,
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
        reuses_existing_session: bool,
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
}
