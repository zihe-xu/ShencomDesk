use std::time::Duration;

use serde::Deserialize;
use tauri::State;

use crate::{
    app::state::AppState,
    application::task_service::{TaskManagerError, TaskService},
    domain::task::{TaskId, TaskSnapshot},
    infrastructure::logging,
};

use super::error::{IpcError, IpcResult};

const MAX_TASK_NAME_CHARS: usize = 128;
const MAX_TASK_ID_CHARS: usize = 128;
const MAX_TOTAL_STEPS: u64 = 10_000;
const MAX_STEP_DELAY_MS: u64 = 1_000;
const MAX_TASK_DURATION_MS: u64 = 10 * 60 * 1_000;
const DEFAULT_STEP_DELAY_MS: u64 = 50;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskRequest {
    name: String,
    total_steps: u64,
    #[serde(default = "default_step_delay_ms")]
    step_delay_ms: u64,
}

#[derive(Debug)]
struct ValidatedTaskRequest {
    name: String,
    total_steps: u64,
    step_delay: Duration,
}

/// Creates a bounded background progress task used by the desktop shell and IPC integration.
#[tauri::command]
pub fn create_task(
    state: State<'_, AppState>,
    request: CreateTaskRequest,
) -> IpcResult<TaskSnapshot> {
    let request = validate_request(request)?;

    TaskService::create_progress_task(
        state.task_manager(),
        request.name,
        request.total_steps,
        request.step_delay,
    )
    .inspect(|_| logging::record_operation("ipc.task.create", "success"))
    .map_err(map_task_manager_error)
}

/// Returns the latest snapshot for one task.
#[tauri::command]
pub fn get_task_status(state: State<'_, AppState>, task_id: String) -> IpcResult<TaskSnapshot> {
    let task_id = parse_task_id(task_id)?;

    TaskService::get(state.task_manager(), &task_id)
        .inspect(|_| logging::record_operation("ipc.task.get", "success"))
        .ok_or_else(|| {
            logging::record_operation("ipc.task.get", "not_found");
            IpcError::task_not_found()
        })
}

/// Lists task snapshots in creation order.
#[tauri::command]
pub fn list_tasks(state: State<'_, AppState>) -> Vec<TaskSnapshot> {
    let tasks = TaskService::list(state.task_manager());
    logging::record_operation("ipc.task.list", "success");
    tasks
}

/// Requests cooperative cancellation and immediately exposes the cancelled state.
#[tauri::command]
pub fn cancel_task(state: State<'_, AppState>, task_id: String) -> IpcResult<TaskSnapshot> {
    let task_id = parse_task_id(task_id)?;

    TaskService::cancel(state.task_manager(), &task_id)
        .inspect(|_| logging::record_operation("ipc.task.cancel", "success"))
        .ok_or_else(|| {
            logging::record_operation("ipc.task.cancel", "not_found");
            IpcError::task_not_found()
        })
}

fn validate_request(request: CreateTaskRequest) -> IpcResult<ValidatedTaskRequest> {
    let name = request.name.trim().to_owned();
    let duration_ms = request.total_steps.saturating_mul(request.step_delay_ms);

    if name.is_empty()
        || name.chars().count() > MAX_TASK_NAME_CHARS
        || request.total_steps == 0
        || request.total_steps > MAX_TOTAL_STEPS
        || request.step_delay_ms > MAX_STEP_DELAY_MS
        || duration_ms > MAX_TASK_DURATION_MS
    {
        return Err(IpcError::validation());
    }

    Ok(ValidatedTaskRequest {
        name,
        total_steps: request.total_steps,
        step_delay: Duration::from_millis(request.step_delay_ms),
    })
}

fn parse_task_id(task_id: String) -> IpcResult<TaskId> {
    let task_id = task_id.trim();
    if task_id.is_empty() || task_id.chars().count() > MAX_TASK_ID_CHARS {
        return Err(IpcError::validation());
    }

    Ok(TaskId::new(task_id))
}

fn map_task_manager_error(error: TaskManagerError) -> IpcError {
    tracing::error!(error = %error, "IPC task creation failed");
    logging::record_operation("ipc.task.create", "failed");

    match error {
        TaskManagerError::InvalidName | TaskManagerError::InvalidTotal => IpcError::validation(),
        TaskManagerError::QueueUnavailable => IpcError::task_queue_unavailable(),
    }
}

const fn default_step_delay_ms() -> u64 {
    DEFAULT_STEP_DELAY_MS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_bounded_progress_task_request() {
        let validated = validate_request(CreateTaskRequest {
            name: "  index files  ".to_owned(),
            total_steps: 20,
            step_delay_ms: 10,
        })
        .expect("request should be valid");

        assert_eq!(validated.name, "index files");
        assert_eq!(validated.total_steps, 20);
        assert_eq!(validated.step_delay, Duration::from_millis(10));
    }

    #[test]
    fn rejects_empty_or_excessively_long_tasks() {
        for request in [
            CreateTaskRequest {
                name: " ".to_owned(),
                total_steps: 1,
                step_delay_ms: 0,
            },
            CreateTaskRequest {
                name: "slow".to_owned(),
                total_steps: MAX_TOTAL_STEPS,
                step_delay_ms: MAX_STEP_DELAY_MS,
            },
        ] {
            assert_eq!(
                validate_request(request)
                    .expect_err("request should be rejected")
                    .code,
                super::super::error::IpcErrorCode::ValidationFailed
            );
        }
    }

    #[test]
    fn applies_default_step_delay_during_deserialization() {
        let request: CreateTaskRequest = serde_json::from_value(serde_json::json!({
            "name": "sync",
            "totalSteps": 2
        }))
        .expect("request should deserialize");

        assert_eq!(request.step_delay_ms, DEFAULT_STEP_DELAY_MS);
    }
}
