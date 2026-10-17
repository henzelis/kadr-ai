//! Running a sorting plan (spec §11, Apply) and undoing it. Safety rules:
//! - nothing is ever overwritten: the final name is reserved with an
//!   exclusive create before any data is written;
//! - data goes to a temporary file, is read back and compared by SHA-256
//!   with the source, and only then gets its final name;
//! - a move deletes the original only after a verified copy (or is a
//!   same-volume rename);
//! - the source must be unchanged since planning, and the destination must
//!   stay inside the archive folder (no `..`, symlink or junction escapes);
//! - every step is journalled, so a crash leaves a plan that can be
//!   continued, retried or undone.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::domain::note::Note;
use crate::domain::operation::{OperationMode, OperationStatus};
use crate::errors::{CoreError, CoreResult};
use crate::persistence::Db;
use crate::persistence::plans::RunItem;

const BUFFER: usize = 1 << 20;
const TMP_SUFFIX: &str = ".kadr-tmp";

/// Points where tests inject failures (disk full, unplugged drive...).
/// Production code uses [`NoHooks`].
pub trait Hooks: Sync {
    /// Called after each chunk written to a temporary file.
    fn on_write(&self, _destination: &Path, _written: u64) -> io::Result<()> {
        Ok(())
    }
    /// Called right before the verified file gets its final name.
    fn before_finalize(&self, _destination: &Path) -> io::Result<()> {
        Ok(())
    }
    /// Called right before a moved file's original is deleted.
    fn before_remove_source(&self, _source: &Path) -> io::Result<()> {
        Ok(())
    }
    /// `false` makes a move behave as if source and archive were on
    /// different drives (copy, verify, delete) — for tests.
    fn allow_rename(&self) -> bool {
        true
    }
}

pub struct NoHooks;
impl Hooks for NoHooks {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunPhase {
    Running,
    Undoing,
    Done,
    /// Stopped by the user, or some files failed: can be continued.
    Partial,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunProgress {
    pub plan_id: i64,
    pub mode: OperationMode,
    pub phase: RunPhase,
    pub total: u64,
    pub done: u64,
    pub failed: u64,
    pub skipped: u64,
    pub bytes_total: u64,
    pub bytes_done: u64,
    pub current: Option<String>,
}

fn mtime_ms(meta: &fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_millis()).ok())
        .unwrap_or(0)
}

pub fn hash_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0_u8; BUFFER];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// `IMG_1.HEIC`, 2 → `IMG_1 (2).HEIC`
fn with_suffix(path: &Path, n: u32) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name.as_str(), ""),
    };
    path.with_file_name(format!("{stem} ({n}){ext}"))
}

