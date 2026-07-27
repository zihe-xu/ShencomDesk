use std::time::{Duration, Instant};

use crate::application::{event_bus::EventBus, task_service::TaskManager};

/// Shared runtime state managed by Tauri.
#[derive(Debug)]
pub struct AppState {
    started_at: Instant,
    event_bus: EventBus,
    task_manager: TaskManager,
}

impl AppState {
    pub fn new() -> Self {
        let event_bus = EventBus::default();
        let task_manager = TaskManager::with_events(event_bus.clone());

        Self {
            started_at: Instant::now(),
            event_bus,
            task_manager,
        }
    }

    pub fn uptime(&self) -> Duration {
        self.started_at.elapsed()
    }

    pub fn event_bus(&self) -> &EventBus {
        &self.event_bus
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

#[cfg(test)]
mod tests {
    use crate::domain::event::EventKind;

    use super::*;

    #[test]
    fn task_manager_and_modules_share_the_application_event_bus() {
        let state = AppState::new();
        let mut subscriber = state.event_bus().subscribe_to([EventKind::TaskFinished]);
        let created = state
            .task_manager()
            .submit("shared bus", 1, |context| {
                context.report_progress(1);
                Ok(())
            })
            .expect("task should be queued");

        let event = tauri::async_runtime::block_on(async {
            subscriber
                .recv()
                .await
                .expect("task finish event should arrive")
        });

        assert_eq!(event.event.kind(), EventKind::TaskFinished);
        assert!(state.task_manager().get(&created.id).is_some());
    }
}
