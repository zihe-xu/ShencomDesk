use std::{error::Error, fmt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppErrorKind {
    Database,
    Configuration,
    Validation,
    Internal,
}

/// Application-level error used at layer boundaries.
#[derive(Debug, Clone)]
pub struct AppError {
    kind: AppErrorKind,
    message: String,
}

impl AppError {
    pub fn new(message: impl Into<String>) -> Self {
        Self::internal(message)
    }

    pub fn database(message: impl Into<String>) -> Self {
        Self::with_kind(AppErrorKind::Database, message)
    }

    pub fn configuration(message: impl Into<String>) -> Self {
        Self::with_kind(AppErrorKind::Configuration, message)
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::with_kind(AppErrorKind::Validation, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::with_kind(AppErrorKind::Internal, message)
    }

    pub fn kind(&self) -> AppErrorKind {
        self.kind
    }

    fn with_kind(kind: AppErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for AppError {}
