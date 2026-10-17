//! Sorting plans in the index: inputs for the planner, saving a plan, and
//! paged reading for the plan screen.

use std::path::PathBuf;

use chrono::NaiveDateTime;
use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use super::Db;
use super::media::{LOCAL_TIME_FORMAT, enum_parse, enum_str, json_notes, now_ms};
use crate::domain::note::Note;
use crate::domain::operation::{OperationMode, OperationStatus};
use crate::errors::CoreResult;
use crate::services::companions::CompanionRole;
use crate::services::planner::{CompanionInput, EntryRole, PlanEntry, PlanInput, PlanStatus};

#[derive(Debug, Clone, Serialize)]
pub struct PlanSummary {
    pub id: i64,
    pub created_at: i64,
    pub destination_root: String,
    pub mode: OperationMode,
    pub state: String,
    /// Main files per plan status.
    pub ready: u64,
    pub conflicts: u64,
    pub already_archived: u64,
    pub already_in_place: u64,
    pub duplicates: u64,
    /// Main files and companions that will be copied or moved.
    pub included_files: u64,
    pub included_bytes: u64,
    pub no_date: u64,
    pub no_place: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanFilter {
    #[default]
    All,
    Included,
    Excluded,
    /// Conflicts, duplicates, no date or no place.
    Attention,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanCompanion {
    pub id: i64,
    pub file_name: String,
    pub destination: String,
    pub role: EntryRole,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanRow {
    pub id: i64,
    pub media_id: Option<i64>,
    pub source: String,
    pub destination: String,
    pub size: u64,
    pub captured_local: Option<String>,
    pub place: Option<String>,
    pub status: PlanStatus,
    pub included: bool,
    pub item_status: OperationStatus,
    pub notes: Vec<Note>,
    pub companions: Vec<PlanCompanion>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanPage {
    pub items: Vec<PlanRow>,
    pub total: u64,
}

fn u(v: i64) -> u64 {
    u64::try_from(v).unwrap_or(0)
}

impl Db {
    /// Every visible main file with its companions, for the planner.
    pub fn plan_inputs(&self) -> CoreResult<Vec<PlanInput>> {
        let mut companions: std::collections::HashMap<i64, Vec<CompanionInput>> =
            std::collections::HashMap::new();
        {
            let mut stmt = self.conn.prepare(
                "SELECT sidecar_parent_id, id, path, file_name, size_bytes, mtime, companion_role FROM media_item
                 WHERE sidecar_parent_id IS NOT NULL AND is_missing = 0 ORDER BY file_name",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    CompanionInput {
                        id: r.get(1)?,
                        path: PathBuf::from(r.get::<_, String>(2)?),
                        file_name: r.get(3)?,
                        size: u(r.get(4)?),
                        mtime: r.get(5)?,
                        role: r
                            .get::<_, Option<String>>(6)?
                            .and_then(|s| enum_parse(&s))
                            .unwrap_or(CompanionRole::Sidecar),
                    },
                ))
            })?;
            for row in rows {
                let (parent, c) = row?;
                companions.entry(parent).or_default().push(c);
            }
        }
        let mut stmt = self.conn.prepare(
            "SELECT id, path, file_name, size_bytes, mtime, sha256, captured_at_local, place_label FROM media_item
             WHERE is_sidecar = 0 AND is_missing = 0",
        )?;
        let rows = stmt.query_map([], |r| {
            let id: i64 = r.get(0)?;
            Ok(PlanInput {
                id,
                path: PathBuf::from(r.get::<_, String>(1)?),
                file_name: r.get(2)?,
                size: u(r.get(3)?),
                mtime: r.get(4)?,
                sha256: r.get(5)?,
                captured: r
                    .get::<_, Option<String>>(6)?
                    .and_then(|s| NaiveDateTime::parse_from_str(&s, LOCAL_TIME_FORMAT).ok()),
                place: r.get(7)?,
                companions: Vec::new(),
            })
        })?;
        let mut out: Vec<PlanInput> = rows.collect::<Result<_, _>>()?;
        for input in &mut out {
            input.companions = companions.remove(&input.id).unwrap_or_default();
        }
        Ok(out)
    }

