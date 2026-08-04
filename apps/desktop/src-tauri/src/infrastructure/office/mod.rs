use std::{
    collections::HashMap,
    ffi::OsString,
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::Duration,
};

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
    sync::mpsc,
    time::sleep,
};

use crate::{
    application::office_service::{
        OfficeCancellationToken, OfficeRuntime, OfficeRuntimeError, OfficeRuntimeErrorKind,
    },
    domain::office::{
        OfficeDocument, OfficeDocumentOperation, OfficeEngineStatus, OfficeLifecycleOperation,
        OfficeOperationResult,
    },
};

const VERSION_MANIFEST: &str = include_str!("../../../../../../third_party/officecli/version.json");
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const OPEN_TIMEOUT: Duration = Duration::from_secs(8);
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);
const DEFAULT_OUTPUT_LIMIT: usize = 1024 * 1024;
const MAX_BATCH_ARGUMENT_UTF16: usize = 24_000;

#[derive(Debug)]
pub struct OfficeCliRuntime {
    sidecar_path: std::path::PathBuf,
    expected_version: String,
    timeout: Duration,
    output_limit: usize,
    next_child_id: AtomicU64,
    active_children: Mutex<HashMap<u64, OfficeCancellationToken>>,
}

#[derive(Debug, Deserialize)]
struct VersionManifest {
    tag: String,
}

#[derive(Debug)]
struct ProcessOutput {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
enum StreamReadError {
    Io,
    Limit,
}

impl OfficeCliRuntime {
    /// Resolves only the sidecar bundled beside the current ShenDesk executable.
    pub fn bundled() -> Result<Self, OfficeRuntimeError> {
        let executable = std::env::current_exe()
            .map_err(|_| OfficeRuntimeError::new(OfficeRuntimeErrorKind::MissingSidecar))?;
        let executable_dir = executable
            .parent()
            .ok_or_else(|| OfficeRuntimeError::new(OfficeRuntimeErrorKind::MissingSidecar))?;
        let sidecar_name = if cfg!(windows) {
            "officecli.exe"
        } else {
            "officecli"
        };
        let manifest: VersionManifest = serde_json::from_str(VERSION_MANIFEST)
            .map_err(|_| OfficeRuntimeError::new(OfficeRuntimeErrorKind::VersionMismatch))?;

        Ok(Self::new(
            executable_dir.join(sidecar_name),
            manifest.tag.trim_start_matches('v').to_owned(),
            DEFAULT_TIMEOUT,
            DEFAULT_OUTPUT_LIMIT,
        ))
    }

    fn new(
        sidecar_path: std::path::PathBuf,
        expected_version: String,
        timeout: Duration,
        output_limit: usize,
    ) -> Self {
        Self {
            sidecar_path,
            expected_version,
            timeout,
            output_limit,
            next_child_id: AtomicU64::new(1),
            active_children: Mutex::new(HashMap::new()),
        }
    }

    async fn run_json(
        &self,
        operation: &'static str,
        arguments: Vec<OsString>,
        cancellation: &OfficeCancellationToken,
        timeout: Duration,
        runtime_cancellable: bool,
    ) -> Result<Value, OfficeRuntimeError> {
        let output = self
            .run_process(
                operation,
                arguments,
                cancellation,
                timeout,
                runtime_cancellable,
            )
            .await?;
        serde_json::from_slice(&output.stdout).map_err(|_| {
            tracing::error!(
                operation,
                stdout_bytes = output.stdout.len(),
                stderr_bytes = output.stderr.len(),
                "OfficeCLI returned invalid JSON"
            );
            OfficeRuntimeError::new(OfficeRuntimeErrorKind::InvalidJson)
        })
    }

