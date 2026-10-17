use kadr_core::services::indexer::Helpers;
use kadr_core::services::tools::ToolStatus;
use kadr_core::settings::AppSettings;
use tauri::State;

use crate::app_state::AppState;
use crate::errors::AppError;

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, AppError> {
    Ok(state.db()?.load_settings()?)
}

/// Validates and stores the settings, returning what was stored. When a
/// change affects derived dates or places, the index is recomputed from the
/// stored metadata (no file is read).
#[tauri::command]
pub async fn update_settings(
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<AppSettings, AppError> {
    let previous = {
        let db = state.db()?;
        let previous = db.load_settings()?;
        // The destination is chosen through its own validated command.
        let settings = AppSettings {
            destination_root: previous.destination_root.clone(),
            ..settings
        };
        db.save_settings(&settings)?;
        previous
    };
    let stored = state.db()?.load_settings()?;
    if previous.exiftool_path != stored.exiftool_path
        || previous.ffprobe_path != stored.ffprobe_path
        || previous.thumbnail_workers != stored.thumbnail_workers
    {
        state.rebuild_thumbnailer(&stored);
    }
    if previous.affects_derivation(&stored) {
        if state.lock(&state.scan)?.is_some() {
            return Err(AppError::user("saved_after_scan"));
        }
        crate::commands::geobase::renormalize_in_background(&state, stored.clone()).await?;
    }
    Ok(stored)
}

/// Probes ExifTool, FFprobe and FFmpeg. Runs off the main thread: a hung
/// helper must not freeze the window.
#[tauri::command]
pub async fn get_tool_status(state: State<'_, AppState>) -> Result<Vec<ToolStatus>, AppError> {
    let settings = state.db()?.load_settings()?;
    let bundled = state.helpers_dir.clone();
    tauri::async_runtime::spawn_blocking(move || Helpers::detect(&settings, bundled.as_deref()).1)
        .await
        .map_err(|e| AppError::internal(&e.to_string()))
}