    /// Stores a freshly built plan and returns its id. Older draft plans are
    /// dropped: only the newest unapplied plan is kept.
    pub fn save_plan(
        &mut self,
        destination_root: &str,
        mode: OperationMode,
        template: &str,
        settings_json: &str,
        entries: &[PlanEntry],
    ) -> CoreResult<i64> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM operation_plan WHERE state = 'draft'", [])?;
        tx.execute(
            "INSERT INTO operation_plan (created_at, destination_root, mode, conflict_policy, template, settings_json, state)
             VALUES (?1, ?2, ?3, '', ?4, ?5, 'draft')",
            params![now_ms(), destination_root, enum_str(&mode), template, settings_json],
        )?;
        let plan_id = tx.last_insert_rowid();
        let mut ids: Vec<i64> = Vec::with_capacity(entries.len());
        {
            let mut stmt = tx.prepare(
                "INSERT INTO operation_item (plan_id, media_id, source_path, destination_path, expected_size,
                    expected_mtime, hash_before, status, warnings_json, role, parent_item_id, plan_status,
                    included, captured_local, place_label, seq)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            )?;
            for (seq, e) in entries.iter().enumerate() {
                let parent = e.parent.and_then(|i| ids.get(i).copied());
                stmt.execute(params![
                    plan_id,
                    e.media_id,
                    e.source.to_string_lossy(),
                    e.destination.to_string_lossy(),
                    i64::try_from(e.size).unwrap_or(i64::MAX),
                    e.mtime,
                    e.sha256,
                    enum_str(&if e.included {
                        OperationStatus::Planned
                    } else {
                        OperationStatus::Excluded
                    }),
                    serde_json::to_string(&e.notes)?,
                    enum_str(&e.role),
                    parent,
                    enum_str(&e.status),
                    e.included,
                    e.captured.map(|d| d.format(LOCAL_TIME_FORMAT).to_string()),
                    e.place,
                    i64::try_from(seq).unwrap_or(i64::MAX),
                ])?;
                ids.push(tx.last_insert_rowid());
            }
        }
        tx.commit()?;
        Ok(plan_id)
    }

    /// Drops plans that were never run, e.g. after folder names changed.
    pub fn discard_draft_plans(&self) -> CoreResult<usize> {
        Ok(self
            .conn
            .execute("DELETE FROM operation_plan WHERE state = 'draft'", [])?)
    }

    pub fn latest_plan_id(&self) -> CoreResult<Option<i64>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id FROM operation_plan ORDER BY id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn plan_summary(&self, plan_id: i64) -> CoreResult<Option<PlanSummary>> {
        let head = self
            .conn
            .query_row(
                "SELECT id, created_at, destination_root, mode, state FROM operation_plan WHERE id = ?1",
                params![plan_id],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()?;
        let Some((id, created_at, destination_root, mode, state)) = head else {
            return Ok(None);
        };
        let counts = self.conn.query_row(
            "SELECT
                COALESCE(SUM(role = 'main' AND plan_status = 'ready'), 0),
                COALESCE(SUM(role = 'main' AND plan_status = 'conflict'), 0),
                COALESCE(SUM(role = 'main' AND plan_status = 'already_archived'), 0),
                COALESCE(SUM(role = 'main' AND plan_status = 'already_in_place'), 0),
                COALESCE(SUM(role = 'main' AND plan_status = 'duplicate'), 0),
                COALESCE(SUM(included), 0),
                COALESCE(SUM(CASE WHEN included THEN expected_size ELSE 0 END), 0),
                COALESCE(SUM(role = 'main' AND captured_local IS NULL), 0),
                COALESCE(SUM(role = 'main' AND place_label IS NULL), 0)
             FROM operation_item WHERE plan_id = ?1",
            params![plan_id],
            |r| {
                Ok([
                    r.get::<_, i64>(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                    r.get(8)?,
                ])
            },
        )?;
        Ok(Some(PlanSummary {
            id,
            created_at,
            destination_root,
            mode: enum_parse(&mode).unwrap_or_default(),
            state,
            ready: u(counts[0]),
            conflicts: u(counts[1]),
            already_archived: u(counts[2]),
            already_in_place: u(counts[3]),
            duplicates: u(counts[4]),
            included_files: u(counts[5]),
            included_bytes: u(counts[6]),
            no_date: u(counts[7]),
            no_place: u(counts[8]),
        }))
    }

    pub fn plan_items(
        &self,
        plan_id: i64,
        filter: PlanFilter,
        search: &str,
        offset: u32,
        limit: u32,
    ) -> CoreResult<PlanPage> {
        let filter_sql = match filter {
            PlanFilter::All => "1",
            PlanFilter::Included => "i.included = 1",
            PlanFilter::Excluded => "i.included = 0",
            PlanFilter::Attention => {
                "(i.plan_status IN ('conflict', 'duplicate') OR i.captured_local IS NULL OR i.place_label IS NULL
                  OR i.warnings_json LIKE '%plan_renamed%' OR i.warnings_json LIKE '%plan_long_path%')"
            }
        };
        let search = search.trim().to_lowercase();
        let pattern = format!(
            "%{}%",
            search
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );
        let base = format!(
            "FROM operation_item i WHERE i.plan_id = ?1 AND i.role = 'main' AND {filter_sql}
             AND (?2 = '' OR lower(i.source_path || ' ' || i.destination_path) LIKE ?3 ESCAPE '\\')"
        );
        let total: i64 = self.conn.query_row(
            &format!("SELECT COUNT(*) {base}"),
            params![plan_id, search, pattern],
            |r| r.get(0),
        )?;
        let limit = if limit == 0 { 200 } else { limit.min(1000) };
        let mut stmt = self.conn.prepare(&format!(
            "SELECT i.id, i.media_id, i.source_path, i.destination_path, i.expected_size, i.captured_local,
                    i.place_label, i.plan_status, i.included, i.status, i.warnings_json
             {base} ORDER BY i.seq LIMIT ?4 OFFSET ?5"
        ))?;
        let read = |r: &Row<'_>| {
            Ok(PlanRow {
                id: r.get(0)?,
                media_id: r.get(1)?,
                source: r.get(2)?,
                destination: r.get(3)?,
                size: u(r.get(4)?),
                captured_local: r.get(5)?,
                place: r.get(6)?,
                status: enum_parse(&r.get::<_, String>(7)?).unwrap_or(PlanStatus::Ready),
                included: r.get(8)?,
                item_status: enum_parse(&r.get::<_, String>(9)?)
                    .unwrap_or(OperationStatus::Planned),
                notes: json_notes(r.get(10)?),
                companions: Vec::new(),
            })
        };
        let mut items: Vec<PlanRow> = stmt
            .query_map(params![plan_id, search, pattern, limit, offset], read)?
            .collect::<Result<_, _>>()?;
        let mut comp = self.conn.prepare_cached(
            "SELECT id, source_path, destination_path, role FROM operation_item WHERE parent_item_id = ?1 ORDER BY seq",
        )?;
        for row in &mut items {
            row.companions = comp
                .query_map(params![row.id], |r| {
                    let source: String = r.get(1)?;
                    Ok(PlanCompanion {
                        id: r.get(0)?,
                        file_name: std::path::Path::new(&source)
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                        destination: r.get(2)?,
                        role: enum_parse(&r.get::<_, String>(3)?).unwrap_or(EntryRole::Sidecar),
                    })
                })?
                .collect::<Result<_, _>>()?;
        }
        Ok(PlanPage {
            items,
            total: u(total),
        })
    }

    /// Includes or excludes main files; their companions follow. Only a
    /// draft plan can be changed.
    pub fn set_plan_included(
        &mut self,
        plan_id: i64,
        item_ids: &[i64],
        included: bool,
    ) -> CoreResult<()> {
        let status = enum_str(&if included {
            OperationStatus::Planned
        } else {
            OperationStatus::Excluded
        });
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "UPDATE operation_item SET included = ?3, status = ?4
                 WHERE plan_id = ?1 AND (id = ?2 OR parent_item_id = ?2)
                   AND plan_status NOT IN ('already_archived', 'already_in_place')
                   AND EXISTS (SELECT 1 FROM operation_plan p WHERE p.id = ?1 AND p.state = 'draft')",
            )?;
            for id in item_ids {
                stmt.execute(params![plan_id, id, included, status])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn set_plan_mode(&self, plan_id: i64, mode: OperationMode) -> CoreResult<()> {
        self.conn.execute(
            "UPDATE operation_plan SET mode = ?2 WHERE id = ?1 AND state = 'draft'",
            params![plan_id, enum_str(&mode)],
        )?;
        Ok(())
    }
}

