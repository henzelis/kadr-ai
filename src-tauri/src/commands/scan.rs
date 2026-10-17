use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use kadr_core::persistence::Db;
use kadr_core::services::indexer::{Helpers, ScanPhase, ScanProgress, ScanRequest, run_scan};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::app_state::{AppState, RunningScan};
use crate::errors::AppError;

/// Progress events are throttled so a fast scan does not flood the UI.
const EMIT_EVERY: Duration = Duration::from_millis(150);

#[tauri::command]
pub fn start_scan(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    let mut running = state.lock(&state.scan)?;
    if running.is_some() {
        return Err(AppError::user("scan_running", "Сканування вже триває."));
    }
    let (roots, settings) = {
        let db = state.db()?;
        (db.source_roots()?, db.load_settings()?)
    };
    if roots.is_empty() {
        return Err(AppError::user(
            "no_sources",
            "Спочатку додайте папки з фото й відео.",
        ));
    }
    let cancel = Arc::new(AtomicBool::new(false));
    *running = Some(RunningScan {
        cancel: cancel.clone(),
    });
    drop(running);

    let db_path = state.db_path.clone();
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        let result = (|| -> Result<ScanProgress, AppError> {
            let mut db = Db::open(&db_path)?;
            let (helpers, _) = Helpers::detect(&settings, state.helpers_dir.as_deref());
            let exclude = settings.destination_root.iter().map(Into::into).collect();
            let req = ScanRequest {
                roots: &roots,
                settings: &settings,
                exclude,
            };
            let mut last_emit = Instant::now() - EMIT_EVERY;
            let mut on_progress = |p: &ScanProgress| {
                if let Ok(mut slot) = state.last_progress.lock() {
                    *slot = Some(p.clone());
                }
                let final_phase = matches!(p.phase, ScanPhase::Done | ScanPhase::Cancelled);
                if final_phase || last_emit.elapsed() >= EMIT_EVERY {
                    last_emit = Instant::now();
                    let _ = app.emit("scan-progress", p);
                }
            };
            let geocoder = state.geocoder();
            let geo = geocoder
                .as_deref()
                .map(|g| g as &dyn kadr_core::services::place::ReverseGeocoder);
            Ok(run_scan(
                &mut db,
                &req,
                &helpers,
                geo,
                &cancel,
                &mut on_progress,
            )?)
        })();
        if let Ok(mut slot) = state.scan.lock() {
            *slot = None;
        }
        match result {
            Ok(p) => {
                let _ = app.emit("scan-finished", &p);
            }
            Err(err) => {
                let _ = app.emit("scan-failed", &err);
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub fn cancel_scan(state: State<'_, AppState>) -> Result<bool, AppError> {
    Ok(match state.lock(&state.scan)?.as_ref() {
        Some(scan) => {
            scan.cancel.store(true, Ordering::Relaxed);
            true
        }
        None => false,
    })
}

#[derive(serde::Serialize)]
pub struct ScanStatus {
    pub running: bool,
    pub progress: Option<ScanProgress>,
    pub last: Option<kadr_core::persistence::media::LastScan>,
}

#[tauri::command]
pub fn get_scan_status(state: State<'_, AppState>) -> Result<ScanStatus, AppError> {
    Ok(ScanStatus {
        running: state.lock(&state.scan)?.is_some(),
        progress: state.lock(&state.last_progress)?.clone(),
        last: state.db()?.last_scan()?,
    })
}
