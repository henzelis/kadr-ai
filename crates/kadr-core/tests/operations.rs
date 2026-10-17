//! Running plans on real files, including injected faults (spec §18 items
//! 12–18 and stage E "fault injection").

#![allow(clippy::unwrap_used, clippy::expect_used)]
// test helpers panic on purpose
// Some helpers are used only by the Unix-only tests (symlinks, permissions).
#![cfg_attr(not(unix), allow(unused_imports, dead_code))]

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use kadr_core::domain::location::Language;
use kadr_core::domain::operation::{OperationMode, OperationStatus};
use kadr_core::persistence::Db;
use kadr_core::persistence::plans::PlanFilter;
use kadr_core::services::indexer::{Helpers, ScanRequest, run_scan};
use kadr_core::services::operations::{
    Hooks, NoHooks, RunPhase, apply_plan, hash_file, recover_after_crash, undo_plan,
};
use kadr_core::services::planner::{PlanStatus, create_plan};
use kadr_core::settings::AppSettings;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/media")
}

struct Env {
    tmp: tempfile::TempDir,
    lib: PathBuf,
    archive: PathBuf,
    db: Db,
    settings: AppSettings,
}

/// Five photos (one with an AAE companion) in a library, nothing archived.
fn env() -> Env {
    let tmp = tempfile::tempdir().unwrap();
    let lib = tmp.path().join("Фото");
    fs::create_dir_all(lib.join("sub")).unwrap();
    for name in [
        "exif_full.jpg",
        "exif_no_offset.jpg",
        "southwest.jpg",
        "IMG_0001.JPG",
        "IMG_0001.AAE",
    ] {
        fs::copy(fixtures().join(name), lib.join(name)).unwrap();
    }
    fs::copy(
        fixtures().join("no_metadata.png"),
        lib.join("sub/no_metadata.png"),
    )
    .unwrap();
    let archive = tmp.path().join("Архів");
    let mut db = Db::open(&tmp.path().join("index.sqlite3")).unwrap();
    let root = db.add_source_root(&lib.to_string_lossy()).unwrap();
    let settings = AppSettings {
        destination_root: Some(archive.to_string_lossy().into_owned()),
        folder_language: Some(Language::En),
        ..AppSettings::default()
    };
    let req = ScanRequest {
        roots: &[root],
        settings: &settings,
        exclude: vec![],
    };
    run_scan(
        &mut db,
        &req,
        &Helpers::default(),
        None,
        &AtomicBool::new(false),
        &mut |_| {},
    )
    .unwrap();
    Env {
        tmp,
        lib,
        archive,
        db,
        settings,
    }
}

/// Every file under `root` as (path relative to root, bytes).
fn files(root: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        for e in fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(root, &p, out);
            } else {
                let rel = p
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((rel, fs::read(&p).unwrap()));
            }
        }
    }
    let mut out = Vec::new();
    if root.exists() {
        walk(root, root, &mut out);
    }
    out.sort();
    out
}

fn run(e: &mut Env, plan: i64, hooks: &dyn Hooks) -> kadr_core::services::operations::RunProgress {
    apply_plan(&mut e.db, plan, &AtomicBool::new(false), hooks, &mut |_| {}).unwrap()
}

fn plan(e: &mut Env, mode: OperationMode) -> i64 {
    create_plan(&mut e.db, &e.settings, mode).unwrap()
}

