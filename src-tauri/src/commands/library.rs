use kadr_core::persistence::Db;
use kadr_core::persistence::media::{MediaPage, MediaQuery, MediaRow, SectionCounts};
use serde::Serialize;
use tauri::State;

use crate::app_state::AppState;
use crate::errors::AppError;

#[derive(Debug, Serialize)]
pub struct LibrarySummary {
    pub media_count: u64,
    pub schema_version: i64,
    pub supported_schema_version: i64,
    pub app_version: &'static str,
}

#[tauri::command]
pub fn get_library_summary(state: State<'_, AppState>) -> Result<LibrarySummary, AppError> {
    let db = state.db()?;
    Ok(LibrarySummary {
        media_count: db.media_count()?,
        schema_version: db.schema_version()?,
        supported_schema_version: Db::supported_version(),
        app_version: env!("CARGO_PKG_VERSION"),
    })
}

#[tauri::command]
pub fn query_media(state: State<'_, AppState>, query: MediaQuery) -> Result<MediaPage, AppError> {
    Ok(state.db()?.query_media(&query)?)
}

#[tauri::command]
pub fn get_media_details(
    state: State<'_, AppState>,
    id: i64,
) -> Result<Option<MediaRow>, AppError> {
    Ok(state.db()?.media_details(id)?)
}

#[tauri::command]
pub fn get_section_counts(state: State<'_, AppState>) -> Result<SectionCounts, AppError> {
    Ok(state.db()?.section_counts()?)
}

/// Applies a manual edit to `ids` and returns the refreshed rows.
async fn after_edit(state: &AppState, ids: Vec<i64>) -> Result<Vec<MediaRow>, AppError> {
    let settings = state.db()?.load_settings()?;
    let path = state.db_path.clone();
    let geocoder = state.geocoder();
    let ids2 = ids.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<(), AppError> {
        let mut db = Db::open(&path)?;
        let geo = geocoder
            .as_deref()
            .map(|g| g as &dyn kadr_core::services::place::ReverseGeocoder);
        kadr_core::services::indexer::renormalize_items(&mut db, Some(&ids2), &settings, geo)?;
        Ok(())
    })
    .await
    .map_err(|e| AppError::internal(&e.to_string()))??;
    let db = state.db()?;
    Ok(ids
        .iter()
        .filter_map(|id| db.media_details(*id).ok().flatten())
        .collect())
}

fn check_ids(ids: &[i64]) -> Result<(), AppError> {
    if ids.is_empty() || ids.len() > 100_000 {
        return Err(AppError::user("invalid_selection"));
    }
    Ok(())
}

/// Sets the user's place name (or clears it with `null`). Original files
/// are never modified.
#[tauri::command]
pub async fn set_manual_place(
    state: State<'_, AppState>,
    ids: Vec<i64>,
    place: Option<String>,
) -> Result<Vec<MediaRow>, AppError> {
    check_ids(&ids)?;
    state.db()?.set_manual_place(&ids, place.as_deref())?;
    after_edit(&state, ids).await
}

/// Sets the user's capture date `YYYY-MM-DDTHH:MM` (or clears it with
/// `null`). Original files are never modified.
#[tauri::command]
pub async fn set_manual_date(
    state: State<'_, AppState>,
    ids: Vec<i64>,
    date: Option<String>,
) -> Result<Vec<MediaRow>, AppError> {
    check_ids(&ids)?;
    state.db()?.set_manual_date(&ids, date.as_deref())?;
    after_edit(&state, ids).await
}