/// One plan item as the runner sees it.
#[derive(Debug, Clone)]
pub struct RunItem {
    pub id: i64,
    pub media_id: Option<i64>,
    pub parent_item_id: Option<i64>,
    pub source: PathBuf,
    pub destination: PathBuf,
    pub expected_size: u64,
    pub expected_mtime: i64,
    pub sha256: Option<String>,
    pub hash_after: Option<String>,
    pub status: OperationStatus,
}

fn read_run_item(r: &Row<'_>) -> rusqlite::Result<RunItem> {
    Ok(RunItem {
        id: r.get(0)?,
        media_id: r.get(1)?,
        parent_item_id: r.get(2)?,
        source: PathBuf::from(r.get::<_, String>(3)?),
        destination: PathBuf::from(r.get::<_, String>(4)?),
        expected_size: u(r.get(5)?),
        expected_mtime: r.get(6)?,
        sha256: r.get(7)?,
        hash_after: r.get(8)?,
        status: enum_parse(&r.get::<_, String>(9)?).unwrap_or(OperationStatus::Planned),
    })
}

const RUN_COLUMNS: &str =
    "id, media_id, parent_item_id, source_path, destination_path, expected_size,
                           expected_mtime, hash_before, hash_after, status";

/// One finished or running plan for the activity log.
#[derive(Debug, Clone, Serialize)]
pub struct RunSummary {
    pub plan_id: i64,
    pub created_at: i64,
    pub destination_root: String,
    pub mode: OperationMode,
    pub state: String,
    pub done: u64,
    pub failed: u64,
    pub undone: u64,
    pub pending: u64,
    pub bytes_done: u64,
}

