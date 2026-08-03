use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use tempfile::Builder as TempFileBuilder;
use wasmtime::{Config, Engine, Instance, Module, Store, StoreLimits, StoreLimitsBuilder};

use crate::{
    application::plugin_service::{
        validate_manifest, PluginCallResult, PluginPackage, PluginRepository, PluginRuntime,
        PluginServiceError, PluginServiceErrorKind, MAX_PLUGIN_MANIFEST_BYTES,
        MAX_PLUGIN_MODULE_BYTES,
    },
    domain::plugin::{
        PluginId, PluginManifest, PluginSnapshot, PluginStatus, PLUGIN_API_VERSION,
        PLUGIN_API_VERSION_EXPORT, PLUGIN_DISABLE_EXPORT, PLUGIN_ENABLE_EXPORT,
        PLUGIN_MANIFEST_FILE_NAME,
    },
};

const STATE_FILE_NAME: &str = "state.json";
const STATE_TEMP_FILE_PREFIX: &str = ".state-";
const STATE_TEMP_FILE_SUFFIX: &str = ".tmp";
const MAX_PLUGIN_MEMORY_BYTES: usize = 64 * 1024 * 1024;
const MAX_PLUGIN_TABLE_ELEMENTS: usize = 10_000;
const MAX_PLUGIN_FUEL_PER_CALL: u64 = 10_000_000;
const MAX_WASM_STACK_BYTES: usize = 512 * 1024;

struct StoreData {
    limits: StoreLimits,
}

#[derive(Clone)]
pub struct WasmtimePluginRuntime {
    engine: Engine,
}

impl std::fmt::Debug for WasmtimePluginRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WasmtimePluginRuntime")
            .field("max_memory_bytes", &MAX_PLUGIN_MEMORY_BYTES)
            .field("max_fuel_per_call", &MAX_PLUGIN_FUEL_PER_CALL)
            .finish()
    }
}

impl WasmtimePluginRuntime {
    pub fn new() -> Result<Self, PluginServiceError> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.max_wasm_stack(MAX_WASM_STACK_BYTES);
        config.generate_address_map(false);
        config.wasm_backtrace_max_frames(None);
        let engine = Engine::new(&config).map_err(|error| {
            PluginServiceError::runtime_rejected(format!(
                "failed to initialize Wasmtime engine: {error}"
            ))
        })?;
        Ok(Self { engine })
    }

    fn compile(&self, module: &[u8]) -> Result<Module, PluginServiceError> {
        if !module.starts_with(b"\0asm") {
            return Err(PluginServiceError::runtime_rejected(
                "plugin module must use the binary WebAssembly format",
            ));
        }
        let module = Module::new(&self.engine, module).map_err(|error| {
            PluginServiceError::runtime_rejected(format!(
                "plugin module compilation failed: {error}"
            ))
        })?;
        if module.imports().next().is_some() {
            return Err(PluginServiceError::runtime_rejected(
                "plugin imports are not allowed by API version 1",
            ));
        }
        Ok(module)
    }

    fn instantiate(
        &self,
        module: &Module,
    ) -> Result<(Store<StoreData>, Instance), PluginServiceError> {
        let limits = StoreLimitsBuilder::new()
            .memory_size(MAX_PLUGIN_MEMORY_BYTES)
            .table_elements(MAX_PLUGIN_TABLE_ELEMENTS)
            .instances(1)
            .memories(1)
            .tables(1)
            .trap_on_grow_failure(true)
            .build();
        let mut store = Store::new(&self.engine, StoreData { limits });
        store.limiter(|state| &mut state.limits);
        store.set_fuel(MAX_PLUGIN_FUEL_PER_CALL).map_err(|error| {
            PluginServiceError::runtime_rejected(format!(
                "plugin fuel configuration failed: {error}"
            ))
        })?;
        let instance = Instance::new(&mut store, module, &[]).map_err(|error| {
            PluginServiceError::runtime_rejected(format!(
                "plugin module instantiation failed: {error}"
            ))
        })?;
        Ok((store, instance))
    }

    fn typed_export(
        instance: &Instance,
        store: &mut Store<StoreData>,
        export: &str,
    ) -> Result<wasmtime::TypedFunc<(), i32>, PluginServiceError> {
        instance
            .get_typed_func::<(), i32>(&mut *store, export)
            .map_err(|error| {
                PluginServiceError::runtime_rejected(format!(
                    "plugin export {export} must have signature () -> i32: {error}"
                ))
            })
    }

    fn call_instance_export(
        instance: &Instance,
        store: &mut Store<StoreData>,
        export: &str,
    ) -> Result<PluginCallResult, PluginServiceError> {
        let function = Self::typed_export(instance, store, export)
            .map_err(|error| PluginServiceError::execution_failed(error.to_string()))?;
        let before = store.get_fuel().map_err(|error| {
            PluginServiceError::execution_failed(format!(
                "plugin fuel state is unavailable: {error}"
            ))
        })?;
        let return_code = function.call(&mut *store, ()).map_err(|error| {
            PluginServiceError::execution_failed(format!("plugin export {export} trapped: {error}"))
        })?;
        let remaining = store.get_fuel().map_err(|error| {
            PluginServiceError::execution_failed(format!(
                "plugin fuel state is unavailable: {error}"
            ))
        })?;
        Ok(PluginCallResult {
            return_code,
            fuel_consumed: before.saturating_sub(remaining),
        })
    }
}

