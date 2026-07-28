//! Thin Tauri IPC adapters. Commands validate transport input and delegate to application services.

pub mod config;
pub mod error;
pub mod file;
pub mod health;
pub mod auth;
pub mod plugin;
pub mod task;
pub mod update;