    async fn run_process(
        &self,
        operation: &'static str,
        arguments: Vec<OsString>,
        cancellation: &OfficeCancellationToken,
        timeout: Duration,
        runtime_cancellable: bool,
    ) -> Result<ProcessOutput, OfficeRuntimeError> {
        if cancellation.is_cancelled() {
            return Err(OfficeRuntimeError::new(OfficeRuntimeErrorKind::Cancelled));
        }
        if !self.sidecar_path.is_file() {
            return Err(OfficeRuntimeError::new(
                OfficeRuntimeErrorKind::MissingSidecar,
            ));
        }

        let mut command = Command::new(&self.sidecar_path);
        command
            .args(arguments)
            .env("OFFICECLI_SKIP_UPDATE", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(|_| {
            tracing::error!(operation, "failed to start bundled OfficeCLI");
            OfficeRuntimeError::new(OfficeRuntimeErrorKind::Spawn)
        })?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| OfficeRuntimeError::new(OfficeRuntimeErrorKind::Io))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| OfficeRuntimeError::new(OfficeRuntimeErrorKind::Io))?;
        let (limit_sender, mut limit_receiver) = mpsc::channel(2);
        let stdout_task = tokio::spawn(read_limited(
            stdout,
            self.output_limit,
            limit_sender.clone(),
        ));
        let stderr_task = tokio::spawn(read_limited(stderr, self.output_limit, limit_sender));

        let child_id = self.next_child_id.fetch_add(1, Ordering::Relaxed);
        let runtime_cancellation = OfficeCancellationToken::default();
        if runtime_cancellable {
            self.active_children
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .insert(child_id, runtime_cancellation.clone());
        }

        let wait_result = tokio::select! {
            status = child.wait() => status.map_err(|_| OfficeRuntimeErrorKind::Io),
            _ = cancellation.cancelled() => Err(OfficeRuntimeErrorKind::Cancelled),
            _ = runtime_cancellation.cancelled() => Err(OfficeRuntimeErrorKind::Cancelled),
            _ = sleep(timeout) => Err(OfficeRuntimeErrorKind::Timeout),
            Some(()) = limit_receiver.recv() => Err(OfficeRuntimeErrorKind::OutputLimit),
        };

        if runtime_cancellable {
            self.active_children
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .remove(&child_id);
        }

        if wait_result.is_err() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }

        let stdout = join_reader(stdout_task).await?;
        let stderr = join_reader(stderr_task).await?;
        let status = wait_result.map_err(OfficeRuntimeError::new)?;

        if !status.success() {
            let kind = if status.code().is_some() {
                OfficeRuntimeErrorKind::NonZeroExit
            } else {
                OfficeRuntimeErrorKind::Crashed
            };
            tracing::error!(
                operation,
                exit_code = status.code(),
                stdout_bytes = stdout.len(),
                stderr_bytes = stderr.len(),
                "OfficeCLI operation failed"
            );
            return Err(OfficeRuntimeError::new(kind));
        }

        Ok(ProcessOutput { stdout, stderr })
    }
}

#[async_trait]
impl OfficeRuntime for OfficeCliRuntime {
    async fn probe(&self) -> Result<OfficeEngineStatus, OfficeRuntimeError> {
        let output = self
            .run_process(
                "probe",
                vec![OsString::from("--version")],
                &OfficeCancellationToken::default(),
                self.timeout.min(PROBE_TIMEOUT),
                true,
            )
            .await?;
        let version_output = String::from_utf8(output.stdout)
            .map_err(|_| OfficeRuntimeError::new(OfficeRuntimeErrorKind::VersionMismatch))?;
        let reported_version = version_output
            .split_whitespace()
            .find_map(|token| semver::Version::parse(token.trim_start_matches('v')).ok());
        let expected_version = semver::Version::parse(&self.expected_version)
            .map_err(|_| OfficeRuntimeError::new(OfficeRuntimeErrorKind::VersionMismatch))?;
        if reported_version.as_ref() != Some(&expected_version) {
            tracing::error!("bundled OfficeCLI version does not match manifest");
            return Err(OfficeRuntimeError::new(
                OfficeRuntimeErrorKind::VersionMismatch,
            ));
        }
        Ok(OfficeEngineStatus::ready(self.expected_version.clone()))
    }