impl PluginRuntime for WasmtimePluginRuntime {
    fn validate(&self, module: &[u8], manifest: &PluginManifest) -> Result<(), PluginServiceError> {
        let module = self.compile(module)?;
        let (mut store, instance) = self.instantiate(&module)?;

        let api_version = Self::typed_export(&instance, &mut store, PLUGIN_API_VERSION_EXPORT)?
            .call(&mut store, ())
            .map_err(|error| {
                PluginServiceError::runtime_rejected(format!(
                    "plugin API version export trapped: {error}"
                ))
            })?;
        if api_version != PLUGIN_API_VERSION as i32 {
            return Err(PluginServiceError::runtime_rejected(format!(
                "plugin module reports API version {api_version}"
            )));
        }

        for lifecycle_export in [PLUGIN_ENABLE_EXPORT, PLUGIN_DISABLE_EXPORT] {
            if instance.get_func(&mut store, lifecycle_export).is_some() {
                Self::typed_export(&instance, &mut store, lifecycle_export)?;
            }
        }
        for command in &manifest.commands {
            Self::typed_export(&instance, &mut store, &command.export)?;
        }
        Ok(())
    }

    fn call(&self, module: &[u8], export: &str) -> Result<PluginCallResult, PluginServiceError> {
        let module = self
            .compile(module)
            .map_err(|error| PluginServiceError::execution_failed(error.to_string()))?;
        let (mut store, instance) = self
            .instantiate(&module)
            .map_err(|error| PluginServiceError::execution_failed(error.to_string()))?;
        Self::call_instance_export(&instance, &mut store, export)
    }

    fn call_optional(
        &self,
        module: &[u8],
        export: &str,
    ) -> Result<Option<PluginCallResult>, PluginServiceError> {
        let module = self
            .compile(module)
            .map_err(|error| PluginServiceError::execution_failed(error.to_string()))?;
        let (mut store, instance) = self
            .instantiate(&module)
            .map_err(|error| PluginServiceError::execution_failed(error.to_string()))?;
        if instance.get_func(&mut store, export).is_none() {
            return Ok(None);
        }
        Self::call_instance_export(&instance, &mut store, export).map(Some)
    }
}

#[derive(Debug)]
pub struct LocalPluginRepository {
    root: PathBuf,
    records: Mutex<BTreeMap<PluginId, PluginSnapshot>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PersistedPluginState {
    status: PluginStatus,
    installed_at_unix_ms: u64,
    updated_at_unix_ms: u64,
}

impl LocalPluginRepository {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, PluginServiceError> {
        let root = root.into();
        fs::create_dir_all(&root).map_err(|error| io_error("create plugin root", error))?;
        let records = load_installed_plugins(&root);
        Ok(Self {
            root,
            records: Mutex::new(records),
        })
    }

    fn plugin_dir(&self, plugin_id: &PluginId) -> PathBuf {
        self.root.join(plugin_id.as_str())
    }

