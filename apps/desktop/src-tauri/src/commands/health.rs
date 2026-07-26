use tauri::State;

use crate::{
    app::state::AppState, application::health_service::HealthService, domain::health::HealthStatus,
};

/// Returns the desktop runtime health snapshot.
#[tauri::command]
pub fn health_check(state: State<'_, AppState>) -> HealthStatus {
    HealthService::check(state.uptime().as_secs())
}
