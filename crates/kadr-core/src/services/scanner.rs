//! Recursive directory walk. Produces candidate files with their file-system
//! attributes; reading metadata happens later in the indexer.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::UNIX_EPOCH;

use walkdir::WalkDir;

use super::detect::{kind_from_extension, sniff_file};
use crate::domain::media_item::MediaKind;

/// Directory names that never hold the user's media.
const SYSTEM_DIRS: &[&str] = &[
    "$recycle.bin",
    "system volume information",
    ".trash",
    ".trashes",
    ".spotlight-v100",
    ".fseventsd",
    ".documentrevisions-v100",
    ".temporaryitems",
    "@eadir",
    "#recycle",
    ".thumbnails",
    "lost+found",
    ".git",
    "node_modules",
];

#[derive(Debug, Clone)]
pub struct WalkOptions {
    pub follow_symlinks: bool,
    pub skip_system_dirs: bool,
    /// Directories never entered, e.g. the archive destination when it lies
    /// inside a source folder.
    pub exclude: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FoundFile {
    pub path: PathBuf,
    pub size: u64,
    /// Modification time, Unix milliseconds (0 if the OS does not report it).
    pub mtime_ms: i64,
    /// Lower-case extension without the dot; empty when there is none.
    pub extension: String,
    pub kind: MediaKind,
    /// `device:inode` on Unix; `None` where no stable identity is available.
    pub file_identity: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    SystemDir,
    Symlink,
    Excluded,
    NotMedia,
}

#[derive(Debug)]
pub enum WalkEvent {
    File(FoundFile),
    Skipped {
        path: PathBuf,
        reason: SkipReason,
    },
    /// One entry could not be read; the walk goes on.
    Error {
        path: PathBuf,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalkOutcome {
    Completed,
    Cancelled,
}

/// Walks `root`, calling `on_event` for every file, skip and error.
/// Errors on single entries never stop the walk; only an unreadable root does.
pub fn walk(
    root: &Path,
    options: &WalkOptions,
    cancel: &AtomicBool,
    mut on_event: impl FnMut(WalkEvent),
) -> std::io::Result<WalkOutcome> {
    // Fail early and clearly when the root itself is missing or unreadable.
    std::fs::read_dir(root)?;

    let mut walker = WalkDir::new(root)
        .follow_links(options.follow_symlinks)
        .sort_by_file_name()
        .into_iter();

    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(WalkOutcome::Cancelled);
        }
        let Some(entry) = walker.next() else {
            return Ok(WalkOutcome::Completed);
        };
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                // Includes loops detected while following links.
                let path = err
                    .path()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| root.to_path_buf());
                on_event(WalkEvent::Error {
                    path,
                    message: err.to_string(),
                });
                continue;
            }
        };
        let path = entry.path();
        let file_type = entry.file_type();

        if entry.depth() > 0 && file_type.is_dir() {
            let skip = if options.exclude.iter().any(|e| path.starts_with(e)) {
                Some(SkipReason::Excluded)
            } else if options.skip_system_dirs && is_system_dir(entry.file_name()) {
                Some(SkipReason::SystemDir)
            } else {
                None
            };
            if let Some(reason) = skip {
                walker.skip_current_dir();
                on_event(WalkEvent::Skipped {
                    path: path.to_path_buf(),
                    reason,
                });
            }
            continue;
        }
        if file_type.is_symlink() {
            // Only reached when links are not followed (junctions count too).
            on_event(WalkEvent::Skipped {
                path: path.to_path_buf(),
                reason: SkipReason::Symlink,
            });
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        if is_system_file(entry.file_name()) {
            continue;
        }

        let extension = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let mut kind = kind_from_extension(&extension);
        if kind == MediaKind::Unsupported && extension.is_empty() {
            // Files without an extension (camera dumps, recovered files) are
            // checked by signature; everything else unknown is skipped cheaply.
            if let Ok(Some(sniffed)) = sniff_file(path) {
                kind = sniffed.kind;
            }
        }
        if kind == MediaKind::Unsupported {
            on_event(WalkEvent::Skipped {
                path: path.to_path_buf(),
                reason: SkipReason::NotMedia,
            });
            continue;
        }

        match entry.metadata() {
            Ok(meta) => {
                let mtime_ms = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .and_then(|d| i64::try_from(d.as_millis()).ok())
                    .unwrap_or(0);
                on_event(WalkEvent::File(FoundFile {
                    path: path.to_path_buf(),
                    size: meta.len(),
                    mtime_ms,
                    extension,
                    kind,
                    file_identity: file_identity(&meta),
                }));
            }
            Err(err) => on_event(WalkEvent::Error {
                path: path.to_path_buf(),
                message: err.to_string(),
            }),
        }
    }
}

fn is_system_dir(name: &std::ffi::OsStr) -> bool {
    let name = name.to_string_lossy().to_lowercase();
    SYSTEM_DIRS.contains(&name.as_str()) || name.starts_with(".trash-")
}

