use std::{
    collections::BTreeSet,
    error::Error,
    fmt,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};

use semver::Version;

use crate::domain::{
    event::AppEvent,
    plugin::{
        PluginExecution, PluginId, PluginManifest, PluginSnapshot, PluginStatus,
        PLUGIN_API_VERSION, PLUGIN_API_VERSION_EXPORT, PLUGIN_DISABLE_EXPORT, PLUGIN_ENABLE_EXPORT,
        PLUGIN_MANIFEST_FILE_NAME,
    },
};

use super::event_bus::EventBus;

pub const MAX_PLUGIN_MANIFEST_BYTES: u64 = 64 * 1024;
pub const MAX_PLUGIN_MODULE_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_PLUGIN_COMMANDS: usize = 64;
const MAX_PATH_CHARS: usize = 4_096;
const MAX_PLUGIN_ID_CHARS: usize = 128;
const MAX_PLUGIN_NAME_CHARS: usize = 128;
const MAX_PLUGIN_VERSION_CHARS: usize = 64;
const MAX_PLUGIN_DESCRIPTION_CHARS: usize = 1_024;
const MAX_COMMAND_NAME_CHARS: usize = 64;
const MAX_EXPORT_NAME_CHARS: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginServiceErrorKind {
    InvalidInput,
    InvalidManifest,
    NotFound,
    AlreadyInstalled,
    Conflict,
    PackageTooLarge,
    RuntimeRejected,
    ExecutionFailed,
    Io,
}

#[derive(Debug, Clone)]
pub struct PluginServiceError {
    kind: PluginServiceErrorKind,
    message: String,
}

impl PluginServiceError {
    pub fn new(kind: PluginServiceErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(PluginServiceErrorKind::InvalidInput, message)
    }

    pub fn invalid_manifest(message: impl Into<String>) -> Self {
        Self::new(PluginServiceErrorKind::InvalidManifest, message)
    }

    pub fn runtime_rejected(message: impl Into<String>) -> Self {
        Self::new(PluginServiceErrorKind::RuntimeRejected, message)
    }

    pub fn execution_failed(message: impl Into<String>) -> Self {
        Self::new(PluginServiceErrorKind::ExecutionFailed, message)
    }

    pub fn kind(&self) -> PluginServiceErrorKind {
        self.kind
    }
}

impl fmt::Display for PluginServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for PluginServiceError {}

#[derive(Debug, Clone)]
pub struct PluginPackage {
    pub manifest: PluginManifest,
    pub module: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginCallResult {
    pub return_code: i32,
    pub fuel_consumed: u64,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PluginStartupReport {
    pub restored: usize,
    pub disabled_after_failure: usize,
}

/// Application-owned persistence port for installed plugin packages and state.
pub trait PluginRepository: Send + Sync {
    fn read_package(&self, manifest_path: &Path) -> Result<PluginPackage, PluginServiceError>;
    fn install(&self, package: PluginPackage) -> Result<PluginSnapshot, PluginServiceError>;
    fn list(&self) -> Result<Vec<PluginSnapshot>, PluginServiceError>;
    fn get(&self, plugin_id: &PluginId) -> Result<Option<PluginSnapshot>, PluginServiceError>;
    fn module_bytes(&self, plugin_id: &PluginId) -> Result<Vec<u8>, PluginServiceError>;
    fn set_status(
        &self,
        plugin_id: &PluginId,
        status: PluginStatus,
    ) -> Result<PluginSnapshot, PluginServiceError>;
    fn remove(&self, plugin_id: &PluginId) -> Result<bool, PluginServiceError>;
}

/// Application-owned execution port. Infrastructure decides how WASM is compiled and sandboxed.
pub trait PluginRuntime: Send + Sync {
    fn validate(&self, module: &[u8], manifest: &PluginManifest) -> Result<(), PluginServiceError>;

    fn call(&self, module: &[u8], export: &str) -> Result<PluginCallResult, PluginServiceError>;

    fn call_optional(
        &self,
        module: &[u8],
        export: &str,
    ) -> Result<Option<PluginCallResult>, PluginServiceError>;
}

#[derive(Clone)]
pub struct PluginService {
    repository: Arc<dyn PluginRepository>,
    runtime: Arc<dyn PluginRuntime>,
    event_bus: EventBus,
    lifecycle_lock: Arc<Mutex<()>>,
}

impl fmt::Debug for PluginService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PluginService")
            .finish_non_exhaustive()
    }
}

