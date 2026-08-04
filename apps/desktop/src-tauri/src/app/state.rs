use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use crate::application::{
    auth_service::{AuthBackend, AuthService, AuthServiceError, AuthSessionStore},
    event_bus::EventBus,
    file_service::{FileRepository, FileService},
    image_service::{ImageProcessor, ImageService},
    office_service::{OfficeRuntime, OfficeService},
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
    image_service: ImageService,
    office_service: OfficeService,
    plugin_service: PluginService,
    update_service: UpdateService,
}

impl AppState {
    // This is the composition root for application ports; keeping its explicit
    // dependency list avoids introducing a container solely to satisfy a lint.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        file_repository: Arc<dyn FileRepository>,
        image_processor: Arc<dyn ImageProcessor>,
        office_runtime: Arc<dyn OfficeRuntime>,
        plugin_repository: Arc<dyn PluginRepository>,
        plugin_runtime: Arc<dyn PluginRuntime>,
        update_backend: Arc<dyn UpdateBackend>,
        auth_backend: Arc<dyn AuthBackend>,
        auth_session_store: Arc<dyn AuthSessionStore>,
    ) -> Result<Self, AuthServiceError> {
        let event_bus = EventBus::default();
        let task_manager = TaskManager::with_events(event_bus.clone());
        let auth_service = AuthService::new(auth_backend, auth_session_store, event_bus.clone())?;
        let file_service = FileService::new(file_repository, event_bus.clone());
        let image_service = ImageService::new(image_processor);
        let office_service = OfficeService::new(office_runtime);
        let plugin_service =
            PluginService::new(plugin_repository, plugin_runtime, event_bus.clone());
        let update_service = UpdateService::new(update_backend, event_bus.clone());

        Ok(Self {
            started_at: Instant::now(),
            event_bus,
            task_manager,
            auth_service,
            file_service,
            image_service,
            office_service,
            plugin_service,
            update_service,
        })
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

    pub fn image_service(&self) -> &ImageService {
        &self.image_service
    }

    pub fn office_service(&self) -> &OfficeService {
        &self.office_service
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
            auth_service::{AuthBackendResponse, AuthSessionStore, RefreshBackendResponse},
            office_service::{OfficeCancellationToken, OfficeRuntime, OfficeRuntimeError},
            update_service::{UpdateProgressHandler, UpdateServiceError, UpdateServiceErrorKind},
        },
        domain::{
            auth::{AccessToken, LoginRequest},
            event::EventKind,
            office::{
                OfficeDocument, OfficeDocumentOperation, OfficeEngineStatus,
                OfficeLifecycleOperation, OfficeOperationResult,
            },
            update::UpdateInfo,
        },
        infrastructure::{
            filesystem::LocalFileRepository,
            image::LocalImageProcessor,
            plugins::{LocalPluginRepository, WasmtimePluginRuntime},
        },
    };

    use super::*;

    #[derive(Debug, Default)]
    struct NoopUpdateBackend;

    #[derive(Debug, Default)]
    struct NoopAuthBackend;

    #[derive(Debug, Default)]
    struct NoopAuthSessionStore;

    #[derive(Debug, Default)]
    struct NoopOfficeRuntime;

    #[async_trait]
    impl OfficeRuntime for NoopOfficeRuntime {
        async fn probe(&self) -> Result<OfficeEngineStatus, OfficeRuntimeError> {
            Ok(OfficeEngineStatus::unavailable())
        }

        async fn open(
            &self,
            _document: &OfficeDocument,
            _cancellation: &OfficeCancellationToken,
        ) -> Result<OfficeOperationResult, OfficeRuntimeError> {
            Ok(OfficeOperationResult::opened(true))
        }

        async fn close(
            &self,
            _document: &OfficeDocument,
        ) -> Result<OfficeOperationResult, OfficeRuntimeError> {
            Ok(OfficeOperationResult::succeeded(
                OfficeLifecycleOperation::Close,
            ))
        }

        async fn create(
            &self,
            _document: &OfficeDocument,
            _cancellation: &OfficeCancellationToken,
        ) -> Result<OfficeOperationResult, OfficeRuntimeError> {
            Ok(OfficeOperationResult::opened(true))
        }

        async fn inspect(
            &self,
            _document: &OfficeDocument,
            _cancellation: &OfficeCancellationToken,
        ) -> Result<serde_json::Value, OfficeRuntimeError> {
            Ok(serde_json::json!({}))
        }

        async fn apply_batch(
            &self,
            _document: &OfficeDocument,
            _operations: &[OfficeDocumentOperation],
            _cancellation: &OfficeCancellationToken,
        ) -> Result<(), OfficeRuntimeError> {
            Ok(())
        }

        async fn render_preview(
            &self,
            _document: &OfficeDocument,
            _page: u32,
            _output: &std::path::Path,
            _cancellation: &OfficeCancellationToken,
        ) -> Result<(), OfficeRuntimeError> {
            Ok(())
        }

        fn cancel_all(&self) -> usize {
            0
        }
    }

    #[async_trait]
    impl AuthBackend for NoopAuthBackend {
        async fn login(
            &self,
            _request: &LoginRequest,
        ) -> Result<AuthBackendResponse, AuthServiceError> {
            Err(AuthServiceError::unavailable(
                "not configured for state test",
            ))
        }

        async fn refresh(
            &self,
            _refresh_token: &str,
        ) -> Result<RefreshBackendResponse, AuthServiceError> {
            Err(AuthServiceError::unavailable(
                "not configured for state test",
            ))
        }
    }

    impl AuthSessionStore for NoopAuthSessionStore {
        fn load(&self) -> Result<Option<AccessToken>, AuthServiceError> {
            Ok(None)
        }

        fn save(&self, _token: &AccessToken) -> Result<(), AuthServiceError> {
            Ok(())
        }

        fn clear(&self) -> Result<(), AuthServiceError> {
            Ok(())
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
            Arc::new(LocalImageProcessor),
            Arc::new(NoopOfficeRuntime),
            Arc::new(
                LocalPluginRepository::new(&plugin_root)
                    .expect("plugin repository should initialize"),
            ),
            Arc::new(WasmtimePluginRuntime::new().expect("plugin runtime should initialize")),
            Arc::new(NoopUpdateBackend),
            Arc::new(NoopAuthBackend),
            Arc::new(NoopAuthSessionStore),
        )
        .expect("app state should initialize");
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