    async fn open(
        &self,
        document: &OfficeDocument,
        cancellation: &OfficeCancellationToken,
    ) -> Result<OfficeOperationResult, OfficeRuntimeError> {
        if cancellation.is_cancelled() {
            return Err(OfficeRuntimeError::new(OfficeRuntimeErrorKind::Cancelled));
        }

        // Once started, let `open` finish its bounded ownership handshake. Killing
        // only the transient CLI can orphan the detached resident it just forked.
        let response = self
            .run_json(
                "open",
                vec![
                    OsString::from("open"),
                    document.path.as_os_str().to_owned(),
                    OsString::from("--json"),
                ],
                &OfficeCancellationToken::default(),
                self.timeout.min(OPEN_TIMEOUT),
                false,
            )
            .await?;
        validate_success_envelope(&response)?;
        let owns_session = response.to_string().contains("(resident started)");
        if cancellation.is_cancelled() {
            if owns_session {
                if let Err(error) = self.close(document).await {
                    tracing::error!(
                        error_kind = ?error.kind(),
                        "failed to close OfficeCLI resident after open cancellation"
                    );
                }
            }
            return Err(OfficeRuntimeError::new(OfficeRuntimeErrorKind::Cancelled));
        }
        Ok(OfficeOperationResult::opened(owns_session))
    }

    async fn close(
        &self,
        document: &OfficeDocument,
    ) -> Result<OfficeOperationResult, OfficeRuntimeError> {
        let response = self
            .run_json(
                "close",
                vec![
                    OsString::from("close"),
                    document.path.as_os_str().to_owned(),
                    OsString::from("--json"),
                ],
                &OfficeCancellationToken::default(),
                self.timeout,
                true,
            )
            .await?;
        validate_success_envelope(&response)?;
        Ok(OfficeOperationResult::succeeded(
            OfficeLifecycleOperation::Close,
        ))
    }

    async fn create(
        &self,
        document: &OfficeDocument,
        cancellation: &OfficeCancellationToken,
    ) -> Result<OfficeOperationResult, OfficeRuntimeError> {
        if cancellation.is_cancelled() {
            return Err(OfficeRuntimeError::new(OfficeRuntimeErrorKind::Cancelled));
        }
        // Like `open`, create forks a resident. Let the ownership handshake
        // finish so cancellation cannot orphan it between spawn and response.
        let response = match self
            .run_json(
                "create",
                vec![
                    OsString::from("create"),
                    document.path.as_os_str().to_owned(),
                    OsString::from("--json"),
                ],
                &OfficeCancellationToken::default(),
                self.timeout,
                false,
            )
            .await
        {
            Ok(response) => response,
            Err(error) => {
                let _ = self.close(document).await;
                return Err(error);
            }
        };
        if let Err(error) = validate_success_envelope(&response) {
            let _ = self.close(document).await;
            return Err(error);
        }
        if cancellation.is_cancelled() {
            let _ = self.close(document).await;
            return Err(OfficeRuntimeError::new(OfficeRuntimeErrorKind::Cancelled));
        }
        Ok(OfficeOperationResult::opened(true))
    }

    async fn inspect(
        &self,
        document: &OfficeDocument,
        cancellation: &OfficeCancellationToken,
    ) -> Result<Value, OfficeRuntimeError> {
        let root = match document.format {
            crate::domain::office::OfficeDocumentFormat::Word => "/body",
            crate::domain::office::OfficeDocumentFormat::Spreadsheet => "/Sheet1",
            crate::domain::office::OfficeDocumentFormat::Presentation => "/",
        };
        let response = self
            .run_json(
                "inspect",
                vec![
                    OsString::from("get"),
                    document.path.as_os_str().to_owned(),
                    OsString::from(root),
                    OsString::from("--depth"),
                    OsString::from("3"),
                    OsString::from("--json"),
                ],
                cancellation,
                self.timeout,
                true,
            )
            .await?;
        success_data(&response)
    }