impl PluginService {
    pub fn new(
        repository: Arc<dyn PluginRepository>,
        runtime: Arc<dyn PluginRuntime>,
        event_bus: EventBus,
    ) -> Self {
        Self {
            repository,
            runtime,
            event_bus,
            lifecycle_lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn install(
        &self,
        manifest_path: impl AsRef<str>,
    ) -> Result<PluginSnapshot, PluginServiceError> {
        let _guard = self.lock_lifecycle()?;
        let manifest_path = validate_manifest_path(manifest_path.as_ref())?;
        let package = self.repository.read_package(&manifest_path)?;
        validate_manifest(&package.manifest)?;
        validate_module_size(&package.module)?;
        self.runtime.validate(&package.module, &package.manifest)?;

        let snapshot = self.repository.install(package)?;
        self.event_bus.publish(AppEvent::PluginInstalled {
            plugin: snapshot.clone(),
        });
        Ok(snapshot)
    }

    pub fn list(&self) -> Result<Vec<PluginSnapshot>, PluginServiceError> {
        self.repository.list()
    }

    pub fn get(&self, plugin_id: impl AsRef<str>) -> Result<PluginSnapshot, PluginServiceError> {
        let plugin_id = validate_plugin_id(plugin_id.as_ref())?;
        self.get_required(&plugin_id)
    }

    pub fn enable(&self, plugin_id: impl AsRef<str>) -> Result<PluginSnapshot, PluginServiceError> {
        let _guard = self.lock_lifecycle()?;
        let plugin_id = validate_plugin_id(plugin_id.as_ref())?;
        let snapshot = self.get_required(&plugin_id)?;
        if snapshot.status == PluginStatus::Enabled {
            return Ok(snapshot);
        }

        self.run_enable_hook(&snapshot)?;
        let snapshot = self
            .repository
            .set_status(&plugin_id, PluginStatus::Enabled)?;
        self.event_bus.publish(AppEvent::PluginEnabled {
            plugin: snapshot.clone(),
        });
        Ok(snapshot)
    }

    pub fn disable(
        &self,
        plugin_id: impl AsRef<str>,
    ) -> Result<PluginSnapshot, PluginServiceError> {
        let _guard = self.lock_lifecycle()?;
        let plugin_id = validate_plugin_id(plugin_id.as_ref())?;
        self.disable_locked(&plugin_id)
    }

    pub fn execute(
        &self,
        plugin_id: impl AsRef<str>,
        command_name: impl AsRef<str>,
    ) -> Result<PluginExecution, PluginServiceError> {
        let _guard = self.lock_lifecycle()?;
        let plugin_id = validate_plugin_id(plugin_id.as_ref())?;
        let command_name = validate_command_name(command_name.as_ref())?;
        let snapshot = self.get_required(&plugin_id)?;
        if snapshot.status != PluginStatus::Enabled {
            return Err(PluginServiceError::new(
                PluginServiceErrorKind::Conflict,
                "plugin must be enabled before command execution",
            ));
        }

        let command = snapshot
            .manifest
            .commands
            .iter()
            .find(|command| command.name == command_name)
            .ok_or_else(|| {
                PluginServiceError::invalid_input("plugin command is not declared in the manifest")
            })?;
        let module = self.repository.module_bytes(&plugin_id)?;
        let result = self.runtime.call(&module, &command.export)?;
        let execution = PluginExecution {
            plugin_id,
            command: command_name,
            return_code: result.return_code,
            fuel_consumed: result.fuel_consumed,
        };
        self.event_bus.publish(AppEvent::PluginExecuted {
            execution: execution.clone(),
        });
        Ok(execution)
    }

    pub fn uninstall(&self, plugin_id: impl AsRef<str>) -> Result<PluginId, PluginServiceError> {
        let _guard = self.lock_lifecycle()?;
        let plugin_id = validate_plugin_id(plugin_id.as_ref())?;
        let snapshot = self.get_required(&plugin_id)?;
        if snapshot.status == PluginStatus::Enabled {
            self.disable_locked(&plugin_id)?;
        }

        if !self.repository.remove(&plugin_id)? {
            return Err(not_found_error());
        }
        self.event_bus.publish(AppEvent::PluginRemoved {
            plugin_id: plugin_id.clone(),
        });
        Ok(plugin_id)
    }

    /// Restores plugins that were persistently enabled before the previous exit.
    /// A plugin that fails validation or its enable hook is quarantined as disabled.
    pub fn restore_enabled_plugins(&self) -> PluginStartupReport {
        let Ok(_guard) = self.lock_lifecycle() else {
            tracing::error!("plugin lifecycle lock is unavailable during startup");
            return PluginStartupReport::default();
        };
        let Ok(plugins) = self.repository.list() else {
            tracing::error!("installed plugins could not be listed during startup");
            return PluginStartupReport::default();
        };

        let mut report = PluginStartupReport::default();
        for plugin in plugins {
            if plugin.status != PluginStatus::Enabled {
                continue;
            }

            match self.run_enable_hook(&plugin) {
                Ok(()) => report.restored += 1,
                Err(error) => {
                    tracing::error!(
                        plugin_id = %plugin.manifest.id,
                        error = %error,
                        "enabled plugin failed to restore"
                    );
                    match self
                        .repository
                        .set_status(&plugin.manifest.id, PluginStatus::Disabled)
                    {
                        Ok(snapshot) => {
                            report.disabled_after_failure += 1;
                            self.event_bus
                                .publish(AppEvent::PluginDisabled { plugin: snapshot });
                        }
                        Err(persist_error) => tracing::error!(
                            plugin_id = %plugin.manifest.id,
                            error = %persist_error,
                            "failed to quarantine plugin after startup error"
                        ),
                    }
                }
            }
        }
        report
    }

    /// Best-effort runtime shutdown. Disable hooks run without changing the
    /// persisted enabled preference, so successful plugins can restore next boot.
    pub fn shutdown(&self) -> usize {
        let Ok(_guard) = self.lock_lifecycle() else {
            tracing::error!("plugin lifecycle lock is unavailable during shutdown");
            return 0;
        };
        let Ok(plugins) = self.repository.list() else {
            tracing::error!("installed plugins could not be listed during shutdown");
            return 0;
        };

        let mut stopped = 0;
        for plugin in plugins {
            if plugin.status != PluginStatus::Enabled {
                continue;
            }
            let result = self
                .repository
                .module_bytes(&plugin.manifest.id)
                .and_then(|module| {
                    ensure_hook_succeeded(
                        self.runtime.call_optional(&module, PLUGIN_DISABLE_EXPORT)?,
                        PLUGIN_DISABLE_EXPORT,
                    )
                });
            match result {
                Ok(()) => stopped += 1,
                Err(error) => tracing::error!(
                    plugin_id = %plugin.manifest.id,
                    error = %error,
                    "plugin disable hook failed during shutdown"
                ),
            }
        }
        stopped
    }

    fn run_enable_hook(&self, snapshot: &PluginSnapshot) -> Result<(), PluginServiceError> {
        let module = self.repository.module_bytes(&snapshot.manifest.id)?;
        self.runtime.validate(&module, &snapshot.manifest)?;
        ensure_hook_succeeded(
            self.runtime.call_optional(&module, PLUGIN_ENABLE_EXPORT)?,
            PLUGIN_ENABLE_EXPORT,
        )
    }

    fn disable_locked(&self, plugin_id: &PluginId) -> Result<PluginSnapshot, PluginServiceError> {
        let snapshot = self.get_required(plugin_id)?;
        if snapshot.status == PluginStatus::Disabled {
            return Ok(snapshot);
        }

        let module = self.repository.module_bytes(plugin_id)?;
        ensure_hook_succeeded(
            self.runtime.call_optional(&module, PLUGIN_DISABLE_EXPORT)?,
            PLUGIN_DISABLE_EXPORT,
        )?;
        let snapshot = self
            .repository
            .set_status(plugin_id, PluginStatus::Disabled)?;
        self.event_bus.publish(AppEvent::PluginDisabled {
            plugin: snapshot.clone(),
        });
        Ok(snapshot)
    }

    fn get_required(&self, plugin_id: &PluginId) -> Result<PluginSnapshot, PluginServiceError> {
        self.repository.get(plugin_id)?.ok_or_else(not_found_error)
    }

    fn lock_lifecycle(&self) -> Result<MutexGuard<'_, ()>, PluginServiceError> {
        self.lifecycle_lock.lock().map_err(|_| {
            PluginServiceError::new(
                PluginServiceErrorKind::Conflict,
                "plugin lifecycle state is unavailable",
            )
        })
    }
}

fn ensure_hook_succeeded(
    result: Option<PluginCallResult>,
    export: &str,
) -> Result<(), PluginServiceError> {
    if let Some(result) = result {
        if result.return_code != 0 {
            return Err(PluginServiceError::execution_failed(format!(
                "plugin lifecycle export {export} returned {}",
                result.return_code
            )));
        }
    }
    Ok(())
}

fn not_found_error() -> PluginServiceError {
    PluginServiceError::new(PluginServiceErrorKind::NotFound, "plugin was not found")
}

fn validate_manifest_path(value: &str) -> Result<PathBuf, PluginServiceError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > MAX_PATH_CHARS {
        return Err(PluginServiceError::invalid_input(
            "plugin manifest path is invalid",
        ));
    }
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(PluginServiceError::invalid_input(
            "plugin manifest path must be absolute",
        ));
    }
    if path.file_name().and_then(|name| name.to_str()) != Some(PLUGIN_MANIFEST_FILE_NAME) {
        return Err(PluginServiceError::invalid_input(
            "plugin manifest file must be named plugin.json",
        ));
    }
    Ok(path)
}