/// AppleDouble `._IMG_0001.JPG` files carry a media extension but no media.
fn is_system_file(name: &std::ffi::OsStr) -> bool {
    let name = name.to_string_lossy();
    name.starts_with("._")
        || name.eq_ignore_ascii_case("thumbs.db")
        || name.eq_ignore_ascii_case("desktop.ini")
        || name == ".DS_Store"
}

#[cfg(unix)]
fn file_identity(meta: &std::fs::Metadata) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    Some(format!("{}:{}", meta.dev(), meta.ino()))
}

#[cfg(not(unix))]
fn file_identity(_meta: &std::fs::Metadata) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn options() -> WalkOptions {
        WalkOptions {
            follow_symlinks: false,
            skip_system_dirs: true,
            exclude: vec![],
        }
    }

    fn collect(
        root: &Path,
        opts: &WalkOptions,
    ) -> (Vec<FoundFile>, Vec<(PathBuf, SkipReason)>, usize) {
        let (mut files, mut skipped, mut errors) = (vec![], vec![], 0);
        let cancel = AtomicBool::new(false);
        walk(root, opts, &cancel, |e| match e {
            WalkEvent::File(f) => files.push(f),
            WalkEvent::Skipped { path, reason } => skipped.push((path, reason)),
            WalkEvent::Error { .. } => errors += 1,
        })
        .unwrap();
        (files, skipped, errors)
    }

    fn touch(path: &Path, bytes: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn finds_nested_media_and_skips_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        touch(&root.join("a/b/c/IMG_1.JPG"), b"\xFF\xD8\xFF\xE0");
        touch(&root.join("a/clip.MOV"), b"\0\0\0\x14ftypqt  ");
        touch(&root.join("a/notes.txt"), b"hello");
        touch(&root.join("a/._IMG_1.JPG"), b"apple double");
        touch(&root.join("$RECYCLE.BIN/old.jpg"), b"\xFF\xD8\xFF");
        touch(&root.join("noext_photo"), b"\xFF\xD8\xFF\xE0");

        let (files, skipped, errors) = collect(root, &options());
        let mut names: Vec<_> = files
            .iter()
            .map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["IMG_1.JPG", "clip.MOV", "noext_photo"]);
        assert_eq!(errors, 0);
        assert!(skipped.iter().any(|(_, r)| *r == SkipReason::SystemDir));
        assert!(
            skipped
                .iter()
                .any(|(p, r)| *r == SkipReason::NotMedia && p.ends_with("notes.txt"))
        );
        let jpg = files.iter().find(|f| f.extension == "jpg").unwrap();
        assert_eq!((jpg.kind, jpg.size), (MediaKind::Photo, 4));
        let noext = files.iter().find(|f| f.extension.is_empty()).unwrap();
        assert_eq!(noext.kind, MediaKind::Photo);
    }

    #[test]
    fn excluded_directory_is_not_entered() {
        let dir = tempfile::tempdir().unwrap();
        touch(&dir.path().join("archive/2025/x.jpg"), b"\xFF\xD8\xFF");
        touch(&dir.path().join("y.jpg"), b"\xFF\xD8\xFF");
        let opts = WalkOptions {
            exclude: vec![dir.path().join("archive")],
            ..options()
        };
        let (files, skipped, _) = collect(dir.path(), &opts);
        assert_eq!(files.len(), 1);
        assert!(skipped.iter().any(|(_, r)| *r == SkipReason::Excluded));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_loops_do_not_hang_or_duplicate() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        touch(&root.join("a/x.jpg"), b"\xFF\xD8\xFF");
        std::os::unix::fs::symlink(root, root.join("a/loop")).unwrap();

        let (files, skipped, _) = collect(root, &options());
        assert_eq!(files.len(), 1);
        assert!(skipped.iter().any(|(_, r)| *r == SkipReason::Symlink));

        // Following links: the loop is detected and reported, not walked forever.
        let follow = WalkOptions {
            follow_symlinks: true,
            ..options()
        };
        let (files, _, errors) = collect(root, &follow);
        assert_eq!(files.len(), 1);
        assert_eq!(errors, 1);
    }

    #[test]
    fn missing_root_is_an_error_and_cancel_stops_early() {
        let cancel = AtomicBool::new(false);
        assert!(
            walk(
                Path::new("/definitely/not/here"),
                &options(),
                &cancel,
                |_| {}
            )
            .is_err()
        );

        let dir = tempfile::tempdir().unwrap();
        touch(&dir.path().join("x.jpg"), b"\xFF\xD8\xFF");
        cancel.store(true, Ordering::Relaxed);
        let outcome = walk(dir.path(), &options(), &cancel, |_| {
            panic!("no events after cancel")
        })
        .unwrap();
        assert_eq!(outcome, WalkOutcome::Cancelled);
    }
}
