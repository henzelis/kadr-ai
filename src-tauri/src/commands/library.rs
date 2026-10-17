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
