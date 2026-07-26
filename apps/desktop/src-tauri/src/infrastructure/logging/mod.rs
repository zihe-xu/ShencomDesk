use std::{any::Any, fs, panic, path::Path, sync::Mutex};

use tracing_appender::{
    non_blocking::WorkerGuard,
    rolling::{RollingFileAppender, Rotation},
};
use tracing_subscriber::{
    filter::{filter_fn, EnvFilter, FilterExt, LevelFilter},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    Layer,
};

use crate::utils::AppError;

pub const OPERATION_TARGET: &str = "shendesk::operation";
const LOG_RETENTION_FILES: usize = 15;

#[derive(Debug)]
struct LoggingWorkers {
    _app: WorkerGuard,
    _error: WorkerGuard,
    _operation: WorkerGuard,
}

/// Keeps non-blocking logging workers alive for the full application lifecycle.
#[derive(Debug)]
pub struct LoggingGuards {
    workers: Mutex<Option<LoggingWorkers>>,
}

impl LoggingGuards {
    /// Drops worker guards so all buffered log events are flushed before process exit.
    pub fn shutdown(&self) -> Result<(), AppError> {
        let workers = self
            .workers
            .lock()
            .map_err(|error| AppError::new(format!("failed to lock logging workers: {error}")))?
            .take();
        drop(workers);

        Ok(())
    }
}

/// Initializes structured logging under the provided application log directory.
pub fn initialize(log_dir: &Path) -> Result<LoggingGuards, AppError> {
    fs::create_dir_all(log_dir)
        .map_err(|error| AppError::new(format!("failed to create log directory: {error}")))?;

    let app_appender = build_daily_appender(log_dir, "app.log")?;
    let error_appender = build_daily_appender(log_dir, "error.log")?;
    let operation_appender = build_daily_appender(log_dir, "operation.log")?;

    let (app_writer, app_guard) = tracing_appender::non_blocking(app_appender);
    let (error_writer, error_guard) = tracing_appender::non_blocking(error_appender);
    let (operation_writer, operation_guard) = tracing_appender::non_blocking(operation_appender);

    let environment_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let app_filter =
        environment_filter.and(filter_fn(|metadata| metadata.target() != OPERATION_TARGET));

    let app_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_writer(app_writer)
        .with_filter(app_filter);

    let error_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_writer(error_writer)
        .with_filter(LevelFilter::ERROR);

    let operation_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_target(false)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_writer(operation_writer)
        .with_filter(filter_fn(|metadata| metadata.target() == OPERATION_TARGET));

    tracing_subscriber::registry()
        .with(app_layer)
        .with(error_layer)
        .with(operation_layer)
        .try_init()
        .map_err(|error| {
            AppError::new(format!("failed to initialize tracing subscriber: {error}"))
        })?;

    install_panic_hook();

    tracing::info!(log_dir = %log_dir.display(), retention_files = LOG_RETENTION_FILES, "logging initialized");
    record_operation("logging.initialize", "success");

    Ok(LoggingGuards {
        workers: Mutex::new(Some(LoggingWorkers {
            _app: app_guard,
            _error: error_guard,
            _operation: operation_guard,
        })),
    })
}

/// Records a user-visible or business operation in the dedicated operation log.
pub fn record_operation(operation: &str, outcome: &str) {
    tracing::info!(
        target: OPERATION_TARGET,
        operation = operation,
        outcome = outcome,
        "operation"
    );
}

fn build_daily_appender(log_dir: &Path, prefix: &str) -> Result<RollingFileAppender, AppError> {
    RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(prefix)
        .max_log_files(LOG_RETENTION_FILES)
        .build(log_dir)
        .map_err(|error| AppError::new(format!("failed to create {prefix} appender: {error}")))
}

fn install_panic_hook() {
    let previous_hook = panic::take_hook();

    panic::set_hook(Box::new(move |panic_info| {
        let message = panic_payload(panic_info.payload());
        let thread = std::thread::current();
        let thread_name = thread.name().unwrap_or("unnamed");
        let (file, line, column) = panic_info
            .location()
            .map(|location| (location.file(), location.line(), location.column()))
            .unwrap_or(("unknown", 0, 0));

        tracing::error!(
            target: "shendesk::panic",
            panic_message = %message,
            panic_file = file,
            panic_line = line,
            panic_column = column,
            thread_name,
            "unhandled application panic"
        );
        previous_hook(panic_info);
    }));
}

fn panic_payload(payload: &(dyn Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string panic payload".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use std::{fs::File, time::SystemTime};

    use super::*;

    #[test]
    fn keeps_at_most_configured_number_of_rotated_files() {
        let log_dir = unique_test_log_dir();
        fs::create_dir_all(&log_dir).expect("test log directory should be created");

        for day in 1..=25 {
            let path = log_dir.join(format!("app.log.2025-01-{day:02}"));
            File::create(path).expect("historical log fixture should be created");
        }

        drop(build_daily_appender(&log_dir, "app.log").expect("appender should initialize"));

        let retained = fs::read_dir(&log_dir)
            .expect("test log directory should be readable")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().starts_with("app.log."))
            .count();

        assert!(retained <= LOG_RETENTION_FILES);
        fs::remove_dir_all(log_dir).expect("test log directory should be removed");
    }

    #[test]
    fn formats_string_and_non_string_panic_payloads() {
        let borrowed: &(dyn Any + Send) = &"borrowed panic";
        let owned: &(dyn Any + Send) = &String::from("owned panic");
        let numeric: &(dyn Any + Send) = &42_u32;

        assert_eq!(panic_payload(borrowed), "borrowed panic");
        assert_eq!(panic_payload(owned), "owned panic");
        assert_eq!(panic_payload(numeric), "non-string panic payload");
    }

    fn unique_test_log_dir() -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("shendesk-logs-{nonce}"))
    }
}
