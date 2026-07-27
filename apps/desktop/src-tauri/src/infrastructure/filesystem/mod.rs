use std::{
    collections::BTreeMap,
    fs::{self, File, Metadata},
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex, MutexGuard,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use moka::sync::Cache;
use notify::{EventKind as NotifyEventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::{
    application::file_service::{
        FileChangeHandler, FileRepository, FileServiceError, FileServiceErrorKind,
    },
    domain::file::{
        FileChange, FileChangeKind, FileEntry, FileIndex, FileReadResult, FileWatch, FileWatchId,
    },
};

const DEFAULT_CACHE_CAPACITY: u64 = 256;
const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileSignature {
    size_bytes: u64,
    modified_at_unix_ns: Option<u128>,
}

#[derive(Debug, Clone)]
struct CachedText {
    signature: FileSignature,
    entry: FileEntry,
    content: String,
}

struct WatchRegistration {
    watcher: RecommendedWatcher,
    root: PathBuf,
}

/// Native local filesystem adapter backed by `notify` and a bounded `moka`
/// text-content cache.
pub struct LocalFileRepository {
    text_cache: Cache<PathBuf, CachedText>,
    watches: Mutex<BTreeMap<FileWatchId, WatchRegistration>>,
    next_watch_id: AtomicU64,
}

impl std::fmt::Debug for LocalFileRepository {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LocalFileRepository")
            .field("watch_count", &lock_mutex(&self.watches).len())
            .finish_non_exhaustive()
    }
}

impl LocalFileRepository {
    pub fn new(cache_capacity: u64, cache_ttl: Duration) -> Self {
        assert!(cache_capacity > 0, "file cache capacity must be positive");
        assert!(!cache_ttl.is_zero(), "file cache TTL must be positive");

        Self {
            text_cache: Cache::builder()
                .max_capacity(cache_capacity)
                .time_to_live(cache_ttl)
                .build(),
            watches: Mutex::new(BTreeMap::new()),
            next_watch_id: AtomicU64::new(1),
        }
    }

    fn canonicalize_existing(path: &Path) -> Result<PathBuf, FileServiceError> {
        fs::canonicalize(path).map_err(|error| map_io_error(error, "failed to resolve file path"))
    }
}

impl Default for LocalFileRepository {
    fn default() -> Self {
        Self::new(DEFAULT_CACHE_CAPACITY, DEFAULT_CACHE_TTL)
    }
}

impl FileRepository for LocalFileRepository {
    fn read_text(&self, path: &Path, max_bytes: u64) -> Result<FileReadResult, FileServiceError> {
        let path = Self::canonicalize_existing(path)?;
        let metadata = fs::metadata(&path)
            .map_err(|error| map_io_error(error, "failed to read file metadata"))?;
        if !metadata.is_file() {
            return Err(FileServiceError::new(
                FileServiceErrorKind::NotAFile,
                "requested path is not a regular file",
            ));
        }
        if metadata.len() > max_bytes {
            return Err(FileServiceError::new(
                FileServiceErrorKind::TooLarge,
                "file exceeds the configured read limit",
            ));
        }

        let signature = file_signature(&metadata);
        if let Some(cached) = self.text_cache.get(&path) {
            if cached.signature == signature {
                return Ok(FileReadResult {
                    entry: cached.entry,
                    content: cached.content,
                    from_cache: true,
                });
            }
            self.text_cache.invalidate(&path);
        }

        let mut bytes = Vec::with_capacity(metadata.len().min(max_bytes) as usize);
        File::open(&path)
            .map_err(|error| map_io_error(error, "failed to open file"))?
            .take(max_bytes.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|error| map_io_error(error, "failed to read file"))?;
        if bytes.len() as u64 > max_bytes {
            return Err(FileServiceError::new(
                FileServiceErrorKind::TooLarge,
                "file grew beyond the configured read limit",
            ));
        }

        let content = String::from_utf8(bytes).map_err(|_| {
            FileServiceError::new(
                FileServiceErrorKind::NonUtf8,
                "file is not valid UTF-8 text",
            )
        })?;
        let entry = file_entry(&path, &metadata);
        self.text_cache.insert(
            path,
            CachedText {
                signature,
                entry: entry.clone(),
                content: content.clone(),
            },
        );

        Ok(FileReadResult {
            entry,
            content,
            from_cache: false,
        })
    }

    fn index_directory(
        &self,
        root: &Path,
        max_entries: usize,
        max_depth: usize,
    ) -> Result<FileIndex, FileServiceError> {
        let root = Self::canonicalize_existing(root)?;
        let metadata = fs::metadata(&root)
            .map_err(|error| map_io_error(error, "failed to read index root metadata"))?;
        if !metadata.is_dir() {
            return Err(FileServiceError::new(
                FileServiceErrorKind::NotADirectory,
                "index root is not a directory",
            ));
        }

        let mut entries = Vec::new();
        let mut truncated = false;
        visit_directory(
            &root,
            0,
            max_depth,
            max_entries,
            &mut entries,
            &mut truncated,
        )?;

        Ok(FileIndex {
            root: path_to_string(&root),
            entries,
            scanned_at_unix_ms: unix_time_ms(),
            truncated,
        })
    }

    fn start_watch(
        &self,
        path: &Path,
        recursive: bool,
        on_change: FileChangeHandler,
    ) -> Result<FileWatch, FileServiceError> {
        let root = Self::canonicalize_existing(path)?;
        let metadata = fs::metadata(&root)
            .map_err(|error| map_io_error(error, "failed to read watch root metadata"))?;
        if recursive && !metadata.is_dir() {
            return Err(FileServiceError::new(
                FileServiceErrorKind::InvalidInput,
                "recursive watches require a directory",
            ));
        }

        let sequence = self.next_watch_id.fetch_add(1, Ordering::Relaxed);
        let id = FileWatchId::new(format!("watch-{sequence:016}"));
        let callback_id = id.clone();
        let callback_root = root.clone();
        let cache = self.text_cache.clone();
        let mut watcher = notify::recommended_watcher(move |result| match result {
            Ok(event) => {
                if event.paths.is_empty() {
                    return;
                }

                // Filesystem events can coalesce multiple paths. Clearing the
                // small bounded cache guarantees correctness for renames and
                // directory removals whose old canonical path no longer exists.
                cache.invalidate_all();
                let kind = map_change_kind(&event.kind);
                for path in event.paths {
                    let path = normalize_event_path(&callback_root, path);
                    on_change(FileChange {
                        watch_id: callback_id.clone(),
                        path: path_to_string(&path),
                        kind,
                    });
                }
            }
            Err(error) => {
                tracing::warn!(error = %error, watch_id = %callback_id, "filesystem watcher reported an error");
            }
        })
        .map_err(|error| {
            FileServiceError::new(
                FileServiceErrorKind::WatchUnavailable,
                format!("failed to create filesystem watcher: {error}"),
            )
        })?;

        let recursive_mode = if recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };
        watcher.watch(&root, recursive_mode).map_err(|error| {
            FileServiceError::new(
                FileServiceErrorKind::WatchUnavailable,
                format!("failed to watch filesystem path: {error}"),
            )
        })?;

        let snapshot = FileWatch {
            id: id.clone(),
            root: path_to_string(&root),
            recursive,
        };
        lock_mutex(&self.watches).insert(id, WatchRegistration { watcher, root });
        Ok(snapshot)
    }

    fn stop_watch(&self, watch_id: &FileWatchId) -> Result<bool, FileServiceError> {
        let registration = lock_mutex(&self.watches).remove(watch_id);
        let Some(mut registration) = registration else {
            return Ok(false);
        };

        if let Err(error) = registration.watcher.unwatch(&registration.root) {
            tracing::warn!(error = %error, watch_id = %watch_id, "failed to explicitly unwatch filesystem path");
        }
        Ok(true)
    }

    fn clear_cache(&self) {
        self.text_cache.invalidate_all();
    }

    fn shutdown(&self) -> usize {
        let registrations = {
            let mut watches = lock_mutex(&self.watches);
            std::mem::take(&mut *watches)
        };
        let count = registrations.len();

        for (watch_id, mut registration) in registrations {
            if let Err(error) = registration.watcher.unwatch(&registration.root) {
                tracing::warn!(error = %error, watch_id = %watch_id, "failed to unwatch during shutdown");
            }
        }
        self.text_cache.invalidate_all();
        count
    }
}

