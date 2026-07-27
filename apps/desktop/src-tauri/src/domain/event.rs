use serde::{Deserialize, Serialize};

use super::task::TaskSnapshot;

/// Stable event categories used by subscribers to select the messages they need.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    ApplicationReady,
    ApplicationExiting,
    TaskCreated,
    TaskStarted,
    TaskProgressed,
    TaskFinished,
    FileChanged,
    UserLoggedIn,
    UpdateAvailable,
}

/// Domain events exchanged between ShenDesk modules.
///
/// Events contain domain data only. They do not depend on Tauri, Tokio, or an
/// infrastructure adapter, which keeps publishers and subscribers decoupled
/// from the transport used by the EventBus.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum AppEvent {
    ApplicationReady,
    ApplicationExiting,
    TaskCreated { task: TaskSnapshot },
    TaskStarted { task: TaskSnapshot },
    TaskProgressed { task: TaskSnapshot },
    TaskFinished { task: TaskSnapshot },
    FileChanged { path: String },
    UserLoggedIn { user_id: String },
    UpdateAvailable { version: String },
}

impl AppEvent {
    pub const fn kind(&self) -> EventKind {
        match self {
            Self::ApplicationReady => EventKind::ApplicationReady,
            Self::ApplicationExiting => EventKind::ApplicationExiting,
            Self::TaskCreated { .. } => EventKind::TaskCreated,
            Self::TaskStarted { .. } => EventKind::TaskStarted,
            Self::TaskProgressed { .. } => EventKind::TaskProgressed,
            Self::TaskFinished { .. } => EventKind::TaskFinished,
            Self::FileChanged { .. } => EventKind::FileChanged,
            Self::UserLoggedIn { .. } => EventKind::UserLoggedIn,
            Self::UpdateAvailable { .. } => EventKind::UpdateAvailable,
        }
    }
}

/// Metadata assigned once when an event is published.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventEnvelope {
    pub sequence: u64,
    pub published_at_unix_ms: u64,
    pub event: AppEvent,
}

impl EventEnvelope {
    pub fn new(sequence: u64, published_at_unix_ms: u64, event: AppEvent) -> Self {
        Self {
            sequence,
            published_at_unix_ms,
            event,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::task::{TaskId, TaskSnapshot};

    #[test]
    fn event_kind_matches_each_wire_event() {
        let task = TaskSnapshot::pending(TaskId::new("task-1"), "index".to_owned(), 10);

        assert_eq!(
            AppEvent::ApplicationReady.kind(),
            EventKind::ApplicationReady
        );
        assert_eq!(
            AppEvent::TaskCreated { task: task.clone() }.kind(),
            EventKind::TaskCreated
        );
        assert_eq!(
            AppEvent::TaskFinished { task }.kind(),
            EventKind::TaskFinished
        );
    }

    #[test]
    fn envelope_has_stable_tagged_serialization() {
        let envelope = EventEnvelope::new(
            7,
            123,
            AppEvent::UpdateAvailable {
                version: "2.0.0".to_owned(),
            },
        );
        let value = serde_json::to_value(envelope).expect("event should serialize");

        assert_eq!(value["sequence"], 7);
        assert_eq!(value["publishedAtUnixMs"], 123);
        assert_eq!(value["event"]["type"], "update_available");
        assert_eq!(value["event"]["payload"]["version"], "2.0.0");
    }
}