    fn write_state(
        &self,
        plugin_id: &PluginId,
        snapshot: &PluginSnapshot,
    ) -> Result<(), PluginServiceError> {
        let state = PersistedPluginState {
            status: snapshot.status,
            installed_at_unix_ms: snapshot.installed_at_unix_ms,
            updated_at_unix_ms: snapshot.updated_at_unix_ms,
        };
        write_json_atomically(&self.plugin_dir(plugin_id).join(STATE_FILE_NAME), &state)
    }
}

impl PluginRepository for LocalPluginRepository {
    fn read_package(&self, manifest_path: &Path) -> Result<PluginPackage, PluginServiceError> {
        let manifest_bytes =
            read_regular_file(manifest_path, MAX_PLUGIN_MANIFEST_BYTES, "plugin manifest")?;
        let manifest: PluginManifest =
            serde_json::from_slice(&manifest_bytes).map_err(|error| {
                PluginServiceError::invalid_manifest(format!("plugin manifest is invalid: {error}"))
            })?;
        validate_manifest(&manifest)?;
        let parent = manifest_path.parent().ok_or_else(|| {
            PluginServiceError::invalid_input("plugin manifest has no parent directory")
        })?;
        let module = read_regular_file(
            &parent.join(&manifest.entrypoint),
            MAX_PLUGIN_MODULE_BYTES,
            "plugin module",
        )?;
        Ok(PluginPackage { manifest, module })
    }

    fn install(&self, package: PluginPackage) -> Result<PluginSnapshot, PluginServiceError> {
        let mut records = self.records.lock().map_err(|_| repository_unavailable())?;
        let plugin_id = package.manifest.id.clone();
        if records.contains_key(&plugin_id) {
            return Err(PluginServiceError::new(
                PluginServiceErrorKind::AlreadyInstalled,
                "plugin is already installed",
            ));
        }

        let target = self.plugin_dir(&plugin_id);
        if target.exists() {
            return Err(PluginServiceError::new(
                PluginServiceErrorKind::Conflict,
                "plugin storage directory already exists",
            ));
        }
        let temporary = self.root.join(format!(
            ".install-{}-{}-{}",
            plugin_id.as_str(),
            std::process::id(),
            unix_time_ms()
        ));
        fs::create_dir(&temporary)
            .map_err(|error| io_error("create plugin staging directory", error))?;

        let now = unix_time_ms();
        let snapshot = PluginSnapshot {
            manifest: package.manifest,
            status: PluginStatus::Disabled,
            installed_at_unix_ms: now,
            updated_at_unix_ms: now,
        };
        let result = (|| {
            write_json(
                &temporary.join(PLUGIN_MANIFEST_FILE_NAME),
                &snapshot.manifest,
            )?;
            fs::write(
                temporary.join(&snapshot.manifest.entrypoint),
                package.module,
            )
            .map_err(|error| io_error("write plugin module", error))?;
            let state = PersistedPluginState {
                status: snapshot.status,
                installed_at_unix_ms: snapshot.installed_at_unix_ms,
                updated_at_unix_ms: snapshot.updated_at_unix_ms,
            };
            write_json(&temporary.join(STATE_FILE_NAME), &state)?;
            fs::rename(&temporary, &target)
                .map_err(|error| io_error("activate plugin installation", error))?;
            Ok::<(), PluginServiceError>(())
        })();
        if let Err(error) = result {
            let _ = fs::remove_dir_all(&temporary);
            return Err(error);
        }

        records.insert(plugin_id, snapshot.clone());
        Ok(snapshot)
    }

    fn list(&self) -> Result<Vec<PluginSnapshot>, PluginServiceError> {
        let records = self.records.lock().map_err(|_| repository_unavailable())?;
        Ok(records.values().cloned().collect())
    }

    fn get(&self, plugin_id: &PluginId) -> Result<Option<PluginSnapshot>, PluginServiceError> {
        let records = self.records.lock().map_err(|_| repository_unavailable())?;
        Ok(records.get(plugin_id).cloned())
    }

