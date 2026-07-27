use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use crate::application::{
    event_bus::EventBus,
    file_service::{FileRepository, FileService},
    task_service::TaskManager,
};

/// Shared runtime state managed by Tauri.
#[derive(Debug)]
pub struct AppState {
    started_at: Instant,
    event_bus: EventBus,
    task_manager: TaskManager,
    file_service: FileService,
}

impl AppState {
    pub fn new(file_repository: Arc<dyn FileRepository>) -> Self {
        let event_bus = EventBus::default();
        let task_manager = TaskManager::with_events(event_bus.clone());
        let file_service = FileService::new(file_repository, event_bus.clone());

        Self {
            started_at: Instant::now(),
            event_bus,
            task_manager,
            file_service,
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

    pub fn file_service(&self) -> &FileService {
        &self.file_service
    }
}

#[cfg(test)]
mod tests {
    use crate::{domain::event::EventKind, infrastructure::filesystem::LocalFileRepository};

    use super::*;

    #[test]
    fn core_services_share_the_application_event_bus() {
        let state = AppState::new(Arc::new(LocalFileRepository::default()));
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
        assert_eq!(state.file_service().shutdown(), 0);
    }
}
