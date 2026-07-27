use std::{
    error::Error,
    fmt,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::domain::{
    event::AppEvent,
    file::{FileChange, FileIndex, FileReadResult, FileWatch, FileWatchId},
};

use super::event_bus::EventBus;

pub const DEFAULT_MAX_READ_BYTES: u64 = 4 * 1024 * 1024;
pub const MAX_READ_BYTES: u64 = 16 * 1024 * 1024;
pub const DEFAULT_MAX_INDEX_ENTRIES: usize = 5_000;
pub const MAX_INDEX_ENTRIES: usize = 20_000;
pub const DEFAULT_MAX_INDEX_DEPTH: usize = 16;
pub const MAX_INDEX_DEPTH: usize = 64;
const MAX_PATH_CHARS: usize = 4_096;
const MAX_WATCH_ID_CHARS: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileServiceErrorKind {
    InvalidInput,
    NotFound,
    PermissionDenied,
    NotAFile,
    NotADirectory,
    TooLarge,
    NonUtf8,
    WatchUnavailable,
    WatchNotFound,
    Io,
}

#[derive(Debug, Clone)]
pub struct FileServiceError {
    kind: FileServiceErrorKind,
    message: String,
}

impl FileServiceError {
    pub fn new(kind: FileServiceErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(FileServiceErrorKind::InvalidInput, message)
    }

    pub fn kind(&self) -> FileServiceErrorKind {
        self.kind
    }
}

impl fmt::Display for FileServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for FileServiceError {}

pub type FileChangeHandler = Arc<dyn Fn(FileChange) + Send + Sync + 'static>;

/// Application-owned port for concrete local filesystem implementations.
///
/// Infrastructure adapters may use platform APIs, `notify`, and in-memory
/// caches, while use cases stay independent from those implementation details.
pub trait FileRepository: Send + Sync {
    fn read_text(&self, path: &Path, max_bytes: u64) -> Result<FileReadResult, FileServiceError>;

    fn index_directory(
        &self,
        root: &Path,
        max_entries: usize,
        max_depth: usize,
    ) -> Result<FileIndex, FileServiceError>;

    fn start_watch(
        &self,
        path: &Path,
        recursive: bool,
        on_change: FileChangeHandler,
    ) -> Result<FileWatch, FileServiceError>;

    fn stop_watch(&self, watch_id: &FileWatchId) -> Result<bool, FileServiceError>;

    fn clear_cache(&self);

    /// Stops active watches and returns how many registrations were removed.
    fn shutdown(&self) -> usize;
}

#[derive(Clone)]
pub struct FileService {
    repository: Arc<dyn FileRepository>,
    event_bus: EventBus,
}

impl fmt::Debug for FileService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FileService")
            .finish_non_exhaustive()
    }
}

impl FileService {
    pub fn new(repository: Arc<dyn FileRepository>, event_bus: EventBus) -> Self {
        Self {
            repository,
            event_bus,
        }
    }

    pub fn read_text_file(
        &self,
        path: impl AsRef<str>,
        max_bytes: Option<u64>,
    ) -> Result<FileReadResult, FileServiceError> {
        let path = validate_path(path.as_ref())?;
        let max_bytes = max_bytes.unwrap_or(DEFAULT_MAX_READ_BYTES);
        if max_bytes == 0 || max_bytes > MAX_READ_BYTES {
            return Err(FileServiceError::invalid_input(format!(
                "max_bytes must be between 1 and {MAX_READ_BYTES}"
            )));
        }

        self.repository.read_text(&path, max_bytes)
    }

    pub fn index_directory(
        &self,
        root: impl AsRef<str>,
        max_entries: Option<usize>,
        max_depth: Option<usize>,
    ) -> Result<FileIndex, FileServiceError> {
        let root = validate_path(root.as_ref())?;
        let max_entries = max_entries.unwrap_or(DEFAULT_MAX_INDEX_ENTRIES);
        let max_depth = max_depth.unwrap_or(DEFAULT_MAX_INDEX_DEPTH);

        if max_entries == 0 || max_entries > MAX_INDEX_ENTRIES {
            return Err(FileServiceError::invalid_input(format!(
                "max_entries must be between 1 and {MAX_INDEX_ENTRIES}"
            )));
        }
        if max_depth > MAX_INDEX_DEPTH {
            return Err(FileServiceError::invalid_input(format!(
                "max_depth must not exceed {MAX_INDEX_DEPTH}"
            )));
        }

        self.repository
            .index_directory(&root, max_entries, max_depth)
    }

    pub fn start_watch(
        &self,
        path: impl AsRef<str>,
        recursive: bool,
    ) -> Result<FileWatch, FileServiceError> {
        let path = validate_path(path.as_ref())?;
        let event_bus = self.event_bus.clone();
        let on_change: FileChangeHandler = Arc::new(move |change| {
            event_bus.publish(AppEvent::FileChanged { change });
        });

        self.repository.start_watch(&path, recursive, on_change)
    }