    fn module_bytes(&self, plugin_id: &PluginId) -> Result<Vec<u8>, PluginServiceError> {
        let entrypoint = {
            let records = self.records.lock().map_err(|_| repository_unavailable())?;
            records
                .get(plugin_id)
                .map(|snapshot| snapshot.manifest.entrypoint.clone())
                .ok_or_else(|| {
                    PluginServiceError::new(
                        PluginServiceErrorKind::NotFound,
                        "plugin was not found",
                    )
                })?
        };
        read_regular_file(
            &self.plugin_dir(plugin_id).join(entrypoint),
            MAX_PLUGIN_MODULE_BYTES,
            "installed plugin module",
        )
    }

    fn set_status(
        &self,
        plugin_id: &PluginId,
        status: PluginStatus,
    ) -> Result<PluginSnapshot, PluginServiceError> {
        let mut records = self.records.lock().map_err(|_| repository_unavailable())?;
        let current = records.get(plugin_id).cloned().ok_or_else(|| {
            PluginServiceError::new(PluginServiceErrorKind::NotFound, "plugin was not found")
        })?;
        let updated = PluginSnapshot {
            status,
            updated_at_unix_ms: next_updated_at(&current, unix_time_ms()),
            ..current
        };
        self.write_state(plugin_id, &updated)?;
        records.insert(plugin_id.clone(), updated.clone());
        Ok(updated)
    }

    fn remove(&self, plugin_id: &PluginId) -> Result<bool, PluginServiceError> {
        let mut records = self.records.lock().map_err(|_| repository_unavailable())?;
        if !records.contains_key(plugin_id) {
            return Ok(false);
        }
        fs::remove_dir_all(self.plugin_dir(plugin_id))
            .map_err(|error| io_error("remove plugin directory", error))?;
        records.remove(plugin_id);
        Ok(true)
    }
}

fn load_installed_plugins(root: &Path) -> BTreeMap<PluginId, PluginSnapshot> {
    let mut records = BTreeMap::new();
    let Ok(entries) = fs::read_dir(root) else {
        return records;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            continue;
        }
        if let Err(error) = cleanup_state_temp_files(&path) {
            tracing::warn!(error = %error, "failed to clean plugin state temporary files");
        }
        match snapshot_from_disk(&path) {
            Ok(snapshot) => {
                records.insert(snapshot.manifest.id.clone(), snapshot);
            }
            Err(error) => tracing::warn!(error = %error, "ignoring invalid installed plugin"),
        }
    }
    records
}

fn snapshot_from_disk(plugin_dir: &Path) -> Result<PluginSnapshot, PluginServiceError> {
    let manifest_bytes = read_regular_file(
        &plugin_dir.join(PLUGIN_MANIFEST_FILE_NAME),
        MAX_PLUGIN_MANIFEST_BYTES,
        "installed plugin manifest",
    )?;
    let manifest: PluginManifest = serde_json::from_slice(&manifest_bytes).map_err(|error| {
        PluginServiceError::invalid_manifest(format!(
            "installed plugin manifest is invalid: {error}"
        ))
    })?;
    validate_manifest(&manifest)?;
    if plugin_dir.file_name().and_then(|value| value.to_str()) != Some(manifest.id.as_str()) {
        return Err(PluginServiceError::invalid_manifest(
            "installed plugin directory does not match its manifest id",
        ));
    }
    read_regular_file(
        &plugin_dir.join(&manifest.entrypoint),
        MAX_PLUGIN_MODULE_BYTES,
        "installed plugin module",
    )?;
    let state_bytes = read_regular_file(
        &plugin_dir.join(STATE_FILE_NAME),
        MAX_PLUGIN_MANIFEST_BYTES,
        "installed plugin state",
    )?;
    let state: PersistedPluginState = serde_json::from_slice(&state_bytes).map_err(|error| {
        PluginServiceError::invalid_manifest(format!("installed plugin state is invalid: {error}"))
    })?;
    if state.installed_at_unix_ms == 0 || state.updated_at_unix_ms < state.installed_at_unix_ms {
        return Err(PluginServiceError::invalid_manifest(
            "installed plugin timestamps are invalid",
        ));
    }
    Ok(PluginSnapshot {
        manifest,
        status: state.status,
        installed_at_unix_ms: state.installed_at_unix_ms,
        updated_at_unix_ms: state.updated_at_unix_ms,
    })
}

