use std::time::{Duration, Instant};

/// Shared runtime state managed by Tauri.
#[derive(Debug)]
pub struct AppState {
    started_at: Instant,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            started_at: Instant::now(),
        }
    }

    pub fn uptime(&self) -> Duration {
        self.started_at.elapsed()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