pub fn validate_manifest(manifest: &PluginManifest) -> Result<(), PluginServiceError> {
    if manifest.api_version != PLUGIN_API_VERSION {
        return Err(PluginServiceError::invalid_manifest(format!(
            "unsupported plugin api version {}",
            manifest.api_version
        )));
    }
    validate_plugin_id(manifest.id.as_str()).map_err(as_manifest_error)?;
    validate_bounded_text(
        &manifest.name,
        MAX_PLUGIN_NAME_CHARS,
        "plugin name is invalid",
    )?;
    validate_version(&manifest.version)?;
    validate_entrypoint(&manifest.entrypoint)?;
    validate_optional_description(
        manifest.description.as_deref(),
        "plugin description is invalid",
    )?;
    if manifest.commands.len() > MAX_PLUGIN_COMMANDS {
        return Err(PluginServiceError::invalid_manifest(
            "plugin declares too many commands",
        ));
    }

    let mut command_names = BTreeSet::new();
    let mut export_names = BTreeSet::new();
    for command in &manifest.commands {
        let command_name = validate_command_name(&command.name).map_err(as_manifest_error)?;
        validate_export_name(&command.export)?;
        if [
            PLUGIN_API_VERSION_EXPORT,
            PLUGIN_ENABLE_EXPORT,
            PLUGIN_DISABLE_EXPORT,
        ]
        .contains(&command.export.as_str())
        {
            return Err(PluginServiceError::invalid_manifest(
                "plugin command cannot use a reserved lifecycle export",
            ));
        }
        if !command_names.insert(command_name) || !export_names.insert(command.export.clone()) {
            return Err(PluginServiceError::invalid_manifest(
                "plugin command names and exports must be unique",
            ));
        }
        validate_optional_description(
            command.description.as_deref(),
            "plugin command description is invalid",
        )?;
    }
    Ok(())
}