fn read_regular_file(
    path: &Path,
    max_bytes: u64,
    description: &str,
) -> Result<Vec<u8>, PluginServiceError> {
    let path_metadata = fs::symlink_metadata(path).map_err(|error| io_error(description, error))?;
    if path_metadata.file_type().is_symlink() || !path_metadata.is_file() {
        return Err(PluginServiceError::invalid_manifest(format!(
            "{description} must be a regular file"
        )));
    }

    let file = File::open(path).map_err(|error| io_error(description, error))?;
    let metadata = file
        .metadata()
        .map_err(|error| io_error(description, error))?;
    if metadata.len() == 0 {
        return Err(PluginServiceError::invalid_manifest(format!(
            "{description} is empty"
        )));
    }
    if metadata.len() > max_bytes {
        return Err(PluginServiceError::new(
            PluginServiceErrorKind::PackageTooLarge,
            format!("{description} exceeds its size limit"),
        ));
    }

    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(max_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| io_error(description, error))?;
    if bytes.len() as u64 > max_bytes {
        return Err(PluginServiceError::new(
            PluginServiceErrorKind::PackageTooLarge,
            format!("{description} grew beyond its size limit"),
        ));
    }
    Ok(bytes)
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), PluginServiceError> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| {
        PluginServiceError::new(
            PluginServiceErrorKind::Io,
            format!("serialize plugin data: {error}"),
        )
    })?;
    fs::write(path, bytes).map_err(|error| io_error("write plugin data", error))
}

fn write_json_atomically(path: &Path, value: &impl Serialize) -> Result<(), PluginServiceError> {
    let parent = path.parent().ok_or_else(|| {
        PluginServiceError::new(
            PluginServiceErrorKind::Io,
            "plugin state path has no parent directory",
        )
    })?;
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| {
        PluginServiceError::new(
            PluginServiceErrorKind::Io,
            format!("serialize plugin data: {error}"),
        )
    })?;
    let mut temporary = TempFileBuilder::new()
        .prefix(STATE_TEMP_FILE_PREFIX)
        .suffix(STATE_TEMP_FILE_SUFFIX)
        .tempfile_in(parent)
        .map_err(|error| io_error("create plugin state temporary file", error))?;
    temporary
        .write_all(&bytes)
        .and_then(|_| temporary.flush())
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|error| io_error("sync plugin state temporary file", error))?;
    temporary
        .persist(path)
        .map_err(|error| io_error("replace plugin state", error.error))?;
    if let Err(error) = sync_directory(parent) {
        tracing::warn!(error = %error, "failed to sync plugin state directory after replacement");
    }
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), PluginServiceError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| io_error("sync plugin state directory", error))
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<(), PluginServiceError> {
    Ok(())
}

fn cleanup_state_temp_files(plugin_dir: &Path) -> Result<(), PluginServiceError> {
    let entries = fs::read_dir(plugin_dir)
        .map_err(|error| io_error("read plugin directory for temporary state cleanup", error))?;
    for entry in entries {
        let entry = entry.map_err(|error| io_error("read plugin directory entry", error))?;
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        if file_name.starts_with(STATE_TEMP_FILE_PREFIX)
            && file_name.ends_with(STATE_TEMP_FILE_SUFFIX)
        {
            fs::remove_file(entry.path())
                .map_err(|error| io_error("remove plugin state temporary file", error))?;
        }
    }
    Ok(())
}

fn next_updated_at(snapshot: &PluginSnapshot, now: u64) -> u64 {
    now.max(snapshot.installed_at_unix_ms)
        .max(snapshot.updated_at_unix_ms)
}

fn io_error(operation: &str, error: std::io::Error) -> PluginServiceError {
    PluginServiceError::new(PluginServiceErrorKind::Io, format!("{operation}: {error}"))
}

fn repository_unavailable() -> PluginServiceError {
    PluginServiceError::new(
        PluginServiceErrorKind::Conflict,
        "plugin repository state is unavailable",
    )
}