    async fn apply_batch(
        &self,
        document: &OfficeDocument,
        operations: &[OfficeDocumentOperation],
        cancellation: &OfficeCancellationToken,
    ) -> Result<(), OfficeRuntimeError> {
        let commands = serde_json::to_string(&batch_commands(operations))
            .map_err(|_| OfficeRuntimeError::new(OfficeRuntimeErrorKind::InvalidJson))?;
        if commands.encode_utf16().count() > MAX_BATCH_ARGUMENT_UTF16 {
            return Err(OfficeRuntimeError::new(OfficeRuntimeErrorKind::OutputLimit));
        }
        let response = self
            .run_json(
                "batch",
                vec![
                    OsString::from("batch"),
                    document.path.as_os_str().to_owned(),
                    OsString::from("--commands"),
                    OsString::from(commands),
                    OsString::from("--json"),
                ],
                cancellation,
                self.timeout,
                true,
            )
            .await?;
        validate_batch_envelope(&response, operations.len())
    }

    async fn render_preview(
        &self,
        document: &OfficeDocument,
        page: u32,
        output: &std::path::Path,
        cancellation: &OfficeCancellationToken,
    ) -> Result<(), OfficeRuntimeError> {
        let response = self
            .run_json(
                "preview",
                vec![
                    OsString::from("view"),
                    document.path.as_os_str().to_owned(),
                    OsString::from("screenshot"),
                    OsString::from("-o"),
                    output.as_os_str().to_owned(),
                    OsString::from("--page"),
                    OsString::from(page.to_string()),
                    OsString::from("--json"),
                ],
                cancellation,
                self.timeout,
                true,
            )
            .await?;
        validate_success_envelope(&response)
    }

    fn cancel_all(&self) -> usize {
        let tokens = self
            .active_children
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for token in &tokens {
            token.cancel();
        }
        tokens.len()
    }
}

fn validate_success_envelope(response: &Value) -> Result<(), OfficeRuntimeError> {
    match response.get("success").and_then(Value::as_bool) {
        Some(true) => Ok(()),
        Some(false) => Err(OfficeRuntimeError::new(OfficeRuntimeErrorKind::NonZeroExit)),
        None => Err(OfficeRuntimeError::new(OfficeRuntimeErrorKind::InvalidJson)),
    }
}

fn success_data(response: &Value) -> Result<Value, OfficeRuntimeError> {
    validate_success_envelope(response)?;
    response
        .get("data")
        .filter(|data| data.is_object() || data.is_array())
        .cloned()
        .ok_or_else(|| OfficeRuntimeError::new(OfficeRuntimeErrorKind::InvalidJson))
}

fn validate_batch_envelope(
    response: &Value,
    expected_count: usize,
) -> Result<(), OfficeRuntimeError> {
    validate_success_envelope(response)?;
    let data = response
        .get("data")
        .and_then(Value::as_object)
        .ok_or_else(|| OfficeRuntimeError::new(OfficeRuntimeErrorKind::InvalidJson))?;
    let results = data
        .get("results")
        .and_then(Value::as_array)
        .ok_or_else(|| OfficeRuntimeError::new(OfficeRuntimeErrorKind::InvalidJson))?;
    let summary = data
        .get("summary")
        .and_then(Value::as_object)
        .ok_or_else(|| OfficeRuntimeError::new(OfficeRuntimeErrorKind::InvalidJson))?;
    let summary_matches = ["total", "executed", "succeeded"]
        .into_iter()
        .all(|key| summary.get(key).and_then(Value::as_u64) == Some(expected_count as u64))
        && ["failed", "skipped"]
            .into_iter()
            .all(|key| summary.get(key).and_then(Value::as_u64) == Some(0));
    let results_match = results.len() == expected_count
        && results.iter().enumerate().all(|(index, result)| {
            result.get("index").and_then(Value::as_u64) == Some(index as u64)
                && result.get("success").and_then(Value::as_bool) == Some(true)
        });
    if summary_matches && results_match {
        Ok(())
    } else {
        Err(OfficeRuntimeError::new(OfficeRuntimeErrorKind::InvalidJson))
    }
}