/// Claims a destination name that nobody else has: creates an empty file
/// exclusively (fails if anything exists, even created a moment ago).
/// Returns the claimed path and whether it differs from the planned one.
fn reserve(planned: &Path) -> io::Result<(PathBuf, bool)> {
    for n in 0..10_000 {
        let candidate = if n == 0 {
            planned.to_path_buf()
        } else {
            with_suffix(planned, n)
        };
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(_) => return Ok((candidate, n > 0)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::other("no free file name"))
}

fn tmp_path(destination: &Path) -> PathBuf {
    let name = destination
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    destination.with_file_name(format!(".{name}{TMP_SUFFIX}"))
}

/// Removes our empty placeholder (never a file with content).
fn release(reserved: &Path) {
    if fs::metadata(reserved).is_ok_and(|m| m.len() == 0) {
        let _ = fs::remove_file(reserved);
    }
}

#[derive(Debug)]
enum OpError {
    SourceChanged,
    VerifyFailed,
    Io(io::Error),
}

impl From<io::Error> for OpError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl OpError {
    fn note(&self) -> Note {
        match self {
            Self::SourceChanged => Note::new("op_source_changed"),
            Self::VerifyFailed => Note::new("op_verify_failed"),
            Self::Io(e) => match e.kind() {
                io::ErrorKind::StorageFull => Note::new("op_disk_full"),
                io::ErrorKind::PermissionDenied => Note::new("op_permission").arg("detail", e),
                io::ErrorKind::NotFound => Note::new("op_not_found").arg("detail", e),
                _ => Note::new("op_io").arg("detail", e),
            },
        }
    }
}

/// Copies `source` to the reserved `destination` through a temporary file,
/// verifies the copy by reading it back, keeps the modification time.
/// Returns the SHA-256 of the copy.
fn copy_verified(
    source: &Path,
    destination: &Path,
    expected: Option<&str>,
    hooks: &dyn Hooks,
) -> Result<String, OpError> {
    let tmp = tmp_path(destination);
    let result = (|| -> Result<String, OpError> {
        let src_meta = fs::metadata(source)?;
        let mut input = File::open(source)?;
        let mut output = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0_u8; BUFFER];
        let mut written: u64 = 0;
        loop {
            let n = input.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            output.write_all(&buf[..n])?;
            written += n as u64;
            hooks.on_write(destination, written)?;
        }
        output.sync_all()?;
        if let Ok(modified) = src_meta.modified() {
            output.set_modified(modified)?;
        }
        drop(output);
        let source_hash = hex::encode(hasher.finalize());
        if expected.is_some_and(|e| e != source_hash) || written != src_meta.len() {
            return Err(OpError::SourceChanged);
        }
        if hash_file(&tmp)? != source_hash {
            return Err(OpError::VerifyFailed);
        }
        hooks.before_finalize(destination)?;
        // Replaces our own empty placeholder, nothing else.
        fs::rename(&tmp, destination)?;
        Ok(source_hash)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Keeps `path` inside `root` after resolving `..`, symlinks and junctions
/// of the folders that exist. Creates the folders.
fn checked_dir(dir: &Path, root_canonical: &Path) -> io::Result<bool> {
    fs::create_dir_all(dir)?;
    Ok(dunce(&fs::canonicalize(dir)?).starts_with(root_canonical))
}

/// `canonicalize` on Windows gives `\\?\C:\...`; compare in plain form.
fn dunce(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => p.to_path_buf(),
    }
}

/// Moves or copies one item. Returns (final destination, hash, note).
fn run_one(
    item: &RunItem,
    mode: OperationMode,
    root_canonical: &Path,
    hooks: &dyn Hooks,
) -> Result<(PathBuf, String, Option<Note>), Note> {
    let meta = match fs::metadata(&item.source) {
        Ok(m) => m,
        Err(_) => {
            // A move that finished right before a crash: the file is already
            // at its destination, intact.
            if mode == OperationMode::Move
                && let Ok(d) = fs::metadata(&item.destination)
                && d.len() == item.expected_size
                && let (Some(expected), Ok(found)) = (&item.sha256, hash_file(&item.destination))
                && *expected == found
            {
                return Ok((item.destination.clone(), found, None));
            }
            return Err(Note::new("op_source_missing"));
        }
    };
    if meta.len() != item.expected_size || mtime_ms(&meta) != item.expected_mtime {
        return Err(Note::new("op_source_changed"));
    }
    let dir = item
        .destination
        .parent()
        .ok_or_else(|| Note::new("op_outside_root"))?;
    match checked_dir(dir, root_canonical) {
        Ok(true) => {}
        Ok(false) => return Err(Note::new("op_outside_root")),
        Err(e) => return Err(OpError::Io(e).note()),
    }
    let (destination, renamed) = reserve(&item.destination).map_err(|e| OpError::Io(e).note())?;
    let note = renamed.then(|| {
        Note::new("op_renamed").arg(
            "name",
            destination
                .file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default(),
        )
    });
    let result = match mode {
        OperationMode::Copy => {
            copy_verified(&item.source, &destination, item.sha256.as_deref(), hooks)
                .map(|h| (h, None))
        }
        OperationMode::Move => {
            move_verified(&item.source, &destination, item.sha256.as_deref(), hooks)
        }
    };
    match result {
        Ok((hash, kept)) => Ok((destination, hash, kept.or(note))),
        Err(e) => {
            release(&destination);
            Err(e.note())
        }
    }
}

/// Same volume: rename (atomic, nothing copied). Otherwise a verified copy,
/// then the original is deleted. If only the deletion fails, the file is
/// safely in the archive: that is a success with a note, never a reason to
/// copy it again.
fn move_verified(
    source: &Path,
    destination: &Path,
    expected: Option<&str>,
    hooks: &dyn Hooks,
) -> Result<(String, Option<Note>), OpError> {
    if hooks.allow_rename() {
        match fs::rename(source, destination) {
            Ok(()) => {
                let hash = match expected {
                    Some(h) => h.to_owned(),
                    None => hash_file(destination)?,
                };
                return Ok((hash, None));
            }
            // Different drive (or a file system that refuses renames): copy.
            Err(e)
                if e.kind() != io::ErrorKind::NotFound
                    && e.kind() != io::ErrorKind::PermissionDenied => {}
            Err(e) => return Err(OpError::Io(e)),
        }
    }
    let hash = copy_verified(source, destination, expected, hooks)?;
    let removed = hooks
        .before_remove_source(source)
        .and_then(|()| fs::remove_file(source));
    Ok((
        hash,
        removed
            .err()
            .map(|e| Note::new("op_source_kept").arg("detail", e)),
    ))
}

/// Empty folders the run created are removed again on undo, up to the root.
fn remove_empty_dirs(mut dir: Option<&Path>, root: &Path) {
    while let Some(d) = dir {
        if d == root || !d.starts_with(root) || fs::remove_dir(d).is_err() {
            break;
        }
        dir = d.parent();
    }
}

fn throttle(last: &mut SystemTime) -> bool {
    let now = SystemTime::now();
    if now.duration_since(*last).unwrap_or_default() >= Duration::from_millis(120) {
        *last = now;
        true
    } else {
        false
    }
}

/// Runs (or continues) a plan. Items that already succeeded are not
/// touched again; failed and interrupted ones are retried.
pub fn apply_plan(
    db: &mut Db,
    plan_id: i64,
    cancel: &AtomicBool,
    hooks: &dyn Hooks,
    on_progress: &mut dyn FnMut(&RunProgress),
) -> CoreResult<RunProgress> {
    let (state, mode, root) = db.plan_state(plan_id)?.ok_or(CoreError::InvalidValue {
        field: "plan",
        reason: "no such plan".to_owned(),
    })?;
    if !matches!(state.as_str(), "draft" | "partial") {
        return Err(CoreError::InvalidValue {
            field: "plan",
            reason: format!("plan is {state}"),
        });
    }
    let root = PathBuf::from(root);
    fs::create_dir_all(&root)?;
    let root_canonical = dunce(&fs::canonicalize(&root)?);
    let items = db.pending_run_items(plan_id)?;
    db.set_plan_state(plan_id, "running")?;
    db.log_event(
        plan_id,
        None,
        if state == "draft" {
            "apply_start"
        } else {
            "apply_continue"
        },
        None,
    )?;

    let mut p = RunProgress {
        plan_id,
        mode,
        phase: RunPhase::Running,
        total: items.len() as u64,
        done: 0,
        failed: 0,
        skipped: 0,
        bytes_total: items.iter().map(|i| i.expected_size).sum(),
        bytes_done: 0,
        current: None,
    };
    on_progress(&p);
    let mut failed_mains: std::collections::HashSet<i64> = std::collections::HashSet::new();
    let mut moved_media = Vec::new();
    let mut last = UNIX_EPOCH;
    for item in &items {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        p.current = item
            .source
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
        if throttle(&mut last) {
            on_progress(&p);
        }
        // A companion never goes anywhere without its main file.
        if item
            .parent_item_id
            .is_some_and(|parent| failed_mains.contains(&parent))
        {
            let note = Note::new("op_main_failed");
            db.mark_item(item.id, OperationStatus::Failed, None, None, Some(&note))?;
            p.failed += 1;
            continue;
        }
        db.mark_item(item.id, OperationStatus::InProgress, None, None, None)?;
        match run_one(item, mode, &root_canonical, hooks) {
            Ok((destination, hash, note)) => {
                db.mark_item(
                    item.id,
                    OperationStatus::Done,
                    Some(&destination),
                    Some(&hash),
                    note.as_ref(),
                )?;
                db.log_event(plan_id, Some(item.id), "done", note.as_ref())?;
                p.done += 1;
                p.bytes_done += item.expected_size;
                if mode == OperationMode::Move
                    && let Some(m) = item.media_id
                {
                    moved_media.push(m);
                }
            }
            Err(note) => {
                db.mark_item(item.id, OperationStatus::Failed, None, None, Some(&note))?;
                db.log_event(plan_id, Some(item.id), "failed", Some(&note))?;
                p.failed += 1;
                if item.parent_item_id.is_none() {
                    failed_mains.insert(item.id);
                }
            }
        }
    }
    db.forget_media(&moved_media)?;
    let cancelled = cancel.load(Ordering::Relaxed);
    let complete = !cancelled && p.failed == 0;
    db.set_plan_state(plan_id, if complete { "done" } else { "partial" })?;
    db.log_event(
        plan_id,
        None,
        if cancelled {
            "apply_cancelled"
        } else {
            "apply_end"
        },
        None,
    )?;
    p.phase = if complete {
        RunPhase::Done
    } else {
        RunPhase::Partial
    };
    p.current = None;
    on_progress(&p);
    Ok(p)
}

/// Undoes what a plan did, newest first. A file is put back only if it is
/// unchanged since the run and its original place is free; otherwise it is
/// left alone and the reason is recorded.
pub fn undo_plan(
    db: &mut Db,
    plan_id: i64,
    cancel: &AtomicBool,
    on_progress: &mut dyn FnMut(&RunProgress),
) -> CoreResult<RunProgress> {
    let (state, mode, root) = db.plan_state(plan_id)?.ok_or(CoreError::InvalidValue {
        field: "plan",
        reason: "no such plan".to_owned(),
    })?;
    if !matches!(state.as_str(), "done" | "partial") {
        return Err(CoreError::InvalidValue {
            field: "plan",
            reason: format!("plan is {state}"),
        });
    }
    let root = PathBuf::from(root);
    let items = db.done_run_items(plan_id)?;
    db.set_plan_state(plan_id, "undoing")?;
    db.log_event(plan_id, None, "undo_start", None)?;
    let mut p = RunProgress {
        plan_id,
        mode,
        phase: RunPhase::Undoing,
        total: items.len() as u64,
        done: 0,
        failed: 0,
        skipped: 0,
        bytes_total: items.iter().map(|i| i.expected_size).sum(),
        bytes_done: 0,
        current: None,
    };
    on_progress(&p);
    let mut last = UNIX_EPOCH;
    for item in &items {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        p.current = item
            .destination
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
        if throttle(&mut last) {
            on_progress(&p);
        }
        let intact = hash_file(&item.destination)
            .ok()
            .is_some_and(|h| Some(h) == item.hash_after);
        let outcome: Result<(), Note> = if !intact {
            Err(Note::new("undo_changed"))
        } else {
            match mode {
                OperationMode::Copy => {
                    fs::remove_file(&item.destination).map_err(|e| OpError::Io(e).note())
                }
                OperationMode::Move => {
                    if fs::symlink_metadata(&item.source).is_ok() {
                        Err(Note::new("undo_source_taken"))
                    } else {
                        let back = item
                            .source
                            .parent()
                            .map_or(Ok(()), fs::create_dir_all)
                            .and_then(|()| reserve(&item.source).map(|_| ()));
                        match back {
                            Err(e) => Err(OpError::Io(e).note()),
                            Ok(()) => move_verified(
                                &item.destination,
                                &item.source,
                                item.hash_after.as_deref(),
                                &NoHooks,
                            )
                            .map(|_| ())
                            .map_err(|e| {
                                release(&item.source);
                                e.note()
                            }),
                        }
                    }
                }
            }
        };
        match outcome {
            Ok(()) => {
                remove_empty_dirs(item.destination.parent(), &root);
                db.mark_item(item.id, OperationStatus::Undone, None, None, None)?;
                db.log_event(plan_id, Some(item.id), "undone", None)?;
                p.done += 1;
                p.bytes_done += item.expected_size;
            }
            Err(note) => {
                db.log_event(plan_id, Some(item.id), "undo_skipped", Some(&note))?;
                db.mark_item(item.id, OperationStatus::Done, None, None, Some(&note))?;
                p.skipped += 1;
            }
        }
    }
    let complete = !cancel.load(Ordering::Relaxed) && p.skipped == 0;
    db.set_plan_state(plan_id, if complete { "undone" } else { "partial" })?;
    db.log_event(plan_id, None, "undo_end", None)?;
    p.phase = if complete {
        RunPhase::Done
    } else {
        RunPhase::Partial
    };
    p.current = None;
    on_progress(&p);
    Ok(p)
}

/// After a crash: marks interrupted items and removes our temporary files
/// and empty placeholders they may have left behind.
pub fn recover_after_crash(db: &Db) -> CoreResult<usize> {
    let items = db.recover_interrupted_runs()?;
    for item in &items {
        let _ = fs::remove_file(tmp_path(&item.destination));
        if fs::metadata(&item.source).is_ok() {
            release(&item.destination);
        }
    }
    Ok(items.len())
}
