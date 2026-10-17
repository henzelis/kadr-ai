use kadr_core::services::tools::{self, Tool, ToolStatus};
use kadr_core::settings::AppSettings;
use tauri::State;

use crate::app_state::AppState;
use crate::errors::AppError;

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, AppError> {
    Ok(state.db()?.load_settings()?)
}

/// Validates and stores the settings, returning what was stored.
#[tauri::command]
pub fn update_settings(
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<AppSettings, AppError> {
    let db = state.db()?;
    db.save_settings(&settings)?;
    Ok(db.load_settings()?)
}

/// Probes ExifTool and FFprobe. Runs off the main thread: a hung helper
/// must not freeze the window.
#[tauri::command]
pub async fn get_tool_status(state: State<'_, AppState>) -> Result<Vec<ToolStatus>, AppError> {
    let settings = state.db()?.load_settings()?;
    tauri::async_runtime::spawn_blocking(move || {
        vec![
            tools::probe(Tool::Exiftool, settings.exiftool_path.as_deref()),
            tools::probe(Tool::Ffprobe, settings.ffprobe_path.as_deref()),
        ]
    })
    .await
    .map_err(|e| AppError::internal(&e.to_string()))
}
