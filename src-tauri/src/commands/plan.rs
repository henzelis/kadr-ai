use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use kadr_core::domain::operation::OperationMode;
use kadr_core::persistence::Db;
use kadr_core::persistence::plans::{PlanFilter, PlanPage, PlanSummary, RunSummary};
use kadr_core::services::operations::{NoHooks, RunPhase, RunProgress, apply_plan, undo_plan};
use kadr_core::services::planner::create_plan;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::app_state::{AppState, RunningScan};
use crate::errors::AppError;

const EMIT_EVERY: Duration = Duration::from_millis(150);

fn busy(state: &AppState) -> Result<(), AppError> {
    if state.lock(&state.scan)?.is_some() {
        return Err(AppError::user("scan_running"));
    }
    if state.lock(&state.run)?.is_some() {
        return Err(AppError::user("run_in_progress"));
    }
    Ok(())
}

/// Builds a new plan from the index (replaces the previous draft).
#[tauri::command]
pub async fn build_sort_plan(
    state: State<'_, AppState>,
    mode: OperationMode,
) -> Result<PlanSummary, AppError> {
    busy(&state)?;
    let settings = state.db()?.load_settings()?;
    if settings.destination_root.is_none() {
        return Err(AppError::user("no_destination"));
    }
    let path = state.db_path.clone();
    let id = tauri::async_runtime::spawn_blocking(move || -> Result<i64, AppError> {
        let mut db = Db::open(&path)?;
        Ok(create_plan(&mut db, &settings, mode)?)
    })
    .await
    .map_err(|e| AppError::internal(&e.to_string()))??;
    state
        .db()?
        .plan_summary(id)?
        .ok_or_else(|| AppError::internal("plan vanished"))
}

/// The latest plan (draft or already run), if any.
#[tauri::command]
pub fn get_plan_summary(
    state: State<'_, AppState>,
    plan_id: Option<i64>,
) -> Result<Option<PlanSummary>, AppError> {
    let db = state.db()?;
    let id = match plan_id {
        Some(id) => Some(id),
        None => db.latest_plan_id()?,
    };
    Ok(match id {
        Some(id) => db.plan_summary(id)?,
        None => None,
    })
}

#[tauri::command]
pub fn get_plan_items(
    state: State<'_, AppState>,
    plan_id: i64,
    filter: PlanFilter,
    search: String,
    offset: u32,
    limit: u32,
) -> Result<PlanPage, AppError> {
    Ok(state
        .db()?
        .plan_items(plan_id, filter, &search, offset, limit)?)
}

#[tauri::command]
pub fn set_plan_included(
    state: State<'_, AppState>,
    plan_id: i64,
    item_ids: Vec<i64>,
    included: bool,
) -> Result<Option<PlanSummary>, AppError> {
    let mut db = state.db()?;
    db.set_plan_included(plan_id, &item_ids, included)?;
    Ok(db.plan_summary(plan_id)?)
}

#[tauri::command]
pub fn set_plan_mode(
    state: State<'_, AppState>,
    plan_id: i64,
    mode: OperationMode,
) -> Result<Option<PlanSummary>, AppError> {
    let db = state.db()?;
    db.set_plan_mode(plan_id, mode)?;
    Ok(db.plan_summary(plan_id)?)
}

/// What the user saw and agreed to in the confirmation dialog. The run only
/// starts if it still matches the plan.
#[derive(Debug, Deserialize)]
pub struct Confirmation {
    pub mode: OperationMode,
    pub files: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum RunKind {
    Apply,
    Undo,
}

fn spawn_run(
    app: AppHandle,
    state: &AppState,
    plan_id: i64,
    kind: RunKind,
) -> Result<(), AppError> {
    let cancel = Arc::new(AtomicBool::new(false));
    *state.lock(&state.run)? = Some(RunningScan {
        cancel: cancel.clone(),
    });
    let db_path = state.db_path.clone();
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        let result = (|| -> Result<RunProgress, AppError> {
            let mut db = Db::open(&db_path)?;
            let mut last = Instant::now() - EMIT_EVERY;
            let mut on_progress = |p: &RunProgress| {
                let last_event = matches!(p.phase, RunPhase::Done | RunPhase::Partial);
                if last_event || last.elapsed() >= EMIT_EVERY {
                    last = Instant::now();
                    let _ = app.emit("run-progress", p);
                }
            };
            Ok(match kind {
                RunKind::Apply => {
                    apply_plan(&mut db, plan_id, &cancel, &NoHooks, &mut on_progress)?
                }
                RunKind::Undo => undo_plan(&mut db, plan_id, &cancel, &mut on_progress)?,
            })
        })();
        if let Ok(mut slot) = state.run.lock() {
            *slot = None;
        }
        match result {
            Ok(p) => {
                let _ = app.emit(
                    "run-finished",
                    serde_json::json!({ "kind": kind, "progress": p }),
                );
            }
            Err(err) => {
                let _ = app.emit("run-failed", &err);
            }
        }
    });
    Ok(())
}

/// Starts (or continues) a plan after the user confirmed it.
#[tauri::command]
pub fn apply_sort_plan(
    app: AppHandle,
    state: State<'_, AppState>,
    plan_id: i64,
    confirmation: Confirmation,
) -> Result<(), AppError> {
    busy(&state)?;
    {
        let db = state.db()?;
        let summary = db
            .plan_summary(plan_id)?
            .ok_or_else(|| AppError::user("plan_missing"))?;
        let continuing = summary.state == "partial";
        if summary.mode != confirmation.mode
            || (!continuing && summary.included_files != confirmation.files)
        {
            return Err(AppError::user("plan_changed"));
        }
        if summary.state != "draft" && !continuing {
            return Err(AppError::user("plan_not_runnable"));
        }
    }
    spawn_run(app, &state, plan_id, RunKind::Apply)
}

#[tauri::command]
pub fn undo_sort_plan(
    app: AppHandle,
    state: State<'_, AppState>,
    plan_id: i64,
) -> Result<(), AppError> {
    busy(&state)?;
    spawn_run(app, &state, plan_id, RunKind::Undo)
}

#[tauri::command]
pub fn cancel_run(state: State<'_, AppState>) -> Result<bool, AppError> {
    Ok(match state.lock(&state.run)?.as_ref() {
        Some(run) => {
            run.cancel.store(true, Ordering::Relaxed);
            true
        }
        None => false,
    })
}

#[derive(Serialize)]
pub struct RunFailure {
    pub source: String,
    pub destination: String,
    pub reason: Option<kadr_core::domain::note::Note>,
}

#[derive(Serialize)]
pub struct RunLog {
    pub running: bool,
    pub runs: Vec<RunSummary>,
}

#[tauri::command]
pub fn get_run_log(state: State<'_, AppState>) -> Result<RunLog, AppError> {
    Ok(RunLog {
        running: state.lock(&state.run)?.is_some(),
        runs: state.db()?.run_summaries()?,
    })
}

#[tauri::command]
pub fn get_run_failures(
    state: State<'_, AppState>,
    plan_id: i64,
) -> Result<Vec<RunFailure>, AppError> {
    Ok(state
        .db()?
        .failed_items(plan_id)?
        .into_iter()
        .map(|(source, destination, reason)| RunFailure {
            source,
            destination,
            reason,
        })
        .collect())
}