fn as_manifest_error(error: PluginServiceError) -> PluginServiceError {
    PluginServiceError::invalid_manifest(error.to_string())
}

fn validate_module_size(module: &[u8]) -> Result<(), PluginServiceError> {
    if module.is_empty() {
        return Err(PluginServiceError::invalid_manifest(
            "plugin module is empty",
        ));
    }
    if module.len() as u64 > MAX_PLUGIN_MODULE_BYTES {
        return Err(PluginServiceError::new(
            PluginServiceErrorKind::PackageTooLarge,
            "plugin module exceeds the configured size limit",
        ));
    }
    Ok(())
}

fn validate_plugin_id(value: &str) -> Result<PluginId, PluginServiceError> {
    let trimmed = value.trim();
    if trimmed != value {
        return Err(PluginServiceError::invalid_input("plugin id is invalid"));
    }
    let value = trimmed;
    let valid_length = (3..=MAX_PLUGIN_ID_CHARS).contains(&value.chars().count());
    let valid_characters = value.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
    });
    let valid_edges = value
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_alphanumeric)
        && value
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric);
    if !valid_length
        || !valid_characters
        || !valid_edges
        || !value.contains('.')
        || value.contains("..")
    {
        return Err(PluginServiceError::invalid_input("plugin id is invalid"));
    }
    Ok(PluginId::new(value))
}