fn batch_commands(operations: &[OfficeDocumentOperation]) -> Vec<Value> {
    operations
        .iter()
        .map(|operation| match operation {
            OfficeDocumentOperation::AddWordParagraph { text } => json!({
                "command": "add",
                "parent": "/body",
                "type": "paragraph",
                "props": { "text": text },
            }),
            OfficeDocumentOperation::SetSpreadsheetCell { cell, value } => json!({
                "command": "set",
                "path": format!("/Sheet1/{cell}"),
                "props": { "value": value },
            }),
            OfficeDocumentOperation::AddPresentationSlide { title } => json!({
                "command": "add",
                "parent": "/",
                "type": "slide",
                "props": { "title": title },
            }),
            OfficeDocumentOperation::AddPresentationText { slide, text } => json!({
                "command": "add",
                "parent": format!("/slide[{slide}]"),
                "type": "shape",
                "props": {
                    "text": text,
                    "x": "2cm",
                    "y": "4cm",
                    "width": "20cm",
                    "height": "2cm",
                },
            }),
        })
        .collect()
}

async fn read_limited<R>(
    mut reader: R,
    limit: usize,
    limit_sender: mpsc::Sender<()>,
) -> Result<Vec<u8>, StreamReadError>
where
    R: AsyncRead + Unpin,
{
    let mut output = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let read = reader
            .read(&mut chunk)
            .await
            .map_err(|_| StreamReadError::Io)?;
        if read == 0 {
            return Ok(output);
        }
        if output.len().saturating_add(read) > limit {
            let _ = limit_sender.send(()).await;
            return Err(StreamReadError::Limit);
        }
        output.extend_from_slice(&chunk[..read]);
    }
}

