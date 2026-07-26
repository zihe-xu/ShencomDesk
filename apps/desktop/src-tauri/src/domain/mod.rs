//! Pure domain types. This layer must not depend on Tauri or infrastructure.

use serde::{Deserialize, Serialize};

pub mod config;
pub mod task;

pub mod user {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct UserId(pub String);
}

pub mod project {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct ProjectId(pub String);
}

pub mod document {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct DocumentId(pub String);
}

pub mod health {
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct HealthStatus {
        pub status: String,
        pub app_name: String,
        pub version: String,
        pub uptime_seconds: u64,
    }

    impl HealthStatus {
        pub fn ready(uptime_seconds: u64) -> Self {
            Self {
                status: "ready".to_owned(),
                app_name: "ShenDesk".to_owned(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
                uptime_seconds,
            }
        }
    }
}
