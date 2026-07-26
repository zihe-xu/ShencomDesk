use std::{fs, panic, path::Path};

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{
    filter::{filter_fn, EnvFilter, LevelFilter},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    Layer,
};

use crate::utils::AppError;

pub const OPERATION_TARGET: &str = "shendesk::operation";

/// Keeps non-blocking logging workers alive for the full application lifecycle.
#[derive(Debug)]
pub struct LoggingGuards {
    _app: WorkerGuard,
    _error: WorkerGuard,
    _operation: WorkerGuard,
}

/// Initializes structured logging under the provided application log directory.
pub fn initialize(log_dir: &Path) -> Result<LoggingGuards, AppError> {
    fs::create_dir_all(log_dir)
        .map_err(|error| AppError::new(format!("failed to create log directory: {error}")))?;

    let app_appender = tracing_appender::rolling::never(log_dir, "app.log");
    let error_appender = tracing_appender::rolling::never(log_dir, "error.log");
    let operation_appender = tracing_appender::rolling::never(log_dir, "operation.log");

    let (app_writer, app_guard) = tracing_appender::non_blocking(app_appender);
    let (error_writer, error_guard) = tracing_appender::non_blocking(error_appender);
    let (operation_writer, operation_guard) = tracing_appender::non_blocking(operation_appender);

    let environment_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let app_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_writer(app_writer)
        .with_filter(filter_fn(|metadata| metadata.target() != OPERATION_TARGET));

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
        .with(environment_filter)
        .with(app_layer)
        .with(error_layer)
        .with(operation_layer)
        .try_init()
        .map_err(|error| AppError::new(format!("failed to initialize tracing subscriber: {error}")))?;

    install_panic_hook();

    tracing::info!(log_dir = %log_dir.display(), "logging initialized");
    record_operation("logging.initialize", "success");

    Ok(LoggingGuards {
        _app: app_guard,
        _error: error_guard,
        _operation: operation_guard,
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

fn install_panic_hook() {
    let previous_hook = panic::take_hook();

    panic::set_hook(Box::new(move |panic_info| {
        tracing::error!(target: "shendesk::panic", "unhandled application panic");
        previous_hook(panic_info);
    }));
}