    pub fn stop_watch(&self, watch_id: impl AsRef<str>) -> Result<FileWatchId, FileServiceError> {
        let watch_id = validate_watch_id(watch_id.as_ref())?;
        if self.repository.stop_watch(&watch_id)? {
            Ok(watch_id)
        } else {
            Err(FileServiceError::new(
                FileServiceErrorKind::WatchNotFound,
                "file watch registration was not found",
            ))
        }
    }

    pub fn clear_cache(&self) {
        self.repository.clear_cache();
    }

    pub fn shutdown(&self) -> usize {
        self.repository.shutdown()
    }
}

fn validate_path(value: &str) -> Result<PathBuf, FileServiceError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > MAX_PATH_CHARS {
        return Err(FileServiceError::invalid_input("file path is invalid"));
    }

    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(FileServiceError::invalid_input(
            "file path must be absolute",
        ));
    }

    Ok(path)
}

fn validate_watch_id(value: &str) -> Result<FileWatchId, FileServiceError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > MAX_WATCH_ID_CHARS {
        return Err(FileServiceError::invalid_input("watch id is invalid"));
    }

    Ok(FileWatchId::new(value))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use crate::domain::file::{FileChangeKind, FileEntry};

    use super::*;

    #[derive(Default)]
    struct RecordingRepository {
        changes: Mutex<Option<FileChangeHandler>>,
    }

    impl FileRepository for RecordingRepository {
        fn read_text(
            &self,
            path: &Path,
            _max_bytes: u64,
        ) -> Result<FileReadResult, FileServiceError> {
            Ok(FileReadResult {
                entry: FileEntry {
                    path: path.to_string_lossy().into_owned(),
                    name: "example.txt".to_owned(),
                    extension: Some("txt".to_owned()),
                    size_bytes: 5,
                    modified_at_unix_ms: Some(1),
                    is_directory: false,
                },
                content: "hello".to_owned(),
                from_cache: false,
            })
        }

        fn index_directory(
            &self,
            root: &Path,
            _max_entries: usize,
            _max_depth: usize,
        ) -> Result<FileIndex, FileServiceError> {
            Ok(FileIndex {
                root: root.to_string_lossy().into_owned(),
                entries: Vec::new(),
                scanned_at_unix_ms: 1,
                truncated: false,
            })
        }

        fn start_watch(
            &self,
            path: &Path,
            recursive: bool,
            on_change: FileChangeHandler,
        ) -> Result<FileWatch, FileServiceError> {
            *self
                .changes
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(on_change);
            Ok(FileWatch {
                id: FileWatchId::new("watch-1"),
                root: path.to_string_lossy().into_owned(),
                recursive,
            })
        }

        fn stop_watch(&self, watch_id: &FileWatchId) -> Result<bool, FileServiceError> {
            Ok(watch_id.0 == "watch-1")
        }

        fn clear_cache(&self) {}

        fn shutdown(&self) -> usize {
            0
        }
    }

    #[test]
    fn validates_absolute_paths_and_operation_limits() {
        let repository = Arc::new(RecordingRepository::default());
        let service = FileService::new(repository, EventBus::default());

        assert_eq!(
            service
                .read_text_file("relative.txt", None)
                .expect_err("relative paths should be rejected")
                .kind(),
            FileServiceErrorKind::InvalidInput
        );
        assert_eq!(
            service
                .read_text_file("/tmp/example.txt", Some(MAX_READ_BYTES + 1))
                .expect_err("oversized read limit should be rejected")
                .kind(),
            FileServiceErrorKind::InvalidInput
        );
        assert_eq!(
            service
                .index_directory("/tmp", Some(MAX_INDEX_ENTRIES + 1), None)
                .expect_err("oversized index should be rejected")
                .kind(),
            FileServiceErrorKind::InvalidInput
        );
    }

    #[test]
    fn publishes_file_changes_from_repository_callbacks() {
        let repository = Arc::new(RecordingRepository::default());
        let event_bus = EventBus::default();
        let mut subscriber = event_bus.subscribe();
        let service = FileService::new(repository.clone(), event_bus);

        service
            .start_watch("/tmp", true)
            .expect("watch should start");
        let callback = repository
            .changes
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
            .expect("callback should be registered");
        callback(FileChange {
            watch_id: FileWatchId::new("watch-1"),
            path: "/tmp/example.txt".to_owned(),
            kind: FileChangeKind::Modified,
        });

        let envelope = subscriber
            .try_recv()
            .expect("subscriber should remain open")
            .expect("file event should arrive");
        assert!(matches!(envelope.event, AppEvent::FileChanged { .. }));
    }

    #[test]
    fn returns_stable_not_found_error_for_unknown_watch() {
        let repository = Arc::new(RecordingRepository::default());
        let service = FileService::new(repository, EventBus::default());

        assert_eq!(
            service
                .stop_watch("unknown")
                .expect_err("unknown watch should fail")
                .kind(),
            FileServiceErrorKind::WatchNotFound
        );
    }
}