#[test]
fn copy_leaves_sources_untouched_and_verifies_every_file() {
    // §18.13
    let mut e = env();
    let before = files(&e.lib);
    let mtime_before = fs::metadata(e.lib.join("exif_full.jpg"))
        .unwrap()
        .modified()
        .unwrap();
    let id = plan(&mut e, OperationMode::Copy);
    let p = run(&mut e, id, &NoHooks);
    assert_eq!((p.phase, p.failed), (RunPhase::Done, 0));
    assert_eq!(p.done, 6); // 5 photos + 1 companion
    assert_eq!(files(&e.lib), before);

    let archived = files(&e.archive);
    let names: Vec<&str> = archived.iter().map(|(n, _)| n.as_str()).collect();
    assert!(
        names.contains(&"2025/08 - August/No location/exif_full.jpg"),
        "{names:?}"
    );
    assert!(
        names.contains(&"2020/02 - February/No location/IMG_0001.AAE"),
        "{names:?}"
    );
    assert!(
        names.contains(&"Unknown date/No location/no_metadata.png"),
        "{names:?}"
    );
    // Same bytes, same modification time; no temporary files left.
    assert_eq!(
        fs::read(e.archive.join("2025/08 - August/No location/exif_full.jpg")).unwrap(),
        fs::read(e.lib.join("exif_full.jpg")).unwrap()
    );
    assert_eq!(
        fs::metadata(e.archive.join("2025/08 - August/No location/exif_full.jpg"))
            .unwrap()
            .modified()
            .unwrap(),
        mtime_before
    );
    assert!(!names.iter().any(|n| n.contains("kadr-tmp")));

    // Running the plan again is refused; a new plan finds everything archived.
    assert!(
        apply_plan(
            &mut e.db,
            id,
            &AtomicBool::new(false),
            &NoHooks,
            &mut |_| {}
        )
        .is_err()
    );
    let again = plan(&mut e, OperationMode::Copy);
    let s = e.db.plan_summary(again).unwrap().unwrap();
    assert_eq!((s.ready, s.already_archived, s.included_files), (0, 5, 0));
}

#[test]
fn move_on_the_same_drive_and_undo() {
    let mut e = env();
    let before = files(&e.lib);
    let id = plan(&mut e, OperationMode::Move);
    let p = run(&mut e, id, &NoHooks);
    assert_eq!((p.phase, p.done), (RunPhase::Done, 6));
    assert!(files(&e.lib).is_empty(), "everything moved out");
    // Moved files are no longer in the library index.
    assert_eq!(e.db.media_count().unwrap(), 0);

    // §18.17 undo puts every file back and removes the folders it created.
    let u = undo_plan(&mut e.db, id, &AtomicBool::new(false), &mut |_| {}).unwrap();
    assert_eq!((u.phase, u.done, u.skipped), (RunPhase::Done, 6, 0));
    assert_eq!(files(&e.lib), before);
    assert!(files(&e.archive).is_empty());
}

struct CrossDevice {
    fail_delete: bool,
}

impl Hooks for CrossDevice {
    fn allow_rename(&self) -> bool {
        false
    }
    fn before_remove_source(&self, _: &Path) -> io::Result<()> {
        if self.fail_delete {
            Err(io::Error::other("drive went away"))
        } else {
            Ok(())
        }
    }
}

#[test]
fn move_between_drives_deletes_originals_only_after_verified_copy() {
    // §18.14
    let mut e = env();
    let before = files(&e.lib);
    let id = plan(&mut e, OperationMode::Move);
    let p = run(&mut e, id, &CrossDevice { fail_delete: false });
    assert_eq!((p.phase, p.done), (RunPhase::Done, 6));
    assert!(files(&e.lib).is_empty());
    let archived: Vec<Vec<u8>> = files(&e.archive).into_iter().map(|(_, b)| b).collect();
    let mut original: Vec<Vec<u8>> = before.into_iter().map(|(_, b)| b).collect();
    let mut archived_sorted = archived.clone();
    original.sort();
    archived_sorted.sort();
    assert_eq!(archived_sorted, original, "every byte arrived");
}

#[test]
fn failed_delete_after_verified_copy_keeps_both_and_never_copies_twice() {
    let mut e = env();
    let before = files(&e.lib);
    let id = plan(&mut e, OperationMode::Move);
    let p = run(&mut e, id, &CrossDevice { fail_delete: true });
    // The files are safely archived; the originals simply stayed.
    assert_eq!((p.phase, p.done, p.failed), (RunPhase::Done, 6, 0));
    assert_eq!(files(&e.lib), before);
    assert_eq!(files(&e.archive).len(), 6);
}

/// Fails writing once `limit` bytes went to the archive (a full disk).
struct DiskFull {
    budget: AtomicU32,
}

