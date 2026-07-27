use std::fmt;

use serde::{Deserialize, Serialize};

/// Stable identifier for one active filesystem watch registration.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FileWatchId(pub String);

impl FileWatchId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

impl fmt::Display for FileWatchId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileChangeKind {
    Created,
    Modified,
    Removed,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub watch_id: FileWatchId,
    pub path: String,
    pub kind: FileChangeKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub path: String,
    pub name: String,
    pub extension: Option<String>,
    pub size_bytes: u64,
    pub modified_at_unix_ms: Option<u64>,
    pub is_directory: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileReadResult {
    pub entry: FileEntry,
    pub content: String,
    pub from_cache: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileIndex {
    pub root: String,
    pub entries: Vec<FileEntry>,
    pub scanned_at_unix_ms: u64,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileWatch {
    pub id: FileWatchId,
    pub root: String,
    pub recursive: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_change_uses_stable_wire_values() {
        let change = FileChange {
            watch_id: FileWatchId::new("watch-1"),
            path: "/tmp/example.txt".to_owned(),
            kind: FileChangeKind::Modified,
        };

        let value = serde_json::to_value(change).expect("change should serialize");
        assert_eq!(value["watchId"], "watch-1");
        assert_eq!(value["kind"], "modified");
    }

    #[test]
    fn file_read_result_marks_cache_origin() {
        let result = FileReadResult {
            entry: FileEntry {
                path: "/tmp/example.txt".to_owned(),
                name: "example.txt".to_owned(),
                extension: Some("txt".to_owned()),
                size_bytes: 5,
                modified_at_unix_ms: Some(123),
                is_directory: false,
            },
            content: "hello".to_owned(),
            from_cache: true,
        };

        let value = serde_json::to_value(result).expect("result should serialize");
        assert_eq!(value["fromCache"], true);
        assert_eq!(value["entry"]["sizeBytes"], 5);
    }
}
