use serde::{Deserialize, Serialize};

pub const CURRENT_CONFIG_SCHEMA_VERSION: u32 = 1;

fn default_theme() -> String {
    "dark".to_owned()
}

fn default_language() -> String {
    "zh-CN".to_owned()
}

fn default_auto_start() -> bool {
    true
}

/// User-editable application configuration persisted as JSON in SQLite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_auto_start")]
    pub auto_start: bool,
}

impl AppConfig {
    /// Upgrades older configuration payloads and normalizes unsupported values.
    pub fn migrate(mut self) -> Self {
        if self.schema_version < CURRENT_CONFIG_SCHEMA_VERSION {
            self.schema_version = CURRENT_CONFIG_SCHEMA_VERSION;
        }

        if !matches!(self.theme.as_str(), "dark" | "light" | "system") {
            self.theme = default_theme();
        }

        if self.language.trim().is_empty() {
            self.language = default_language();
        }

        self
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_CONFIG_SCHEMA_VERSION,
            theme: default_theme(),
            language: default_language(),
            auto_start: default_auto_start(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_legacy_configuration_without_schema_version() {
        let legacy: AppConfig =
            serde_json::from_str(r#"{"theme":"light","language":"en-US","autoStart":false}"#)
                .expect("legacy configuration should deserialize");

        let migrated = legacy.migrate();

        assert_eq!(migrated.schema_version, CURRENT_CONFIG_SCHEMA_VERSION);
        assert_eq!(migrated.theme, "light");
        assert_eq!(migrated.language, "en-US");
        assert!(!migrated.auto_start);
    }

    #[test]
    fn normalizes_invalid_values() {
        let migrated = AppConfig {
            schema_version: 0,
            theme: "unknown".to_owned(),
            language: " ".to_owned(),
            auto_start: true,
        }
        .migrate();

        assert_eq!(migrated, AppConfig::default());
    }
}
