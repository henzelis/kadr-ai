use std::path::{Path, PathBuf};

use kadr_core::persistence::media::SourceRoot;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::app_state::AppState;
use crate::errors::AppError;

/// Checks a folder chosen by the user and returns its canonical form.
fn validated_dir(app: &AppHandle, raw: &str) -> Result<PathBuf, AppError> {
    let path = Path::new(raw.trim());
    if !path.is_absolute() {
        return Err(AppError::user(
            "invalid_path",
            "Потрібен повний шлях до папки.",
        ));
    }
    let canonical = dunce_canonicalize(path).map_err(|_| {
        AppError::user(
            "invalid_path",
            "Папку не знайдено або до неї немає доступу.",
        )
    })?;
    if !canonical.is_dir() {
        return Err(AppError::user("invalid_path", "Це не папка."));
    }
    if let Ok(data) = app.path().app_data_dir()
        && (canonical.starts_with(&data) || data.starts_with(&canonical))
    {
        return Err(AppError::user(
            "invalid_path",
            "Ця папка містить службові дані Kadr і не може бути вибрана.",
        ));
    }
    Ok(canonical)
}

/// `canonicalize` on Windows returns `\\?\C:\...`; keep the familiar form.
fn dunce_canonicalize(path: &Path) -> std::io::Result<PathBuf> {
    let p = std::fs::canonicalize(path)?;
    #[cfg(windows)]
    {
        let s = p.to_string_lossy();
        if let Some(rest) = s.strip_prefix(r"\\?\")
            && !rest.starts_with("UNC\\")
        {
            return Ok(PathBuf::from(rest));
        }
    }
    Ok(p)
}

#[derive(Serialize)]
pub struct Folders {
    pub sources: Vec<SourceRoot>,
    pub destination: Option<String>,
    /// Shown as a warning in the UI: the archive lies inside a source.
    pub destination_inside_source: bool,
}

fn folders(state: &AppState) -> Result<Folders, AppError> {
    let db = state.db()?;
    let sources = db.source_roots()?;
    let destination = db.load_settings()?.destination_root;
    let destination_inside_source = destination.as_deref().is_some_and(|d| {
        sources
            .iter()
            .any(|s| Path::new(d).starts_with(&s.path) || Path::new(&s.path).starts_with(d))
    });
    Ok(Folders {
        sources,
        destination,
        destination_inside_source,
    })
}

#[tauri::command]
pub fn get_folders(state: State<'_, AppState>) -> Result<Folders, AppError> {
    folders(&state)
}

#[tauri::command]
pub fn add_source_roots(
    app: AppHandle,
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> Result<Folders, AppError> {
    let dirs = paths
        .iter()
        .map(|p| validated_dir(&app, p))
        .collect::<Result<Vec<_>, _>>()?;
    {
        let db = state.db()?;
        for dir in dirs {
            db.add_source_root(&dir.to_string_lossy())?;
        }
    }
    folders(&state)
}

#[tauri::command]
pub fn remove_source_root(state: State<'_, AppState>, id: i64) -> Result<Folders, AppError> {
    if state.lock(&state.scan)?.is_some() {
        return Err(AppError::user(
            "scan_running",
            "Дочекайтеся завершення сканування.",
        ));
    }
    state.db()?.remove_source_root(id)?;
    folders(&state)
}

#[tauri::command]
pub fn set_destination_root(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<Folders, AppError> {
    let dir = validated_dir(&app, &path)?;
    {
        let db = state.db()?;
        let mut settings = db.load_settings()?;
        settings.destination_root = Some(dir.to_string_lossy().into_owned());
        db.save_settings(&settings)?;
    }
    folders(&state)
}