fn validate_bounded_text(
    value: &str,
    max_chars: usize,
    message: &'static str,
) -> Result<(), PluginServiceError> {
    let trimmed = value.trim();
    if trimmed != value
        || value.is_empty()
        || value.chars().count() > max_chars
        || value.chars().any(char::is_control)
    {
        return Err(PluginServiceError::invalid_manifest(message));
    }
    Ok(())
}

fn validate_optional_description(
    value: Option<&str>,
    message: &'static str,
) -> Result<(), PluginServiceError> {
    if let Some(value) = value {
        validate_bounded_text(value, MAX_PLUGIN_DESCRIPTION_CHARS, message)?;
    }
    Ok(())
}

fn validate_version(value: &str) -> Result<(), PluginServiceError> {
    if value.chars().count() > MAX_PLUGIN_VERSION_CHARS || Version::parse(value).is_err() {
        return Err(PluginServiceError::invalid_manifest(
            "plugin version must be a valid semantic version",
        ));
    }
    Ok(())
}

fn validate_entrypoint(value: &str) -> Result<(), PluginServiceError> {
    let path = Path::new(value);
    let mut components = path.components();
    let is_single_file =
        matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none();
    if !is_single_file || path.extension().and_then(|value| value.to_str()) != Some("wasm") {
        return Err(PluginServiceError::invalid_manifest(
            "plugin entrypoint must be one relative .wasm file name",
        ));
    }
    Ok(())
}

fn validate_command_name(value: &str) -> Result<String, PluginServiceError> {
    let trimmed = value.trim();
    if trimmed != value {
        return Err(PluginServiceError::invalid_input(
            "plugin command name is invalid",
        ));
    }
    let value = trimmed;
    let valid_length = (1..=MAX_COMMAND_NAME_CHARS).contains(&value.chars().count());
    let valid = value.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
    });
    if !valid_length || !valid {
        return Err(PluginServiceError::invalid_input(
            "plugin command name is invalid",
        ));
    }
    Ok(value.to_owned())
}

