use tauri::{App, Manager};

use super::{lifecycle, state::AppState};

/// Initializes shared runtime resources before the main window is used.
pub fn initialize(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    app.manage(AppState::new());
    lifecycle::on_ready(app.handle());

    Ok(())
}