impl Db {
    pub fn plan_state(&self, plan_id: i64) -> CoreResult<Option<(String, OperationMode, String)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT state, mode, destination_root FROM operation_plan WHERE id = ?1",
                params![plan_id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        enum_parse(&r.get::<_, String>(1)?).unwrap_or_default(),
                        r.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?)
    }

    pub fn set_plan_state(&self, plan_id: i64, state: &str) -> CoreResult<()> {
        self.conn.execute(
            "UPDATE operation_plan SET state = ?2 WHERE id = ?1",
            params![plan_id, state],
        )?;
        Ok(())
    }

    /// Included items still to do (planned, failed or interrupted), in plan
    /// order; companions come right after their main file.
    pub fn pending_run_items(&self, plan_id: i64) -> CoreResult<Vec<RunItem>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {RUN_COLUMNS} FROM operation_item
             WHERE plan_id = ?1 AND included = 1 AND status IN ('planned', 'failed', 'interrupted')
             ORDER BY seq"
        ))?;
        let rows = stmt.query_map(params![plan_id], read_run_item)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Done items, newest first, for undo.
    pub fn done_run_items(&self, plan_id: i64) -> CoreResult<Vec<RunItem>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {RUN_COLUMNS} FROM operation_item WHERE plan_id = ?1 AND status = 'done' ORDER BY seq DESC"
        ))?;
        let rows = stmt.query_map(params![plan_id], read_run_item)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn mark_item(
        &self,
        item_id: i64,
        status: OperationStatus,
        destination: Option<&std::path::Path>,
        hash_after: Option<&str>,
        note: Option<&Note>,
    ) -> CoreResult<()> {
        let now = now_ms();
        self.conn.execute(
            "UPDATE operation_item SET status = ?2,
                destination_path = COALESCE(?3, destination_path),
                hash_after = COALESCE(?4, hash_after),
                error = ?5,
                started_at = CASE WHEN ?2 = 'in_progress' THEN ?6 ELSE started_at END,
                finished_at = CASE WHEN ?2 = 'in_progress' THEN NULL ELSE ?6 END
             WHERE id = ?1",
            params![
                item_id,
                enum_str(&status),
                destination.map(|d| d.to_string_lossy().into_owned()),
                hash_after,
                note.map(Note::to_json),
                now
            ],
        )?;
        Ok(())
    }

    pub fn log_event(
        &self,
        plan_id: i64,
        item_id: Option<i64>,
        event: &str,
        detail: Option<&Note>,
    ) -> CoreResult<()> {
        self.conn.execute(
            "INSERT INTO operation_log (plan_id, item_id, at, event, detail) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![plan_id, item_id, now_ms(), event, detail.map(Note::to_json)],
        )?;
        Ok(())
    }

    /// After a crash or forced close: items that were in progress become
    /// `interrupted` (retry continues them), running plans become `partial`.
    pub fn recover_interrupted_runs(&self) -> CoreResult<Vec<RunItem>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {RUN_COLUMNS} FROM operation_item WHERE status = 'in_progress'"
        ))?;
        let items: Vec<RunItem> = stmt
            .query_map([], read_run_item)?
            .collect::<Result<_, _>>()?;
        self.conn.execute(
            "UPDATE operation_item SET status = 'interrupted' WHERE status = 'in_progress'",
            [],
        )?;
        self.conn.execute(
            "UPDATE operation_plan SET state = 'partial' WHERE state IN ('running', 'undoing')",
            [],
        )?;
        Ok(items)
    }

    /// After a move the files no longer belong to the sources: their index
    /// rows are dropped (the plan keeps the paths for undo).
    pub fn forget_media(&mut self, ids: &[i64]) -> CoreResult<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare("DELETE FROM media_item WHERE id = ?1")?;
            for id in ids {
                stmt.execute(params![id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn run_summaries(&self) -> CoreResult<Vec<RunSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT p.id, p.created_at, p.destination_root, p.mode, p.state,
                    COALESCE(SUM(i.status = 'done'), 0), COALESCE(SUM(i.status = 'failed'), 0),
                    COALESCE(SUM(i.status = 'undone'), 0),
                    COALESCE(SUM(i.included = 1 AND i.status IN ('planned', 'interrupted')), 0),
                    COALESCE(SUM(CASE WHEN i.status = 'done' THEN i.expected_size ELSE 0 END), 0)
             FROM operation_plan p LEFT JOIN operation_item i ON i.plan_id = p.id
             WHERE p.state <> 'draft' GROUP BY p.id ORDER BY p.id DESC LIMIT 50",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(RunSummary {
                plan_id: r.get(0)?,
                created_at: r.get(1)?,
                destination_root: r.get(2)?,
                mode: enum_parse(&r.get::<_, String>(3)?).unwrap_or_default(),
                state: r.get(4)?,
                done: u(r.get(5)?),
                failed: u(r.get(6)?),
                undone: u(r.get(7)?),
                pending: u(r.get(8)?),
                bytes_done: u(r.get(9)?),
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Failed items with their reason, for the log.
    pub fn failed_items(&self, plan_id: i64) -> CoreResult<Vec<(String, String, Option<Note>)>> {
        let mut stmt = self.conn.prepare(
            "SELECT source_path, destination_path, error FROM operation_item
             WHERE plan_id = ?1 AND status = 'failed' ORDER BY seq LIMIT 500",
        )?;
        let rows = stmt.query_map(params![plan_id], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get::<_, Option<String>>(2)?.map(|n| Note::parse(&n)),
            ))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}
