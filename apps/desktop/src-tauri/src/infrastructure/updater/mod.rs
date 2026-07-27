use std::{
    fmt,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use async_trait::async_trait;
use tauri::AppHandle;
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio::sync::Mutex;

use crate::{
    application::update_service::{
        UpdateBackend, UpdateProgressHandler, UpdateServiceError, UpdateServiceErrorKind,
    },
    domain::update::{UpdateInfo, UpdateProgress},
};

const UPDATE_ENDPOINT: &str =
    "https://github.com/zihe-xu/ShencomDesk/releases/latest/download/latest.json";
const UPDATE_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const EMBEDDED_PUBLIC_KEY: Option<&str> = option_env!("SHENDESK_UPDATER_PUBLIC_KEY");

/// Tauri Updater adapter. The pending `Update` retains its verified manifest
/// metadata without exposing URLs or signatures outside Infrastructure.
pub struct TauriUpdateBackend {
    app: AppHandle,
    pending: Mutex<Option<Update>>,
}

impl TauriUpdateBackend {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            pending: Mutex::new(None),
        }
    }

    fn public_key(&self) -> Result<&'static str, UpdateServiceError> {
        EMBEDDED_PUBLIC_KEY
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                UpdateServiceError::not_configured(
                    "the updater public key is not embedded in this build",
                )
            })
    }

    fn build_updater(&self) -> Result<tauri_plugin_updater::Updater, UpdateServiceError> {
        let endpoint = url::Url::parse(UPDATE_ENDPOINT).map_err(|error| {
            UpdateServiceError::new(
                UpdateServiceErrorKind::Internal,
                format!("the fixed update endpoint is invalid: {error}"),
            )
        })?;

        self.app
            .updater_builder()
            .pubkey(self.public_key()?)
            .endpoints(vec![endpoint])
            .map_err(|error| {
                UpdateServiceError::new(
                    UpdateServiceErrorKind::Internal,
                    format!("failed to configure update endpoint: {error}"),
                )
            })?
            .timeout(UPDATE_TIMEOUT)
            .build()
            .map_err(|error| {
                UpdateServiceError::new(
                    UpdateServiceErrorKind::Internal,
                    format!("failed to build updater client: {error}"),
                )
            })
    }

    async fn set_pending(&self, update: Option<Update>) {
        *self.pending.lock().await = update;
    }
}

impl fmt::Debug for TauriUpdateBackend {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TauriUpdateBackend")
            .field(
                "configured",
                &EMBEDDED_PUBLIC_KEY.is_some_and(|value| !value.trim().is_empty()),
            )
            .field("endpoint", &UPDATE_ENDPOINT)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl UpdateBackend for TauriUpdateBackend {
    async fn check(&self) -> Result<Option<UpdateInfo>, UpdateServiceError> {
        let update = self
            .build_updater()?
            .check()
            .await
            .map_err(|error| {
                UpdateServiceError::check_failed(format!(
                    "update check request failed: {error}"
                ))
            })?;

        let info = update.as_ref().map(|update| UpdateInfo {
            current_version: update.current_version.clone(),
            version: update.version.clone(),
            notes: update.body.clone(),
            published_at_unix_seconds: update.date.map(|date| date.unix_timestamp()),
            target: update.target.clone(),
        });
        self.set_pending(update).await;
        Ok(info)
    }

    async fn install(
        &self,
        on_progress: UpdateProgressHandler,
    ) -> Result<(), UpdateServiceError> {
        let update = self.pending.lock().await.clone().ok_or_else(|| {
            UpdateServiceError::new(
                UpdateServiceErrorKind::NoPendingUpdate,
                "no checked update is ready to install",
            )
        })?;

        let downloaded = Arc::new(AtomicU64::new(0));
        let started = Arc::new(AtomicBool::new(false));

        let chunk_downloaded = downloaded.clone();
        let chunk_started = started.clone();
        let chunk_progress = on_progress.clone();

        let finish_downloaded = downloaded.clone();
        let finish_started = started.clone();
        let finish_progress = on_progress.clone();

        update
            .download_and_install(
                move |chunk_length, content_length| {
                    if !chunk_started.swap(true, Ordering::Relaxed) {
                        chunk_progress(UpdateProgress::Started { content_length });
                    }
                    let chunk_length = chunk_length as u64;
                    let downloaded =
                        chunk_downloaded.fetch_add(chunk_length, Ordering::Relaxed) + chunk_length;
                    chunk_progress(UpdateProgress::Progress {
                        chunk_length,
                        downloaded,
                        content_length,
                    });
                },
                move || {
                    if !finish_started.swap(true, Ordering::Relaxed) {
                        finish_progress(UpdateProgress::Started {
                            content_length: None,
                        });
                    }
                    finish_progress(UpdateProgress::Finished {
                        downloaded: finish_downloaded.load(Ordering::Relaxed),
                    });
                },
            )
            .await
            .map_err(|error| {
                UpdateServiceError::install_failed(format!(
                    "signed update download or installation failed: {error}"
                ))
            })?;

        self.set_pending(None).await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_update_endpoint_is_fixed_and_uses_https() {
        assert_eq!(
            UPDATE_ENDPOINT,
            "https://github.com/zihe-xu/ShencomDesk/releases/latest/download/latest.json"
        );
        assert!(UPDATE_ENDPOINT.starts_with("https://"));
        assert!(!UPDATE_ENDPOINT.contains("{{"));
    }

    #[test]
    fn ordinary_builds_may_omit_the_release_public_key() {
        if let Some(public_key) = EMBEDDED_PUBLIC_KEY {
            assert!(!public_key.trim().is_empty());
        }
    }
}