fn unix_time_ms() -> u64 {
    let milliseconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    milliseconds.min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::plugin::PluginCommand;

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

    #[test]
    fn runtime_validates_and_executes_a_sandboxed_command() {
        let runtime = WasmtimePluginRuntime::new().expect("runtime should initialize");
        let wasm = wat::parse_str(
            r#"(module
                (func (export "shendesk_plugin_api_version") (result i32) i32.const 1)
                (func (export "hello") (result i32) i32.const 7)
            )"#,
        )
        .expect("WAT fixture should compile");

        runtime
            .validate(&wasm, &manifest())
            .expect("module should validate");
        let result = runtime.call(&wasm, "hello").expect("command should run");
        assert_eq!(result.return_code, 7);
        assert!(result.fuel_consumed > 0);
    }

    #[test]
    fn runtime_rejects_imports_excess_memory_and_wrong_api_versions() {
        let runtime = WasmtimePluginRuntime::new().expect("runtime should initialize");
        assert_eq!(
            runtime
                .validate(b"(module)", &manifest())
                .expect_err("WAT text must not be accepted as an installable module")
                .kind(),
            PluginServiceErrorKind::RuntimeRejected
        );

        let imported = wat::parse_str(
            r#"(module
                (import "host" "read_file" (func))
                (func (export "shendesk_plugin_api_version") (result i32) i32.const 1)
                (func (export "hello") (result i32) i32.const 0)
            )"#,
        )
        .expect("WAT fixture should compile");
        assert_eq!(
            runtime
                .validate(&imported, &manifest())
                .expect_err("imports must fail")
                .kind(),
            PluginServiceErrorKind::RuntimeRejected
        );

        let excessive_memory = wat::parse_str(
            r#"(module
                (memory 1025)
                (func (export "shendesk_plugin_api_version") (result i32) i32.const 1)
                (func (export "hello") (result i32) i32.const 0)
            )"#,
        )
        .expect("WAT fixture should compile");
        assert_eq!(
            runtime
                .validate(&excessive_memory, &manifest())
                .expect_err("excessive memory must fail")
                .kind(),
            PluginServiceErrorKind::RuntimeRejected
        );

        let wrong_api = wat::parse_str(
            r#"(module
                (func (export "shendesk_plugin_api_version") (result i32) i32.const 2)
                (func (export "hello") (result i32) i32.const 0)
            )"#,
        )
        .expect("WAT fixture should compile");
        assert_eq!(
            runtime
                .validate(&wrong_api, &manifest())
                .expect_err("wrong API must fail")
                .kind(),
            PluginServiceErrorKind::RuntimeRejected
        );
    }

    #[test]
    fn runtime_traps_unbounded_execution_when_fuel_is_exhausted() {
        let runtime = WasmtimePluginRuntime::new().expect("runtime should initialize");
        let spinning = wat::parse_str(
            r#"(module
                (func (export "shendesk_plugin_api_version") (result i32) i32.const 1)
                (func (export "hello") (result i32)
                    (loop $spin br $spin)
                    i32.const 0)
            )"#,
        )
        .expect("WAT fixture should compile");
        runtime
            .validate(&spinning, &manifest())
            .expect("non-running command should validate");
        assert_eq!(
            runtime
                .call(&spinning, "hello")
                .expect_err("fuel exhaustion must trap")
                .kind(),
            PluginServiceErrorKind::ExecutionFailed
        );
    }

    #[test]
    fn repository_persists_lifecycle_state_and_removes_packages() {
        let root = unique_temp_dir("repository");
        let repository = LocalPluginRepository::new(&root).expect("repository should initialize");
        let snapshot = repository
            .install(PluginPackage {
                manifest: manifest(),
                module: b"test-module".to_vec(),
            })
            .expect("plugin should install");
        assert_eq!(snapshot.status, PluginStatus::Disabled);

        let enabled = repository
            .set_status(&snapshot.manifest.id, PluginStatus::Enabled)
            .expect("status should persist");
        assert_eq!(enabled.status, PluginStatus::Enabled);
        assert_eq!(repository.list().expect("list should work").len(), 1);

        drop(repository);
        let reloaded = LocalPluginRepository::new(&root).expect("repository should reload");
        assert_eq!(
            reloaded
                .get(&snapshot.manifest.id)
                .expect("get should work")
                .expect("plugin should exist")
                .status,
            PluginStatus::Enabled
        );
        assert!(reloaded
            .remove(&snapshot.manifest.id)
            .expect("remove should work"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn interrupted_state_write_preserves_the_last_valid_state() {
        let root = unique_temp_dir("interrupted-state");
        let repository = LocalPluginRepository::new(&root).expect("repository should initialize");
        let snapshot = repository
            .install(PluginPackage {
                manifest: manifest(),
                module: b"test-module".to_vec(),
            })
            .expect("plugin should install");
        repository
            .set_status(&snapshot.manifest.id, PluginStatus::Enabled)
            .expect("enabled state should persist");
        let plugin_dir = root.join(snapshot.manifest.id.as_str());
        let state_path = plugin_dir.join(STATE_FILE_NAME);
        let valid_state = fs::read(&state_path).expect("valid state should be readable");

        fs::write(plugin_dir.join(".state-interrupted.tmp"), b"{")
            .expect("interrupted temporary state should be created");

        assert_eq!(
            fs::read(&state_path).expect("last valid state should remain readable"),
            valid_state
        );
        assert_eq!(
            snapshot_from_disk(&plugin_dir)
                .expect("last valid state should still load")
                .status,
            PluginStatus::Enabled
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn repository_cleans_leftover_state_temp_files_on_startup() {
        let root = unique_temp_dir("state-cleanup");
        let repository = LocalPluginRepository::new(&root).expect("repository should initialize");
        let snapshot = repository
            .install(PluginPackage {
                manifest: manifest(),
                module: b"test-module".to_vec(),
            })
            .expect("plugin should install");
        let leftover = root
            .join(snapshot.manifest.id.as_str())
            .join(".state-leftover.tmp");
        fs::write(&leftover, b"partial").expect("leftover state should be created");
        drop(repository);

        let reloaded = LocalPluginRepository::new(&root).expect("repository should reload");

        assert!(!leftover.exists());
        assert!(reloaded
            .get(&snapshot.manifest.id)
            .expect("get should work")
            .is_some());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn updated_timestamp_remains_monotonic_when_the_clock_moves_backwards() {
        let root = unique_temp_dir("clock-rollback");
        let repository = LocalPluginRepository::new(&root).expect("repository should initialize");
        let snapshot = repository
            .install(PluginPackage {
                manifest: manifest(),
                module: b"test-module".to_vec(),
            })
            .expect("plugin should install");
        let future_update = unix_time_ms().saturating_add(60_000);
        repository
            .records
            .lock()
            .expect("records lock")
            .get_mut(&snapshot.manifest.id)
            .expect("plugin should exist")
            .updated_at_unix_ms = future_update;

        let updated = repository
            .set_status(&snapshot.manifest.id, PluginStatus::Enabled)
            .expect("status should persist despite clock rollback");

        assert_eq!(updated.updated_at_unix_ms, future_update);
        drop(repository);
        assert_eq!(
            LocalPluginRepository::new(&root)
                .expect("repository should reload")
                .get(&snapshot.manifest.id)
                .expect("get should work")
                .expect("plugin should remain installed")
                .updated_at_unix_ms,
            future_update
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn repository_ignores_a_directory_that_does_not_match_manifest_id() {
        let root = unique_temp_dir("mismatch");
        let wrong_dir = root.join("com.shencom.wrong");
        fs::create_dir_all(&wrong_dir).expect("directory should be created");
        write_json(&wrong_dir.join(PLUGIN_MANIFEST_FILE_NAME), &manifest())
            .expect("manifest should be written");
        fs::write(wrong_dir.join("hello.wasm"), b"module").expect("module should be written");
        write_json(
            &wrong_dir.join(STATE_FILE_NAME),
            &PersistedPluginState {
                status: PluginStatus::Disabled,
                installed_at_unix_ms: 1,
                updated_at_unix_ms: 1,
            },
        )
        .expect("state should be written");

        let repository = LocalPluginRepository::new(&root).expect("repository should initialize");
        assert!(repository.list().expect("list should work").is_empty());
        let _ = fs::remove_dir_all(root);
    }

    fn unique_temp_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "shendesk-plugin-{label}-{}-{}",
            std::process::id(),
            unix_time_ms()
        ))
    }
}