impl Hooks for DiskFull {
    // The archive is on another drive, so every file is really written.
    fn allow_rename(&self) -> bool {
        false
    }
    fn on_write(&self, _: &Path, written: u64) -> io::Result<()> {
        let left = self.budget.load(Ordering::Relaxed);
        if written > u64::from(left) {
            return Err(io::Error::new(
                io::ErrorKind::StorageFull,
                "no space left on device",
            ));
        }
        self.budget.fetch_sub(
            u32::try_from(written).unwrap_or(u32::MAX).min(left),
            Ordering::Relaxed,
        );
        Ok(())
    }
}

#[test]
fn full_disk_fails_cleanly_and_retry_finishes() {
    // §18.16: the original is never deleted on failure, and the plan shows
    // the failed items.
    let mut e = env();
    let before = files(&e.lib);
    let id = plan(&mut e, OperationMode::Move);
    let p = run(
        &mut e,
        id,
        &DiskFull {
            budget: AtomicU32::new(0),
        },
    );
    assert_eq!((p.phase, p.done), (RunPhase::Partial, 0));
    assert_eq!(p.failed, 6);
    assert_eq!(files(&e.lib), before, "nothing lost");
    assert!(
        files(&e.archive).is_empty(),
        "no temporary files or placeholders left"
    );
    let failed = e.db.failed_items(id).unwrap();
    assert!(
        failed
            .iter()
            .any(|(_, _, n)| n.as_ref().is_some_and(|n| n.code == "op_disk_full"))
    );
    // The AAE did not travel without its photo.
    assert!(
        failed
            .iter()
            .any(|(src, _, n)| src.ends_with("IMG_0001.AAE")
                && n.as_ref().is_some_and(|n| n.code == "op_main_failed"))
    );

    // Space freed: continuing finishes the plan.
    let p = run(&mut e, id, &NoHooks);
    assert_eq!((p.phase, p.done, p.failed), (RunPhase::Done, 6, 0));
    assert!(files(&e.lib).is_empty());
}

#[test]
fn conflicts_appearing_after_planning_are_not_overwritten() {
    // §18.15
    let mut e = env();
    let id = plan(&mut e, OperationMode::Copy);
    let target = e.archive.join("2025/08 - August/No location/exif_full.jpg");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, b"someone else's file").unwrap();
    let p = run(&mut e, id, &NoHooks);
    assert_eq!((p.phase, p.failed), (RunPhase::Done, 0));
    assert_eq!(fs::read(&target).unwrap(), b"someone else's file");
    let renamed = target.with_file_name("exif_full (1).jpg");
    assert_eq!(
        fs::read(&renamed).unwrap(),
        fs::read(e.lib.join("exif_full.jpg")).unwrap()
    );
}

#[test]
fn changed_source_is_not_copied() {
    let mut e = env();
    let id = plan(&mut e, OperationMode::Move);
    fs::write(e.lib.join("southwest.jpg"), b"edited after planning").unwrap();
    let p = run(&mut e, id, &NoHooks);
    assert_eq!((p.done, p.failed), (5, 1));
    assert_eq!(
        fs::read(e.lib.join("southwest.jpg")).unwrap(),
        b"edited after planning"
    );
    let failed = e.db.failed_items(id).unwrap();
    assert_eq!(failed[0].2.as_ref().unwrap().code, "op_source_changed");
}

#[test]
fn undo_leaves_files_changed_after_the_copy() {
    let mut e = env();
    let id = plan(&mut e, OperationMode::Copy);
    run(&mut e, id, &NoHooks);
    let edited = e.archive.join("2025/08 - August/No location/exif_full.jpg");
    fs::write(&edited, b"retouched in the archive").unwrap();
    let u = undo_plan(&mut e.db, id, &AtomicBool::new(false), &mut |_| {}).unwrap();
    assert_eq!((u.phase, u.done, u.skipped), (RunPhase::Partial, 5, 1));
    assert_eq!(fs::read(&edited).unwrap(), b"retouched in the archive");
    assert_eq!(files(&e.archive).len(), 1);
}

/// Asks to cancel as soon as the first file has been written.
struct CancelAfterFirst<'a>(&'a AtomicBool);