impl Drop for LocalFileRepository {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

fn visit_directory(
    directory: &Path,
    depth: usize,
    max_depth: usize,
    max_entries: usize,
    entries: &mut Vec<FileEntry>,
    truncated: &mut bool,
) -> Result<(), FileServiceError> {
    let mut children = fs::read_dir(directory)
        .map_err(|error| map_io_error(error, "failed to read directory"))?
        .filter_map(|entry| match entry {
            Ok(entry) => Some(entry.path()),
            Err(error) => {
                tracing::warn!(error = %error, path = %directory.display(), "skipping unreadable directory entry");
                None
            }
        })
        .collect::<Vec<_>>();
    children.sort();

    for child in children {
        if entries.len() >= max_entries {
            *truncated = true;
            return Ok(());
        }

        let metadata = match fs::symlink_metadata(&child) {
            Ok(metadata) => metadata,
            Err(error) => {
                tracing::warn!(error = %error, path = %child.display(), "skipping unreadable filesystem entry");
                continue;
            }
        };
        if metadata.file_type().is_symlink() {
            continue;
        }

        let is_directory = metadata.is_dir();
        entries.push(file_entry(&child, &metadata));
        if is_directory && depth < max_depth {
            visit_directory(
                &child,
                depth + 1,
                max_depth,
                max_entries,
                entries,
                truncated,
            )?;
            if *truncated {
                return Ok(());
            }
        }
    }

    Ok(())
}

fn file_entry(path: &Path, metadata: &Metadata) -> FileEntry {
    FileEntry {
        path: path_to_string(path),
        name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        extension: path
            .extension()
            .map(|extension| extension.to_string_lossy().into_owned()),
        size_bytes: if metadata.is_file() {
            metadata.len()
        } else {
            0
        },
        modified_at_unix_ms: metadata.modified().ok().and_then(system_time_ms),
        is_directory: metadata.is_dir(),
    }
}

fn file_signature(metadata: &Metadata) -> FileSignature {
    FileSignature {
        size_bytes: metadata.len(),
        modified_at_unix_ns: metadata.modified().ok().and_then(system_time_ns),
    }
}

fn normalize_event_path(root: &Path, path: PathBuf) -> PathBuf {
    let path = if path.is_absolute() {
        path
    } else {
        root.join(path)
    };
    fs::canonicalize(&path).unwrap_or(path)
}

fn map_change_kind(kind: &NotifyEventKind) -> FileChangeKind {
    match kind {
        NotifyEventKind::Create(_) => FileChangeKind::Created,
        NotifyEventKind::Modify(_) => FileChangeKind::Modified,
        NotifyEventKind::Remove(_) => FileChangeKind::Removed,
        _ => FileChangeKind::Other,
    }
}

fn map_io_error(error: io::Error, context: &str) -> FileServiceError {
    let kind = match error.kind() {
        io::ErrorKind::NotFound => FileServiceErrorKind::NotFound,
        io::ErrorKind::PermissionDenied => FileServiceErrorKind::PermissionDenied,
        _ => FileServiceErrorKind::Io,
    };
    FileServiceError::new(kind, format!("{context}: {error}"))
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn unix_time_ms() -> u64 {
    system_time_ms(SystemTime::now()).unwrap_or_default()
}

fn system_time_ms(value: SystemTime) -> Option<u64> {
    value
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
}

fn system_time_ns(value: SystemTime) -> Option<u128> {
    value
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_nanos())
}

fn lock_mutex<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::{mpsc, Arc},
        thread,
        time::Duration,
    };

    use super::*;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(name: &str) -> Self {
            static NEXT_ID: AtomicU64 = AtomicU64::new(1);
            let sequence = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "shendesk-file-service-{name}-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("test directory should be created");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn reads_utf8_text_and_reuses_valid_cache_entries() {
        let directory = TestDirectory::new("read-cache");
        let path = directory.path().join("example.txt");
        fs::write(&path, "hello").expect("file should be written");
        let repository = LocalFileRepository::new(8, Duration::from_secs(60));

        let first = repository
            .read_text(&path, 1024)
            .expect("first read should succeed");
        let second = repository
            .read_text(&path, 1024)
            .expect("cached read should succeed");
        assert!(!first.from_cache);
        assert!(second.from_cache);
        assert_eq!(second.content, "hello");

        fs::write(&path, "hello, updated").expect("file should be updated");
        let updated = repository
            .read_text(&path, 1024)
            .expect("updated read should succeed");
        assert!(!updated.from_cache);
        assert_eq!(updated.content, "hello, updated");
    }

    #[test]
    fn rejects_oversized_and_non_utf8_files() {
        let directory = TestDirectory::new("read-errors");
        let oversized = directory.path().join("large.txt");
        let binary = directory.path().join("binary.dat");
        fs::write(&oversized, "12345").expect("file should be written");
        fs::write(&binary, [0xff, 0xfe]).expect("binary file should be written");
        let repository = LocalFileRepository::default();

        assert_eq!(
            repository
                .read_text(&oversized, 4)
                .expect_err("oversized file should fail")
                .kind(),
            FileServiceErrorKind::TooLarge
        );
        assert_eq!(
            repository
                .read_text(&binary, 16)
                .expect_err("binary file should fail")
                .kind(),
            FileServiceErrorKind::NonUtf8
        );
    }

    #[test]
    fn indexes_nested_files_and_reports_truncation() {
        let directory = TestDirectory::new("index");
        let nested = directory.path().join("nested");
        fs::create_dir_all(&nested).expect("nested directory should be created");
        fs::write(directory.path().join("a.txt"), "a").expect("file should be written");
        fs::write(nested.join("b.txt"), "b").expect("file should be written");
        let repository = LocalFileRepository::default();

        let full = repository
            .index_directory(directory.path(), 10, 4)
            .expect("index should succeed");
        assert!(!full.truncated);
        assert!(full.entries.iter().any(|entry| entry.name == "a.txt"));
        assert!(full.entries.iter().any(|entry| entry.name == "b.txt"));
        assert!(full.entries.iter().any(|entry| entry.name == "nested"));

        let limited = repository
            .index_directory(directory.path(), 1, 4)
            .expect("limited index should succeed");
        assert!(limited.truncated);
        assert_eq!(limited.entries.len(), 1);
    }

    #[test]
    fn watcher_emits_changes_and_stops_by_id() {
        let directory = TestDirectory::new("watch");
        let repository = LocalFileRepository::default();
        let (sender, receiver) = mpsc::channel();
        let callback: FileChangeHandler = Arc::new(move |change| {
            let _ = sender.send(change);
        });
        let watch = repository
            .start_watch(directory.path(), true, callback)
            .expect("watch should start");

        // Give the platform watcher a brief opportunity to finish registering.
        thread::sleep(Duration::from_millis(100));
        fs::write(directory.path().join("created.txt"), "hello")
            .expect("watched file should be created");
        let change = receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("filesystem event should arrive");
        assert_eq!(change.watch_id, watch.id);
        assert!(change.path.ends_with("created.txt"));
        assert!(matches!(
            change.kind,
            FileChangeKind::Created | FileChangeKind::Modified | FileChangeKind::Other
        ));

        assert!(repository.stop_watch(&watch.id).expect("watch should stop"));
        assert!(!repository
            .stop_watch(&watch.id)
            .expect("second stop should be harmless"));
    }
}
