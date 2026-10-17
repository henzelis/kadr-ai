use std::path::Path;
use std::sync::Arc;

use kadr_core::services::geocoder::{GeobaseInfo, OfflineGeocoder};
use kadr_core::services::indexer::renormalize;
use kadr_core::services::place::ReverseGeocoder;
use kadr_core::settings::AppSettings;
use tauri::State;

use crate::app_state::AppState;
use crate::errors::AppError;

/// Recomputes dates and places from stored metadata off the UI thread.
pub async fn renormalize_in_background(
    state: &AppState,
    settings: AppSettings,
) -> Result<usize, AppError> {
    let path = state.db_path.clone();
    let geocoder = state.geocoder();
    tauri::async_runtime::spawn_blocking(move || -> Result<usize, AppError> {
        let mut db = kadr_core::persistence::Db::open(&path)?;
        let geo = geocoder.as_deref().map(|g| g as &dyn ReverseGeocoder);
        Ok(renormalize(&mut db, &settings, geo)?)
    })
    .await
    .map_err(|e| AppError::internal(&e.to_string()))?
}

#[tauri::command]
pub fn get_geobase_status(state: State<'_, AppState>) -> Result<Option<GeobaseInfo>, AppError> {
    Ok(state.geocoder().map(|g| g.info().clone()))
}

/// Validates a geobase file chosen by the user, copies it into the app data
/// directory, switches to it and recomputes place names.
#[tauri::command]
pub async fn import_geobase(
    state: State<'_, AppState>,
    path: String,
) -> Result<GeobaseInfo, AppError> {
    if state.lock(&state.scan)?.is_some() {
        return Err(AppError::user(
            "scan_running",
            "Дочекайтеся завершення сканування.",
        ));
    }
    let source = Path::new(path.trim()).to_path_buf();
    let target = state.geobase_path.clone();
    let geocoder =
        tauri::async_runtime::spawn_blocking(move || -> Result<OfflineGeocoder, AppError> {
            // Check before copying, so a wrong file never replaces a working one.
            OfflineGeocoder::open(&source).map_err(|_| {
                AppError::user(
                    "invalid_geobase",
                    "Це не файл геобази Kadr або він пошкоджений.",
                )
            })?;
            let tmp = target.with_extension("sqlite3.tmp");
            std::fs::copy(&source, &tmp).map_err(|e| AppError::internal(&e.to_string()))?;
            std::fs::rename(&tmp, &target).map_err(|e| AppError::internal(&e.to_string()))?;
            OfflineGeocoder::open(&target).map_err(AppError::from)
        })
        .await
        .map_err(|e| AppError::internal(&e.to_string()))??;
    let info = geocoder.info().clone();
    if let Ok(mut slot) = state.geocoder.write() {
        *slot = Some(Arc::new(geocoder));
    }
    let settings = state.db()?.load_settings()?;
    renormalize_in_background(&state, settings).await?;
    Ok(info)
}