impl Hooks for CancelAfterFirst<'_> {
    fn on_write(&self, _: &Path, _: u64) -> io::Result<()> {
        self.0.store(true, Ordering::Relaxed);
        Ok(())
    }
}

#[test]
fn cancel_and_continue() {
    let mut e = env();
    let id = plan(&mut e, OperationMode::Copy);
    let cancel = AtomicBool::new(false);
    let p = apply_plan(
        &mut e.db,
        id,
        &cancel,
        &CancelAfterFirst(&cancel),
        &mut |_| {},
    )
    .unwrap();
    // The file being copied is finished; nothing after it is started.
    assert_eq!((p.phase, p.done), (RunPhase::Partial, 1));
    assert_eq!(files(&e.archive).len(), 1);
    let p2 = run(&mut e, id, &NoHooks);
    assert_eq!((p2.phase, p2.done, p2.failed), (RunPhase::Done, 5, 0));
    assert_eq!(files(&e.archive).len(), 6);
}

#[test]
fn crash_mid_copy_is_recovered() {
    let mut e = env();
    let id = plan(&mut e, OperationMode::Copy);
    // Simulate a crash: one item stuck in progress with a half-written
    // temporary file and our empty placeholder.
    let page =
        e.db.plan_items(id, PlanFilter::All, "exif_full", 0, 10)
            .unwrap();
    let item = &page.items[0];
    let dest = PathBuf::from(&item.destination);
    fs::create_dir_all(dest.parent().unwrap()).unwrap();
    fs::write(&dest, b"").unwrap();
    let tmp = dest.with_file_name(format!(
        ".{}.kadr-tmp",
        dest.file_name().unwrap().to_string_lossy()
    ));
    fs::write(&tmp, b"half").unwrap();
    e.db.mark_item(item.id, OperationStatus::InProgress, None, None, None)
        .unwrap();
    e.db.set_plan_state(id, "running").unwrap();

    assert_eq!(recover_after_crash(&e.db).unwrap(), 1);
    assert!(!tmp.exists() && !dest.exists());
    assert_eq!(e.db.plan_state(id).unwrap().unwrap().0, "partial");
    let p = run(&mut e, id, &NoHooks);
    assert_eq!((p.phase, p.done), (RunPhase::Done, 6));
    assert_eq!(
        hash_file(&dest).unwrap(),
        hash_file(&e.lib.join("exif_full.jpg")).unwrap()
    );
}

#[cfg(unix)]
#[test]
fn symlink_inside_the_archive_cannot_lead_outside() {
    // §18.18
    let mut e = env();
    let outside = e.tmp.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::create_dir_all(e.archive.join("2025/08 - August")).unwrap();
    std::os::unix::fs::symlink(&outside, e.archive.join("2025/08 - August/No location")).unwrap();
    let id = plan(&mut e, OperationMode::Copy);
    let p = run(&mut e, id, &NoHooks);
    assert!(p.failed >= 1);
    assert!(
        files(&outside).is_empty(),
        "nothing written outside the archive"
    );
    let failed = e.db.failed_items(id).unwrap();
    assert!(
        failed
            .iter()
            .any(|(_, _, n)| n.as_ref().is_some_and(|n| n.code == "op_outside_root"))
    );
}

#[cfg(unix)]
#[test]
fn read_only_archive_fails_without_losing_anything() {
    use std::os::unix::fs::PermissionsExt;
    let mut e = env();
    fs::create_dir_all(&e.archive).unwrap();
    fs::set_permissions(&e.archive, fs::Permissions::from_mode(0o555)).unwrap();
    let before = files(&e.lib);
    let id = plan(&mut e, OperationMode::Move);
    let p = run(&mut e, id, &NoHooks);
    fs::set_permissions(&e.archive, fs::Permissions::from_mode(0o755)).unwrap();
    if p.failed == 0 {
        eprintln!("NOTICE: running as root, permissions are not enforced — test skipped");
        return;
    }
    assert_eq!(files(&e.lib), before);
    assert_eq!(
        e.db.plan_items(id, PlanFilter::All, "", 0, 100)
            .unwrap()
            .items
            .iter()
            .filter(|r| r.status == PlanStatus::Ready)
            .count(),
        5
    );
}
