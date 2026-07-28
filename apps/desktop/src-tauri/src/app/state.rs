use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use crate::application::{
    auth_service::{AuthBackend, AuthService},
    event_bus::EventBus,
    file_service::{FileRepository, FileService},
    plugin_service::{PluginRepository, PluginRuntime, PluginService},
    task_service::TaskManager,
    update_service::{UpdateBackend, UpdateService},
};

/// Shared runtime state managed by Tauri.
#[derive(Debug)]
pub struct AppState {
    started_at: Instant,
    event_bus: EventBus,
    task_manager: TaskManager,
    auth_service: AuthService,
    file_service: FileService,
    plugin_service: PluginService,
    update_service: UpdateService,
}

impl AppState {
    pub fn new(
        file_repository: Arc<dyn FileRepository>,
        plugin_repository: Arc<dyn PluginRepository>,
        plugin_runtime: Arc<dyn PluginRuntime>,
        update_backend: Arc<dyn UpdateBackend>,
        auth_backend: Arc<dyn AuthBackend>,
    ) -> Self {
        let event_bus = EventBus::default();
        let task_manager = TaskManager::with_events(event_bus.clone());
        let auth_service = AuthService::new(auth_backend);
        let file_service = FileService::new(file_repository, event_bus.clone());
        let plugin_service =
            PluginService::new(plugin_repository, plugin_runtime, event_bus.clone());
        let update_service = UpdateService::new(update_backend, event_bus.clone());

        Self {
            started_at: Instant::now(),
            event_bus,
            task_manager,
            auth_service,
            file_service,
            plugin_service,
            update_service,
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

    pub fn auth_service(&self) -> &AuthService {
        &self.auth_service
    }

    pub fn file_service(&self) -> &FileService {
        &self.file_service
    }

    pub fn plugin_service(&self) -> &PluginService {
        &self.plugin_service
    }

    pub fn update_service(&self) -> &UpdateService {
        &self.update_service
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use async_trait::async_trait;

    use crate::{
        application::{
            auth_service::{AuthBackendResponse, AuthServiceError},
            update_service::{
                UpdateProgressHandler, UpdateServiceError, UpdateServiceErrorKind,
            },
        },
        domain::{auth::LoginRequest, event::EventKind, update::UpdateInfo},
        infrastructure::{
            filesystem::LocalFileRepository,
            plugins::{LocalPluginRepository, WasmtimePluginRuntime},
        },
    };

    use super::*;

    #[derive(Debug, Default)]
    struct NoopUpdateBackend;

    #[derive(Debug, Default)]
    struct NoopAuthBackend;

    #[async_trait]
    impl AuthBackend for NoopAuthBackend {
        async fn login(
            &self,
            _request: &LoginRequest,
        ) -> Result<AuthBackendResponse, AuthServiceError> {
            Err(AuthServiceError::unavailable("not configured for state test"))
        }
    }

    #[async_trait]
    impl UpdateBackend for NoopUpdateBackend {
        async fn check(&self) -> Result<Option<UpdateInfo>, UpdateServiceError> {
            Ok(None)
        }

        async fn install(
            &self,
            _on_progress: UpdateProgressHandler,
        ) -> Result<(), UpdateServiceError> {
            Err(UpdateServiceError::new(
                UpdateServiceErrorKind::NoPendingUpdate,
                "no update",
            ))
        }
    }

    #[test]
    fn core_services_share_the_application_event_bus() {
        let plugin_root = std::env::temp_dir().join(format!(
            "shendesk-state-plugin-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let state = AppState::new(
            Arc::new(LocalFileRepository::default()),
            Arc::new(
                LocalPluginRepository::new(&plugin_root)
                    .expect("plugin repository should initialize"),
            ),
            Arc::new(WasmtimePluginRuntime::new().expect("plugin runtime should initialize")),
            Arc::new(NoopUpdateBackend),
            Arc::new(NoopAuthBackend),
        );
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
        assert_eq!(state.plugin_service().shutdown(), 0);
        let _ = fs::remove_dir_all(plugin_root);
    }
}
