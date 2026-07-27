use serde::{Deserialize, Serialize};

pub const PLUGIN_API_VERSION: u32 = 1;
pub const PLUGIN_MANIFEST_FILE_NAME: &str = "plugin.json";
pub const PLUGIN_API_VERSION_EXPORT: &str = "shendesk_plugin_api_version";
pub const PLUGIN_ENABLE_EXPORT: &str = "shendesk_on_enable";
pub const PLUGIN_DISABLE_EXPORT: &str = "shendesk_on_disable";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PluginId(String);

impl PluginId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PluginId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginCommand {
    pub name: String,
    pub export: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginManifest {
    pub api_version: u32,
    pub id: PluginId,
    pub name: String,
    pub version: String,
    pub entrypoint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub commands: Vec<PluginCommand>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginStatus {
    Disabled,
    Enabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSnapshot {
    pub manifest: PluginManifest,
    pub status: PluginStatus,
    pub installed_at_unix_ms: u64,
    pub updated_at_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginExecution {
    pub plugin_id: PluginId,
    pub command: String,
    pub return_code: i32,
    pub fuel_consumed: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_uses_a_versioned_camel_case_wire_format() {
        let manifest: PluginManifest = serde_json::from_value(serde_json::json!({
            "apiVersion": 1,
            "id": "com.shencom.hello",
            "name": "Hello",
            "version": "1.0.0",
            "entrypoint": "hello.wasm",
            "commands": [{
                "name": "hello",
                "export": "hello"
            }]
        }))
        .expect("manifest should deserialize");

        assert_eq!(manifest.api_version, PLUGIN_API_VERSION);
        assert_eq!(manifest.id.as_str(), "com.shencom.hello");
        assert_eq!(manifest.commands[0].export, "hello");
    }

    #[test]
    fn manifest_rejects_unknown_fields() {
        let error = serde_json::from_value::<PluginManifest>(serde_json::json!({
            "apiVersion": 1,
            "id": "com.shencom.hello",
            "name": "Hello",
            "version": "1.0.0",
            "entrypoint": "hello.wasm",
            "unexpected": true
        }))
        .expect_err("unknown fields must be rejected");

        assert!(error.to_string().contains("unknown field"));
    }
}
