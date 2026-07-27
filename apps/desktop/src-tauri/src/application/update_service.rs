use std::{error::Error, fmt, sync::Arc};

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::domain::{
    event::AppEvent,
    update::{UpdateInfo, UpdateProgress},
};

use super::event_bus::EventBus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateServiceErrorKind {
    NotConfigured,
    Busy,
    NoPendingUpdate,
    CheckFailed,
    InstallFailed,
    Internal,
}

#[derive(Debug, Clone)]
pub struct UpdateServiceError {
    kind: UpdateServiceErrorKind,
    message: String,
}

impl UpdateServiceError {
    pub fn new(kind: UpdateServiceErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn not_configured(message: impl Into<String>) -> Self {
        Self::new(UpdateServiceErrorKind::NotConfigured, message)
    }

    pub fn check_failed(message: impl Into<String>) -> Self {
        Self::new(UpdateServiceErrorKind::CheckFailed, message)
    }

    pub fn install_failed(message: impl Into<String>) -> Self {
        Self::new(UpdateServiceErrorKind::InstallFailed, message)
    }

    pub fn kind(&self) -> UpdateServiceErrorKind {
        self.kind
    }
}

impl fmt::Display for UpdateServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for UpdateServiceError {}

pub type UpdateProgressHandler = Arc<dyn Fn(UpdateProgress) + Send + Sync + 'static>;

#[async_trait]
pub trait UpdateBackend: Send + Sync {
    async fn check(&self) -> Result<Option<UpdateInfo>, UpdateServiceError>;

    async fn install(&self, on_progress: UpdateProgressHandler) -> Result<(), UpdateServiceError>;
}

/// Serializes update checks and installations while keeping Tauri-specific
/// updater objects behind an Application-owned port.
#[derive(Clone)]
pub struct UpdateService {
    backend: Arc<dyn UpdateBackend>,
    event_bus: EventBus,
    operation_lock: Arc<Mutex<()>>,
}

impl fmt::Debug for UpdateService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UpdateService")
            .finish_non_exhaustive()
    }
}

impl UpdateService {
    pub fn new(backend: Arc<dyn UpdateBackend>, event_bus: EventBus) -> Self {
        Self {
            backend,
            event_bus,
            operation_lock: Arc::new(Mutex::new(())),
        }
    }

    pub async fn check(&self) -> Result<Option<UpdateInfo>, UpdateServiceError> {
        let _operation = self.operation_lock.try_lock().map_err(|_| busy_error())?;
        let update = self.backend.check().await?;

        if let Some(update) = &update {
            self.event_bus.publish(AppEvent::UpdateAvailable {
                version: update.version.clone(),
            });
        }

        Ok(update)
    }

    pub async fn install(
        &self,
        on_progress: UpdateProgressHandler,
    ) -> Result<(), UpdateServiceError> {
        let _operation = self.operation_lock.try_lock().map_err(|_| busy_error())?;
        self.backend.install(on_progress).await
    }
}

fn busy_error() -> UpdateServiceError {
    UpdateServiceError::new(
        UpdateServiceErrorKind::Busy,
        "another update operation is already running",
    )
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex as StdMutex,
    };

    use super::*;
    use crate::domain::event::EventKind;

    #[derive(Default)]
    struct RecordingBackend {
        update: StdMutex<Option<UpdateInfo>>,
        installs: AtomicUsize,
    }

    #[async_trait]
    impl UpdateBackend for RecordingBackend {
        async fn check(&self) -> Result<Option<UpdateInfo>, UpdateServiceError> {
            Ok(self.update.lock().expect("update lock").clone())
        }

        async fn install(
            &self,
            on_progress: UpdateProgressHandler,
        ) -> Result<(), UpdateServiceError> {
            self.installs.fetch_add(1, Ordering::Relaxed);
            on_progress(UpdateProgress::Started {
                content_length: Some(10),
            });
            on_progress(UpdateProgress::Progress {
                chunk_length: 10,
                downloaded: 10,
                content_length: Some(10),
            });
            on_progress(UpdateProgress::Finished { downloaded: 10 });
            Ok(())
        }
    }

    fn available_update() -> UpdateInfo {
        UpdateInfo {
            current_version: "0.1.0".to_owned(),
            version: "0.2.0".to_owned(),
            notes: None,
            published_at_unix_seconds: Some(1),
            target: "windows-x86_64".to_owned(),
        }
    }

    #[test]
    fn publishes_update_available_after_a_successful_check() {
        tauri::async_runtime::block_on(async {
            let backend = Arc::new(RecordingBackend {
                update: StdMutex::new(Some(available_update())),
                installs: AtomicUsize::new(0),
            });
            let bus = EventBus::new(8);
            let mut subscriber = bus.subscribe_to([EventKind::UpdateAvailable]);
            let service = UpdateService::new(backend, bus);

            let update = service
                .check()
                .await
                .expect("check should work")
                .expect("update should exist");
            let event = subscriber.recv().await.expect("update event should arrive");

            assert_eq!(update.version, "0.2.0");
            assert_eq!(event.event.kind(), EventKind::UpdateAvailable);
        });
    }

    #[test]
    fn forwards_ordered_install_progress() {
        tauri::async_runtime::block_on(async {
            let backend = Arc::new(RecordingBackend::default());
            let service = UpdateService::new(backend.clone(), EventBus::default());
            let events = Arc::new(StdMutex::new(Vec::new()));
            let received = events.clone();

            service
                .install(Arc::new(move |event| {
                    received.lock().expect("events lock").push(event);
                }))
                .await
                .expect("install should work");

            assert_eq!(backend.installs.load(Ordering::Relaxed), 1);
            let events = events.lock().expect("events lock");
            assert!(matches!(&events[0], UpdateProgress::Started { .. }));
            assert!(matches!(&events[1], UpdateProgress::Progress { .. }));
            assert!(matches!(&events[2], UpdateProgress::Finished { .. }));
        });
    }
}