async fn join_reader(
    task: tokio::task::JoinHandle<Result<Vec<u8>, StreamReadError>>,
) -> Result<Vec<u8>, OfficeRuntimeError> {
    match task.await {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(StreamReadError::Limit)) => {
            Err(OfficeRuntimeError::new(OfficeRuntimeErrorKind::OutputLimit))
        }
        Ok(Err(StreamReadError::Io)) | Err(_) => {
            Err(OfficeRuntimeError::new(OfficeRuntimeErrorKind::Io))
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::{fs, os::unix::fs::PermissionsExt, path::Path};

    use tempfile::TempDir;

    use crate::domain::office::OfficeDocumentFormat;

    use super::*;

    fn fixture_runtime(
        script: &str,
        timeout: Duration,
        output_limit: usize,
    ) -> (TempDir, OfficeCliRuntime) {
        let root = tempfile::tempdir().expect("temporary directory should exist");
        let sidecar = root.path().join("officecli");
        fs::write(&sidecar, script).expect("fixture sidecar should be written");
        let mut permissions = fs::metadata(&sidecar)
            .expect("fixture metadata should exist")
            .permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&sidecar, permissions).expect("fixture should be executable");
        (
            root,
            OfficeCliRuntime::new(sidecar, "1.0.143".to_owned(), timeout, output_limit),
        )
    }

    fn document(path: &Path) -> OfficeDocument {
        OfficeDocument {
            path: path.to_path_buf(),
            format: OfficeDocumentFormat::Word,
        }
    }

    #[test]
    fn probes_version_and_parses_lifecycle_json() {
        let (root, runtime) = fixture_runtime(
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'officecli 1.0.143'; else echo '{\"success\":true}'; fi\n",
            Duration::from_secs(1),
            1024,
        );
        let path = root.path().join("fixture.docx");
        fs::write(&path, b"fixture").expect("document fixture should exist");

        tauri::async_runtime::block_on(async {
            assert_eq!(
                runtime.probe().await.expect("probe should succeed").version,
                Some("1.0.143".to_owned())
            );
            runtime
                .open(&document(&path), &OfficeCancellationToken::default())
                .await
                .expect("open should parse JSON");
            runtime
                .close(&document(&path))
                .await
                .expect("close should parse JSON");
        });
    }

    #[test]
    fn maps_version_json_exit_timeout_cancel_and_output_failures() {
        let cases = [
            (
                "#!/bin/sh\necho 'officecli 9.9.9'\n",
                OfficeRuntimeErrorKind::VersionMismatch,
                "probe",
                1024,
                Duration::from_secs(1),
            ),
            (
                "#!/bin/sh\necho 'officecli 11.0.143'\n",
                OfficeRuntimeErrorKind::VersionMismatch,
                "probe",
                1024,
                Duration::from_secs(1),
            ),
            (
                "#!/bin/sh\necho 'not-json'\n",
                OfficeRuntimeErrorKind::InvalidJson,
                "open",
                1024,
                Duration::from_secs(1),
            ),
            (
                "#!/bin/sh\necho '{}'\n",
                OfficeRuntimeErrorKind::InvalidJson,
                "open",
                1024,
                Duration::from_secs(1),
            ),
            (
                "#!/bin/sh\necho 'null'\n",
                OfficeRuntimeErrorKind::InvalidJson,
                "open",
                1024,
                Duration::from_secs(1),
            ),
            (
                "#!/bin/sh\necho 'failure' >&2\nexit 7\n",
                OfficeRuntimeErrorKind::NonZeroExit,
                "open",
                1024,
                Duration::from_secs(1),
            ),
            (
                "#!/bin/sh\nkill -9 $$\n",
                OfficeRuntimeErrorKind::Crashed,
                "open",
                1024,
                Duration::from_secs(1),
            ),
            (
                "#!/bin/sh\nsleep 2\n",
                OfficeRuntimeErrorKind::Timeout,
                "open",
                1024,
                Duration::from_millis(50),
            ),
            (
                "#!/bin/sh\nprintf '0123456789abcdef'\n",
                OfficeRuntimeErrorKind::OutputLimit,
                "open",
                8,
                Duration::from_secs(1),
            ),
        ];

        for (script, expected, operation, output_limit, timeout) in cases {
            let (root, runtime) = fixture_runtime(script, timeout, output_limit);
            let path = root.path().join("fixture.docx");
            fs::write(&path, b"fixture").expect("document fixture should exist");
            let error = tauri::async_runtime::block_on(async {
                if operation == "probe" {
                    runtime.probe().await.expect_err("probe should fail")
                } else {
                    runtime
                        .open(&document(&path), &OfficeCancellationToken::default())
                        .await
                        .expect_err("open should fail")
                }
            });
            assert_eq!(error.kind(), expected);
        }

        let (root, runtime) = fixture_runtime("#!/bin/sh\nsleep 2\n", Duration::from_secs(3), 1024);
        let path = root.path().join("fixture.docx");
        fs::write(&path, b"fixture").expect("document fixture should exist");
        let cancellation = OfficeCancellationToken::default();
        cancellation.cancel();
        let error = tauri::async_runtime::block_on(runtime.open(&document(&path), &cancellation))
            .expect_err("cancelled open should fail");
        assert_eq!(error.kind(), OfficeRuntimeErrorKind::Cancelled);
    }

    #[test]
    fn cancellation_after_spawn_closes_the_new_resident() {
        let (root, runtime) = fixture_runtime(
            "#!/bin/sh\nif [ \"$1\" = \"open\" ]; then sleep 0.1; echo '{\"success\":true,\"data\":{\"text\":\"Opened (resident started)\"}}'; else touch \"$2.closed\"; echo '{\"success\":true}'; fi\n",
            Duration::from_secs(1),
            1024,
        );
        let path = root.path().join("fixture.docx");
        let closed_marker = root.path().join("fixture.docx.closed");
        fs::write(&path, b"fixture").expect("document fixture should exist");
        let runtime = std::sync::Arc::new(runtime);
        let cancellation = OfficeCancellationToken::default();

        let error = tauri::async_runtime::block_on(async {
            let task_runtime = std::sync::Arc::clone(&runtime);
            let task_document = document(&path);
            let task_cancellation = cancellation.clone();
            let task =
                tokio::spawn(
                    async move { task_runtime.open(&task_document, &task_cancellation).await },
                );
            tokio::time::sleep(Duration::from_millis(20)).await;
            cancellation.cancel();
            task.await
                .expect("open task should join")
                .expect_err("open should report cancellation")
        });

        assert_eq!(error.kind(), OfficeRuntimeErrorKind::Cancelled);
        assert!(closed_marker.is_file());
    }

    #[test]
    fn malformed_create_response_closes_a_possible_resident() {
        let (root, runtime) = fixture_runtime(
            "#!/bin/sh\nif [ \"$1\" = \"create\" ]; then echo 'not-json'; else touch \"$2.closed\"; echo '{\"success\":true}'; fi\n",
            Duration::from_secs(1),
            1024,
        );
        let path = root.path().join("fixture.docx");
        let closed_marker = root.path().join("fixture.docx.closed");

        let error = tauri::async_runtime::block_on(
            runtime.create(&document(&path), &OfficeCancellationToken::default()),
        )
        .expect_err("malformed create should fail");

        assert_eq!(error.kind(), OfficeRuntimeErrorKind::InvalidJson);
        assert!(closed_marker.is_file());
    }

    #[test]
    fn startup_probe_uses_a_shorter_timeout_than_document_operations() {
        assert!(PROBE_TIMEOUT < DEFAULT_TIMEOUT);
        assert!(PROBE_TIMEOUT < OPEN_TIMEOUT);
    }

    #[test]
    fn missing_sidecar_is_stable_and_safe() {
        let runtime = OfficeCliRuntime::new(
            Path::new("/definitely/missing/officecli").to_path_buf(),
            "1.0.143".to_owned(),
            Duration::from_secs(1),
            1024,
        );
        let error = tauri::async_runtime::block_on(runtime.probe())
            .expect_err("missing sidecar should fail");

        assert_eq!(error.kind(), OfficeRuntimeErrorKind::MissingSidecar);
        assert!(!error.to_string().contains("/definitely"));
    }

    #[test]
    fn translates_only_whitelisted_batch_operations_in_order() {
        let commands = batch_commands(&[
            OfficeDocumentOperation::AddPresentationSlide {
                title: "Fixture".to_owned(),
            },
            OfficeDocumentOperation::AddPresentationText {
                slide: 1,
                text: "Body".to_owned(),
            },
        ]);

        assert_eq!(commands[0]["command"], "add");
        assert_eq!(commands[0]["type"], "slide");
        assert_eq!(commands[1]["parent"], "/slide[1]");
        assert_eq!(commands[1]["props"]["text"], "Body");
        assert!(commands.iter().all(|command| command["command"] != "raw"));
    }

    #[test]
    fn oversized_escaped_batch_is_rejected_before_spawn() {
        let (root, runtime) = fixture_runtime(
            "#!/bin/sh\ntouch \"$0.started\"\necho '{\"success\":true}'\n",
            Duration::from_secs(1),
            1024,
        );
        let path = root.path().join("fixture.docx");
        fs::write(&path, b"fixture").expect("fixture should exist");
        let error = tauri::async_runtime::block_on(runtime.apply_batch(
            &document(&path),
            &[OfficeDocumentOperation::AddWordParagraph {
                text: "\0".repeat(5_000),
            }],
            &OfficeCancellationToken::default(),
        ))
        .expect_err("oversized command line should fail");

        assert_eq!(error.kind(), OfficeRuntimeErrorKind::OutputLimit);
        assert!(!runtime.sidecar_path.with_extension("started").exists());
    }

    #[test]
    fn extracts_only_valid_non_null_success_data() {
        assert_eq!(
            success_data(&serde_json::json!({ "success": true, "data": { "matches": 1 } }))
                .expect("data should be accepted"),
            serde_json::json!({ "matches": 1 })
        );
        assert!(success_data(&serde_json::json!({ "success": true })).is_err());
        assert!(success_data(&serde_json::json!({ "success": true, "data": null })).is_err());
        assert!(success_data(&serde_json::json!({ "success": true, "data": "text" })).is_err());
    }

    #[test]
    fn batch_success_requires_every_ordered_result() {
        let valid = serde_json::json!({
            "success": true,
            "data": {
                "results": [
                    { "index": 0, "success": true },
                    { "index": 1, "success": true }
                ],
                "summary": {
                    "total": 2,
                    "executed": 2,
                    "succeeded": 2,
                    "failed": 0,
                    "skipped": 0
                }
            }
        });
        assert!(validate_batch_envelope(&valid, 2).is_ok());

        let mut malformed = valid;
        malformed["data"]["results"][1]["index"] = serde_json::json!(0);
        assert!(validate_batch_envelope(&malformed, 2).is_err());
    }

    #[test]
    fn officecli_round_trip_and_regression_fixtures_when_enabled() {
        let Ok(sidecar) = std::env::var("SHENDESK_OFFICECLI_E2E") else {
            return;
        };
        let runtime = OfficeCliRuntime::new(
            Path::new(&sidecar).to_path_buf(),
            "1.0.143".to_owned(),
            Duration::from_secs(60),
            16 * 1024 * 1024,
        );
        let cancellation = OfficeCancellationToken::default();
        tauri::async_runtime::block_on(runtime.probe()).expect("pinned OfficeCLI should probe");

        let cases = [
            (
                OfficeDocumentFormat::Word,
                "docx",
                "ShenDesk Word fixture",
                vec![OfficeDocumentOperation::AddWordParagraph {
                    text: "ShenDesk Word fixture".to_owned(),
                }],
                "word-fixture.docx",
            ),
            (
                OfficeDocumentFormat::Spreadsheet,
                "xlsx",
                "ShenDesk Excel fixture",
                vec![OfficeDocumentOperation::SetSpreadsheetCell {
                    cell: "A1".to_owned(),
                    value: "ShenDesk Excel fixture".to_owned(),
                }],
                "spreadsheet-fixture.xlsx",
            ),
            (
                OfficeDocumentFormat::Presentation,
                "pptx",
                "ShenDesk PowerPoint fixture",
                vec![
                    OfficeDocumentOperation::AddPresentationSlide {
                        title: "ShenDesk PowerPoint fixture".to_owned(),
                    },
                    OfficeDocumentOperation::AddPresentationText {
                        slide: 1,
                        text: "ShenDesk PowerPoint fixture".to_owned(),
                    },
                ],
                "presentation-fixture.pptx",
            ),
        ];

        for (format, extension, marker, operations, fixture_name) in cases {
            let root = tempfile::tempdir().expect("temporary directory should exist");
            let created = OfficeDocument {
                path: root.path().join(format!("created.{extension}")),
                format,
            };
            tauri::async_runtime::block_on(runtime.create(&created, &cancellation))
                .expect("document should be created");
            tauri::async_runtime::block_on(runtime.close(&created))
                .expect("created resident should close");
            tauri::async_runtime::block_on(runtime.open(&created, &cancellation))
                .expect("created document should reopen");
            tauri::async_runtime::block_on(runtime.apply_batch(
                &created,
                &operations,
                &cancellation,
            ))
            .expect("batch should succeed");
            tauri::async_runtime::block_on(runtime.close(&created))
                .expect("changes should flush on close");
            tauri::async_runtime::block_on(runtime.open(&created, &cancellation))
                .expect("modified document should reopen");
            let structure =
                tauri::async_runtime::block_on(runtime.inspect(&created, &cancellation))
                    .expect("document should inspect");
            assert!(structure.to_string().contains(marker));
            let preview = root.path().join("preview.png");
            tauri::async_runtime::block_on(runtime.render_preview(
                &created,
                1,
                &preview,
                &cancellation,
            ))
            .expect("preview should render");
            tauri::async_runtime::block_on(runtime.close(&created))
                .expect("inspection resident should close");
            image::load_from_memory(&fs::read(&preview).expect("preview should exist"))
                .expect("preview should be valid PNG");

            let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/office")
                .join(fixture_name);
            let fixture_document = OfficeDocument {
                path: fixture,
                format,
            };
            let fixture_structure =
                tauri::async_runtime::block_on(runtime.inspect(&fixture_document, &cancellation))
                    .expect("regression fixture should inspect");
            assert!(fixture_structure.to_string().contains(marker));
        }

        assert_eq!(runtime.cancel_all(), 0);
    }
}
