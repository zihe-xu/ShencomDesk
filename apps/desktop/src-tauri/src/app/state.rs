use std::time::{Duration, Instant};

use crate::application::task_service::TaskManager;

/// Shared runtime state managed by Tauri.
#[derive(Debug)]
pub struct AppState {
    started_at: Instant,
    task_manager: TaskManager,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            started_at: Instant::now(),
            task_manager: TaskManager::default(),
        }
    }

    pub fn uptime(&self) -> Duration {
        self.started_at.elapsed()
    }

    pub fn task_manager(&self) -> &TaskManager {
        &self.task_manager
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
