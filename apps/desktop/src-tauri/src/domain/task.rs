use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskId(pub String);

impl TaskId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

impl fmt::Display for TaskId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Pending,
    Running,
    Success,
    Failed,
    Cancelled,
}

impl TaskState {
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Success | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskProgress {
    pub completed: u64,
    pub total: u64,
    pub percentage: u8,
}

impl TaskProgress {
    pub fn new(total: u64) -> Self {
        Self {
            completed: 0,
            total,
            percentage: 0,
        }
    }

    pub fn update(&mut self, completed: u64) {
        self.completed = completed.min(self.total);
        self.percentage = calculate_percentage(self.completed, self.total);
    }

    pub fn complete(&mut self) {
        self.update(self.total);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSnapshot {
    pub id: TaskId,
    pub name: String,
    pub state: TaskState,
    pub progress: TaskProgress,
    pub error: Option<String>,
}

impl TaskSnapshot {
    pub fn pending(id: TaskId, name: String, total: u64) -> Self {
        Self {
            id,
            name,
            state: TaskState::Pending,
            progress: TaskProgress::new(total),
            error: None,
        }
    }
}

fn calculate_percentage(completed: u64, total: u64) -> u8 {
    if total == 0 {
        return 0;
    }

    ((u128::from(completed) * 100) / u128::from(total)) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_is_clamped_and_calculated_without_overflow() {
        let mut progress = TaskProgress::new(u64::MAX);

        progress.update(u64::MAX / 2);
        assert_eq!(progress.percentage, 49);

        progress.update(u64::MAX);
        assert_eq!(progress.completed, u64::MAX);
        assert_eq!(progress.percentage, 100);
    }

    #[test]
    fn progress_never_exceeds_total() {
        let mut progress = TaskProgress::new(4);

        progress.update(9);

        assert_eq!(progress.completed, 4);
        assert_eq!(progress.percentage, 100);
    }

    #[test]
    fn task_state_has_stable_wire_values() {
        assert_eq!(
            serde_json::to_string(&TaskState::Pending).expect("state should serialize"),
            "\"pending\""
        );
        assert_eq!(
            serde_json::to_string(&TaskState::Success).expect("state should serialize"),
            "\"success\""
        );
        assert_eq!(
            serde_json::to_string(&TaskState::Cancelled).expect("state should serialize"),
            "\"cancelled\""
        );
    }
}
