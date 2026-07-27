use serde::{Deserialize, Serialize};

/// Public update metadata returned to the React client.
///
/// Download URLs and signatures deliberately remain inside the infrastructure
/// adapter and never cross the IPC boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current_version: String,
    pub version: String,
    pub notes: Option<String>,
    pub published_at_unix_seconds: Option<i64>,
    pub target: String,
}

/// Ordered download progress sent over a Tauri IPC channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    tag = "event",
    content = "data"
)]
pub enum UpdateProgress {
    Started {
        content_length: Option<u64>,
    },
    Progress {
        chunk_length: u64,
        downloaded: u64,
        content_length: Option<u64>,
    },
    Finished {
        downloaded: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInstallResult {
    pub installed: bool,
    pub restart_requested: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_events_use_a_stable_tagged_wire_format() {
        let value = serde_json::to_value(UpdateProgress::Progress {
            chunk_length: 512,
            downloaded: 1_024,
            content_length: Some(4_096),
        })
        .expect("progress should serialize");

        assert_eq!(value["event"], "progress");
        assert_eq!(value["data"]["chunkLength"], 512);
        assert_eq!(value["data"]["downloaded"], 1_024);
        assert_eq!(value["data"]["contentLength"], 4_096);
    }

    #[test]
    fn update_metadata_does_not_contain_transport_secrets() {
        let value = serde_json::to_value(UpdateInfo {
            current_version: "0.1.0".to_owned(),
            version: "0.2.0".to_owned(),
            notes: Some("Security update".to_owned()),
            published_at_unix_seconds: Some(1_785_100_000),
            target: "darwin-aarch64".to_owned(),
        })
        .expect("update info should serialize");

        assert!(value.get("downloadUrl").is_none());
        assert!(value.get("signature").is_none());
        assert_eq!(value["version"], "0.2.0");
    }
}
