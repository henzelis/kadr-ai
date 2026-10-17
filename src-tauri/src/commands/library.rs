use kadr_core::persistence::Db;
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