fn validate_export_name(value: &str) -> Result<(), PluginServiceError> {
    let mut bytes = value.bytes();
    let Some(first) = bytes.next() else {
        return Err(PluginServiceError::invalid_manifest(
            "plugin command export is invalid",
        ));
    };
    if value.chars().count() > MAX_EXPORT_NAME_CHARS
        || !(first.is_ascii_alphabetic() || first == b'_')
        || !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(PluginServiceError::invalid_manifest(
            "plugin command export is invalid",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::domain::{event::EventKind, plugin::PluginCommand};

    fn manifest() -> PluginManifest {
        PluginManifest {
            api_version: PLUGIN_API_VERSION,
            id: PluginId::new("com.shencom.hello"),
            name: "Hello".to_owned(),
            version: "1.0.0".to_owned(),
            entrypoint: "hello.wasm".to_owned(),
            description: None,
            commands: vec![PluginCommand {
                name: "hello".to_owned(),
                export: "hello".to_owned(),
                description: None,
            }],
        }
    }

    #[derive(Default)]
    struct MemoryRepository {
        source: Mutex<Option<PluginPackage>>,
        installed: Mutex<BTreeMap<PluginId, (PluginSnapshot, Vec<u8>)>>,
    }

    impl MemoryRepository {
        fn with_source(package: PluginPackage) -> Self {
            Self {
                source: Mutex::new(Some(package)),
                installed: Mutex::new(BTreeMap::new()),
            }
        }
    }

    impl PluginRepository for MemoryRepository {
        fn read_package(&self, _manifest_path: &Path) -> Result<PluginPackage, PluginServiceError> {
            self.source
                .lock()
                .expect("source lock")
                .clone()
                .ok_or_else(not_found_error)
        }

        fn install(&self, package: PluginPackage) -> Result<PluginSnapshot, PluginServiceError> {
            let snapshot = PluginSnapshot {
                manifest: package.manifest,
                status: PluginStatus::Disabled,
                installed_at_unix_ms: 1,
                updated_at_unix_ms: 1,
            };
            let mut installed = self.installed.lock().expect("installed lock");
            if installed.contains_key(&snapshot.manifest.id) {
                return Err(PluginServiceError::new(
                    PluginServiceErrorKind::AlreadyInstalled,
                    "already installed",
                ));
            }
            installed.insert(
                snapshot.manifest.id.clone(),
                (snapshot.clone(), package.module),
            );
            Ok(snapshot)
        }

        fn list(&self) -> Result<Vec<PluginSnapshot>, PluginServiceError> {
            Ok(self
                .installed
                .lock()
                .expect("installed lock")
                .values()
                .map(|(snapshot, _)| snapshot.clone())
                .collect())
        }

        fn get(&self, plugin_id: &PluginId) -> Result<Option<PluginSnapshot>, PluginServiceError> {
            Ok(self
                .installed
                .lock()
                .expect("installed lock")
                .get(plugin_id)
                .map(|(snapshot, _)| snapshot.clone()))
        }

        fn module_bytes(&self, plugin_id: &PluginId) -> Result<Vec<u8>, PluginServiceError> {
            self.installed
                .lock()
                .expect("installed lock")
                .get(plugin_id)
                .map(|(_, module)| module.clone())
                .ok_or_else(not_found_error)
        }

        fn set_status(
            &self,
            plugin_id: &PluginId,
            status: PluginStatus,
        ) -> Result<PluginSnapshot, PluginServiceError> {
            let mut installed = self.installed.lock().expect("installed lock");
            let (snapshot, _) = installed.get_mut(plugin_id).ok_or_else(not_found_error)?;
            snapshot.status = status;
            snapshot.updated_at_unix_ms += 1;
            Ok(snapshot.clone())
        }

        fn remove(&self, plugin_id: &PluginId) -> Result<bool, PluginServiceError> {
            Ok(self
                .installed
                .lock()
                .expect("installed lock")
                .remove(plugin_id)
                .is_some())
        }
    }

    #[derive(Default)]
    struct RecordingRuntime {
        calls: Mutex<Vec<String>>,
    }

    impl RecordingRuntime {
        fn calls(&self) -> Vec<String> {
            self.calls.lock().expect("calls lock").clone()
        }
    }

    impl PluginRuntime for RecordingRuntime {
        fn validate(
            &self,
            _module: &[u8],
            _manifest: &PluginManifest,
        ) -> Result<(), PluginServiceError> {
            self.calls
                .lock()
                .expect("calls lock")
                .push("validate".to_owned());
            Ok(())
        }

        fn call(
            &self,
            _module: &[u8],
            export: &str,
        ) -> Result<PluginCallResult, PluginServiceError> {
            self.calls
                .lock()
                .expect("calls lock")
                .push(export.to_owned());
            Ok(PluginCallResult {
                return_code: 7,
                fuel_consumed: 42,
            })
        }

        fn call_optional(
            &self,
            _module: &[u8],
            export: &str,
        ) -> Result<Option<PluginCallResult>, PluginServiceError> {
            self.calls
                .lock()
                .expect("calls lock")
                .push(export.to_owned());
            Ok(Some(PluginCallResult {
                return_code: 0,
                fuel_consumed: 1,
            }))
        }
    }

    #[test]
    fn validates_the_versioned_manifest_contract() {
        validate_manifest(&manifest()).expect("valid manifest should pass");
    }

    #[test]
    fn rejects_path_traversal_reserved_exports_and_invalid_semver() {
        let mut invalid_entrypoint = manifest();
        invalid_entrypoint.entrypoint = "../hello.wasm".to_owned();
        assert_eq!(
            validate_manifest(&invalid_entrypoint)
                .expect_err("traversal must fail")
                .kind(),
            PluginServiceErrorKind::InvalidManifest
        );

        let mut reserved = manifest();
        reserved.commands[0].export = PLUGIN_ENABLE_EXPORT.to_owned();
        assert_eq!(
            validate_manifest(&reserved)
                .expect_err("reserved export must fail")
                .kind(),
            PluginServiceErrorKind::InvalidManifest
        );

        let mut invalid_version = manifest();
        invalid_version.version = "latest".to_owned();
        assert!(validate_manifest(&invalid_version).is_err());
    }

    #[test]
    fn rejects_invalid_plugin_and_command_identifiers() {
        assert!(validate_plugin_id("../../private").is_err());
        assert!(validate_plugin_id("hello").is_err());
        assert!(validate_plugin_id(" com.shencom.hello").is_err());
        assert!(validate_command_name("Run Shell").is_err());
        assert!(validate_command_name("hello ").is_err());
        assert!(validate_export_name("9invalid").is_err());
    }

    #[test]
    fn rejects_non_canonical_manifest_text_instead_of_silently_trimming_it() {
        let mut invalid_name = manifest();
        invalid_name.name = " Hello".to_owned();
        assert_eq!(
            validate_manifest(&invalid_name)
                .expect_err("leading whitespace must fail")
                .kind(),
            PluginServiceErrorKind::InvalidManifest
        );

        let mut invalid_command = manifest();
        invalid_command.commands[0].name = "hello ".to_owned();
        assert_eq!(
            validate_manifest(&invalid_command)
                .expect_err("trailing whitespace must fail")
                .kind(),
            PluginServiceErrorKind::InvalidManifest
        );
    }

    #[test]
    fn drives_install_enable_execute_disable_and_remove_events() {
        let repository = Arc::new(MemoryRepository::with_source(PluginPackage {
            manifest: manifest(),
            module: vec![1, 2, 3],
        }));
        let runtime = Arc::new(RecordingRuntime::default());
        let bus = EventBus::new(16);
        let mut subscriber = bus.subscribe_to([
            EventKind::PluginInstalled,
            EventKind::PluginEnabled,
            EventKind::PluginExecuted,
            EventKind::PluginDisabled,
            EventKind::PluginRemoved,
        ]);
        let service = PluginService::new(repository, runtime.clone(), bus);
        let manifest_path = std::env::temp_dir()
            .join(PLUGIN_MANIFEST_FILE_NAME)
            .to_string_lossy()
            .into_owned();

        service.install(manifest_path).expect("install should work");
        service
            .enable("com.shencom.hello")
            .expect("enable should work");
        let execution = service
            .execute("com.shencom.hello", "hello")
            .expect("command should run");
        service
            .disable("com.shencom.hello")
            .expect("disable should work");
        service
            .uninstall("com.shencom.hello")
            .expect("uninstall should work");

        assert_eq!(execution.return_code, 7);
        assert_eq!(execution.fuel_consumed, 42);
        let kinds = (0..5)
            .map(|_| {
                subscriber
                    .try_recv()
                    .expect("event receive should work")
                    .expect("event should exist")
                    .event
                    .kind()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            vec![
                EventKind::PluginInstalled,
                EventKind::PluginEnabled,
                EventKind::PluginExecuted,
                EventKind::PluginDisabled,
                EventKind::PluginRemoved,
            ]
        );
        assert_eq!(
            runtime.calls(),
            vec![
                "validate",
                "validate",
                PLUGIN_ENABLE_EXPORT,
                "hello",
                PLUGIN_DISABLE_EXPORT,
            ]
        );
    }

    #[test]
    fn shutdown_runs_hooks_without_losing_the_enabled_preference() {
        let repository = Arc::new(MemoryRepository::with_source(PluginPackage {
            manifest: manifest(),
            module: vec![1],
        }));
        let runtime = Arc::new(RecordingRuntime::default());
        let service = PluginService::new(repository.clone(), runtime.clone(), EventBus::default());
        let manifest_path = std::env::temp_dir()
            .join(PLUGIN_MANIFEST_FILE_NAME)
            .to_string_lossy()
            .into_owned();

        service.install(manifest_path).expect("install should work");
        repository
            .set_status(&PluginId::new("com.shencom.hello"), PluginStatus::Enabled)
            .expect("status should change");

        let report = service.restore_enabled_plugins();
        assert_eq!(report.restored, 1);
        assert_eq!(service.shutdown(), 1);
        assert_eq!(
            service
                .get("com.shencom.hello")
                .expect("plugin should exist")
                .status,
            PluginStatus::Enabled
        );
        assert!(runtime.calls().contains(&PLUGIN_ENABLE_EXPORT.to_owned()));
        assert!(runtime.calls().contains(&PLUGIN_DISABLE_EXPORT.to_owned()));
    }
}
