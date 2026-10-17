//! Indexing pipeline: walk the source roots, re-read only new or changed
//! files, read metadata and hashes in parallel with bounded workers, write
//! results in transactions, and report progress. Cancelling leaves every row
//! that was written complete and consistent.

use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::detect::{extension_matches, resolve_kind, sniff_file};
use super::metadata::exiftool::ExifTool;
use super::metadata::ffprobe::FfProbe;
use super::metadata::native_image::NativeExif;
use super::metadata::normalize::{NormalizeContext, Normalized, normalize};
use super::metadata::{MetadataProvider, RawMetadata, ReadError};
use super::place::{self, PlaceInputs, ReverseGeocoder};
use super::scanner::{self, FoundFile, SkipReason, WalkEvent, WalkOptions, WalkOutcome};
use crate::domain::media_item::{MediaKind, MetadataStatus};
use crate::errors::CoreResult;
use crate::persistence::Db;
use crate::persistence::media::{
    DerivedUpdate, Existing, IndexedFile, SourceRoot, normalized_path, now_ms,
};
use crate::settings::AppSettings;

/// Bump when normalization changes in a way that needs re-reading files.
const PIPELINE_VERSION: u32 = 2;
/// Files handed to one worker at a time (one ExifTool process per slice).
const FILES_PER_WORKER: usize = 24;
const HASH_BUFFER: usize = 1 << 20;

/// External helpers found at startup; any may be missing.
#[derive(Default)]
pub struct Helpers {
    pub exiftool: Option<ExifTool>,
    pub ffprobe: Option<FfProbe>,
    /// Used for thumbnails (video frames, HEIC, AVIF).
    pub ffmpeg: Option<PathBuf>,
}

impl Helpers {
    /// Probes the helpers (Settings path, bundled copy, PATH) and keeps the
    /// ones that answer.
    pub fn detect(
        settings: &AppSettings,
        bundled: Option<&std::path::Path>,
    ) -> (Self, Vec<super::tools::ToolStatus>) {
        use super::tools::{Tool, probe};
        let exif = probe(Tool::Exiftool, settings.exiftool_path.as_deref(), bundled);
        let ff = probe(Tool::Ffprobe, settings.ffprobe_path.as_deref(), bundled);
        // FFmpeg lives next to the chosen FFprobe when that one was set by hand.
        let ffmpeg_custom = settings.ffprobe_path.as_deref().and_then(|p| {
            let name = if cfg!(windows) {
                "ffmpeg.exe"
            } else {
                "ffmpeg"
            };
            std::path::Path::new(p.trim())
                .parent()
                .map(|d| d.join(name).to_string_lossy().into_owned())
        });
        let mpeg = probe(Tool::Ffmpeg, ffmpeg_custom.as_deref(), bundled);
        let helpers = Self {
            exiftool: exif
                .version()
                .map(|v| ExifTool::new(PathBuf::from(&exif.program), v)),
            ffprobe: ff
                .version()
                .map(|v| FfProbe::new(PathBuf::from(&ff.program), v)),
            ffmpeg: mpeg.version().map(|_| PathBuf::from(&mpeg.program)),
        };
        (helpers, vec![exif, ff, mpeg])
    }

    fn provider_key(&self, hashes: bool) -> String {
        format!(
            "{}|{}|p{PIPELINE_VERSION}|h{}",
            self.exiftool
                .as_ref()
                .map_or(super::metadata::native_image::ID, |e| e.id()),
            self.ffprobe.as_ref().map_or("-", |f| f.id()),
            u8::from(hashes)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanPhase {
    Walking,
    Reading,
    Done,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanProgress {
    pub job_id: i64,
    pub phase: ScanPhase,
    pub root: Option<String>,
    pub found: u64,
    pub processed: u64,
    pub unchanged: u64,
    pub skipped: u64,
    pub errors: u64,
    /// Files still to read in the current root.
    pub pending: u64,
    pub current: Option<String>,
}

struct WorkerOut {
    found: FoundFile,
    sniffed_format: Option<&'static str>,
    kind: MediaKind,
    meta: Result<RawMetadata, ReadError>,
    meta_partial: bool,
    sha256: Result<Option<String>, String>,
}

fn sha256_file(path: &PathBuf, cancel: &AtomicBool) -> Result<Option<String>, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0_u8; HASH_BUFFER];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(Some(hex::encode(hasher.finalize())))
}

/// Reads metadata and hashes for one slice of files.
fn work(
    files: Vec<FoundFile>,
    helpers: &Helpers,
    hashes: bool,
    cancel: &AtomicBool,
) -> Vec<WorkerOut> {
    let sniffed: Vec<_> = files
        .iter()
        .map(|f| sniff_file(&f.path).ok().flatten())
        .collect();
    let kinds: Vec<MediaKind> = files
        .iter()
        .zip(&sniffed)
        .map(|(f, s)| resolve_kind(f.kind, *s).0)
        .collect();

    // ExifTool (or the built-in reader) for everything except sidecars.
    let readable: Vec<PathBuf> = files
        .iter()
        .zip(&kinds)
        .filter(|(_, k)| **k != MediaKind::Sidecar)
        .map(|(f, _)| f.path.clone())
        .collect();
    let mut primary = match &helpers.exiftool {
        Some(tool) => tool.read_batch(&readable),
        None => readable
            .iter()
            .zip(kinds.iter().filter(|k| **k != MediaKind::Sidecar))
            .map(|(p, k)| match k {
                MediaKind::Video => Err(ReadError::HelperMissing),
                _ => NativeExif
                    .read_batch(std::slice::from_ref(p))
                    .pop()
                    .unwrap_or(Err(ReadError::HelperMissing)),
            })
            .collect(),
    }
    .into_iter();

    files
        .into_iter()
        .zip(sniffed)
        .zip(kinds)
        .map(|((found, sniffed), kind)| {
            let (meta, meta_partial) = if kind == MediaKind::Sidecar {
                (Ok(RawMetadata::default()), false)
            } else {
                let mut first = primary.next().unwrap_or(Err(ReadError::HelperMissing));
                let mut partial = false;
                if kind == MediaKind::Video
                    && let Some(ff) = &helpers.ffprobe
                {
                    let second = ff.read_batch(std::slice::from_ref(&found.path)).pop();
                    match (&mut first, second) {
                        (Ok(a), Some(Ok(b))) => a.merge(b),
                        (Err(_), Some(Ok(b))) => {
                            partial = helpers.exiftool.is_some();
                            first = Ok(b);
                        }
                        (Ok(_), Some(Err(_))) => partial = true,
                        _ => {}
                    }
                }
                (first, partial)
            };
            let sha256 = if hashes && kind != MediaKind::Sidecar {
                sha256_file(&found.path, cancel)
            } else {
                Ok(None)
            };
            WorkerOut {
                found,
                sniffed_format: sniffed.map(|s| s.format),
                kind,
                meta,
                meta_partial,
                sha256,
            }
        })
        .collect()
}

fn friendly_error(err: &ReadError) -> &'static str {
    match err {
        ReadError::HelperMissing => {
            "Немає інструмента для читання метаданих цього формату. Перевстановіть Kadr, щоб відновити вбудовані інструменти."
        }
        ReadError::Unreadable(_) => {
            "Не вдалося прочитати метадані: файл пошкоджений або формат не підтримується."
        }
        ReadError::HelperFailed(_) => "Інструмент метаданих завершився з помилкою або не відповів.",
    }
}

/// Everything derived from metadata + settings; shared by scanning and
/// re-normalization so both always agree.
pub struct Derivation {
    pub normalized: Normalized,
    pub place: place::ResolvedPlace,
}

/// File facts that date and place derivation depend on.
pub struct FileFacts<'a> {
    pub kind: MediaKind,
    pub mtime_ms: i64,
    pub sniffed_format: Option<&'a str>,
    pub manual_place: Option<&'a str>,
}

pub fn derive(
    raw: &RawMetadata,
    facts: &FileFacts<'_>,
    settings: &AppSettings,
    ctx: &NormalizeContext<'_>,
    geocoder: Option<&dyn ReverseGeocoder>,
) -> Derivation {
    let normalized = normalize(raw, facts.kind, facts.mtime_ms, facts.sniffed_format, ctx);
    let place = place::resolve(
        &PlaceInputs {
            manual: facts.manual_place,
            text: &normalized.place_text,
            gps: normalized.gps,
            granularity: settings.place_granularity,
            coordinates_as_place: settings.coordinates_as_place,
        },
        if settings.offline_geocoding {
            geocoder
        } else {
            None
        },
    );
    Derivation { normalized, place }
}

#[allow(clippy::too_many_arguments)]
fn to_indexed(
    out: WorkerOut,
    root_id: i64,
    provider_key: &str,
    manual_place: Option<&str>,
    settings: &AppSettings,
    ctx: &NormalizeContext<'_>,
    geocoder: Option<&dyn ReverseGeocoder>,
) -> (IndexedFile, Option<String>) {
    let f = &out.found;
    let mut warnings = Vec::new();
    if let Some(format) = out.sniffed_format
        && !f.extension.is_empty()
        && !extension_matches(&f.extension, format)
    {
        warnings.push(format!(
            "Розширення .{} не відповідає вмісту файла ({format})",
            f.extension
        ));
    }
    let (raw, status, last_error) = match out.meta {
        Ok(raw) => {
            let partial = out.meta_partial || !raw.warnings.is_empty();
            warnings.extend(raw.warnings.iter().cloned());
            let status = if out.kind == MediaKind::Sidecar {
                MetadataStatus::NotRead
            } else if partial {
                MetadataStatus::Partial
            } else {
                MetadataStatus::Read
            };
            (raw, status, None)
        }
        Err(err) => {
            let status = if err == ReadError::HelperMissing {
                MetadataStatus::Unavailable
            } else {
                MetadataStatus::Failed
            };
            if let ReadError::Unreadable(d) | ReadError::HelperFailed(d) = &err {
                warnings.push(d.clone());
            }
            (
                RawMetadata::default(),
                status,
                Some(friendly_error(&err).to_owned()),
            )
        }
    };
    let (sha256, hash_issue) = match out.sha256 {
        Ok(h) => (h, None),
        Err(e) => (None, Some(e)),
    };
    let last_error = last_error.or_else(|| {
        hash_issue
            .as_ref()
            .map(|e| format!("Не вдалося прочитати файл: {e}"))
    });
    let facts = FileFacts {
        kind: out.kind,
        mtime_ms: f.mtime_ms,
        sniffed_format: out.sniffed_format,
        manual_place,
    };
    let d = derive(&raw, &facts, settings, ctx, geocoder);
    let n = d.normalized;
    let path = f.path.to_string_lossy().into_owned();
    let issue = last_error.clone();
    (
        IndexedFile {
            root_id,
            normalized_path: normalized_path(&f.path),
            file_name: f
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            path,
            extension: f.extension.clone(),
            kind: out.kind,
            size: f.size,
            mtime: f.mtime_ms,
            file_identity: f.file_identity.clone(),
            sha256,
            detected_type: n.detected_type,
            metadata_status: status,
            provider_key: provider_key.to_owned(),
            metadata_json: (!raw.tags.is_empty() || !raw.providers.is_empty())
                .then(|| serde_json::to_string(&raw).unwrap_or_default()),
            captured: n.captured,
            gps: n.gps,
            place_city: n.place_text.city,
            place_region: n.place_text.region,
            place_country: n.place_text.country,
            place_label: d.place.label,
            location_source: d.place.source,
            place_status: d.place.status,
            width: n.width,
            height: n.height,
            duration_ms: n.duration_ms,
            video_codec: n.video_codec,
            audio_codec: n.audio_codec,
            review_reasons: n.review_reasons,
            warnings,
            last_error,
        },
        issue,
    )
}

pub struct ScanRequest<'a> {
    pub roots: &'a [SourceRoot],
    pub settings: &'a AppSettings,
    /// Never entered, e.g. the archive destination.
    pub exclude: Vec<PathBuf>,
}

/// Runs a full incremental scan. `on_progress` is called often; callers that
/// forward it to a UI should throttle.
pub fn run_scan(
    db: &mut Db,
    req: &ScanRequest<'_>,
    helpers: &Helpers,
    geocoder: Option<&dyn ReverseGeocoder>,
    cancel: &AtomicBool,
    on_progress: &mut dyn FnMut(&ScanProgress),
) -> CoreResult<ScanProgress> {
    let settings = req.settings;
    let job_id = db.start_scan_job(&serde_json::to_string(
        &req.roots.iter().map(|r| &r.path).collect::<Vec<_>>(),
    )?)?;
    let ctx = NormalizeContext::system(settings.filesystem_date_fallback);
    let provider_key = helpers.provider_key(settings.compute_hashes);
    let workers = usize::from(settings.metadata_workers.max(1));
    let walk_options = WalkOptions {
        follow_symlinks: settings.follow_symlinks,
        skip_system_dirs: settings.skip_system_dirs,
        exclude: req.exclude.clone(),
    };
    let mut p = ScanProgress {
        job_id,
        phase: ScanPhase::Walking,
        root: None,
        found: 0,
        processed: 0,
        unchanged: 0,
        skipped: 0,
        errors: 0,
        pending: 0,
        current: None,
    };
    let mut issues: Vec<(String, &'static str, String)> = Vec::new();

    'roots: for root in req.roots {
        p.root = Some(root.path.clone());
        p.phase = ScanPhase::Walking;
        let scan_ts = now_ms();
        let mut files = Vec::new();
        let walked = scanner::walk(
            std::path::Path::new(&root.path),
            &walk_options,
            cancel,
            |event| match event {
                WalkEvent::File(f) => {
                    files.push(f);
                    p.found += 1;
                    if p.found.is_multiple_of(200) {
                        on_progress(&p);
                    }
                }
                WalkEvent::Skipped { path, reason } => {
                    p.skipped += 1;
                    let kind = match reason {
                        SkipReason::NotMedia => return,
                        SkipReason::SystemDir => "system_dir",
                        SkipReason::Symlink => "symlink",
                        SkipReason::Excluded => "excluded",
                    };
                    issues.push((path.to_string_lossy().into_owned(), kind, String::new()));
                }
                WalkEvent::Error { path, message } => {
                    p.errors += 1;
                    issues.push((path.to_string_lossy().into_owned(), "walk_error", message));
                }
            },
        );
        let outcome = match walked {
            Ok(o) => {
                db.set_root_available(root.id, true)?;
                o
            }
            Err(err) => {
                db.set_root_available(root.id, false)?;
                p.errors += 1;
                issues.push((root.path.clone(), "root_unavailable", err.to_string()));
                continue;
            }
        };
        if outcome == WalkOutcome::Cancelled {
            break 'roots;
        }

        // Split into unchanged (cache hit) and files to read.
        let existing = db.existing_for_root(root.id)?;
        let mut unchanged_ids = Vec::new();
        let mut todo: Vec<(FoundFile, Option<Existing>)> = Vec::new();
        for f in files {
            let ex = existing.get(&normalized_path(&f.path)).cloned();
            match &ex {
                Some(e)
                    if e.size == f.size
                        && e.mtime == f.mtime_ms
                        && e.provider_key.as_deref() == Some(provider_key.as_str()) =>
                {
                    unchanged_ids.push(e.id);
                }
                _ => todo.push((f, ex)),
            }
        }
        db.touch_unchanged(&unchanged_ids, scan_ts)?;
        p.unchanged += unchanged_ids.len() as u64;
        p.pending = todo.len() as u64;
        p.phase = ScanPhase::Reading;
        on_progress(&p);

        let chunk_len = workers * FILES_PER_WORKER;
        let mut todo = todo.into_iter().peekable();
        while todo.peek().is_some() {
            if cancel.load(Ordering::Relaxed) {
                break 'roots;
            }
            let chunk: Vec<(FoundFile, Option<Existing>)> = todo.by_ref().take(chunk_len).collect();
            p.current = chunk
                .first()
                .map(|(f, _)| f.path.to_string_lossy().into_owned());
            on_progress(&p);
            let manual: std::collections::HashMap<String, Option<String>> = chunk
                .iter()
                .map(|(f, e)| {
                    (
                        normalized_path(&f.path),
                        e.as_ref().and_then(|e| e.manual_place.clone()),
                    )
                })
                .collect();
            let slices: Vec<Vec<FoundFile>> = chunk
                .into_iter()
                .map(|(f, _)| f)
                .collect::<Vec<_>>()
                .chunks(FILES_PER_WORKER)
                .map(<[FoundFile]>::to_vec)
                .collect();
            let outputs: Vec<WorkerOut> = thread::scope(|s| {
                let handles: Vec<_> = slices
                    .into_iter()
                    .map(|slice| s.spawn(|| work(slice, helpers, settings.compute_hashes, cancel)))
                    .collect();
                handles
                    .into_iter()
                    .flat_map(|h| h.join().unwrap_or_default())
                    .collect()
            });
            if cancel.load(Ordering::Relaxed) {
                // Hashes of this chunk may be incomplete; do not write it.
                break 'roots;
            }
            let mut batch = Vec::with_capacity(outputs.len());
            for out in outputs {
                let key = normalized_path(&out.found.path);
                let manual_place = manual.get(&key).cloned().flatten();
                let (file, issue) = to_indexed(
                    out,
                    root.id,
                    &provider_key,
                    manual_place.as_deref(),
                    settings,
                    &ctx,
                    geocoder,
                );
                if let Some(message) = issue {
                    p.errors += 1;
                    issues.push((file.path.clone(), "read_error", message));
                }
                batch.push(file);
            }
            p.processed += batch.len() as u64;
            p.pending = p.pending.saturating_sub(batch.len() as u64);
            db.write_indexed(&batch, scan_ts)?;
            db.update_scan_job(
                job_id,
                p.found,
                p.processed,
                p.unchanged,
                p.skipped,
                p.errors,
            )?;
        }
        db.mark_missing(root.id, scan_ts)?;
    }

    let cancelled = cancel.load(Ordering::Relaxed);
    p.phase = if cancelled {
        ScanPhase::Cancelled
    } else {
        ScanPhase::Done
    };
    p.current = None;
    db.add_scan_issues(job_id, &issues)?;
    db.update_scan_job(
        job_id,
        p.found,
        p.processed,
        p.unchanged,
        p.skipped,
        p.errors,
    )?;
    db.finish_scan_job(job_id, if cancelled { "cancelled" } else { "done" }, None)?;
    on_progress(&p);
    Ok(p)
}

/// Recomputes dates and places of every indexed file from the stored
/// metadata, e.g. after the file-system date fallback or place level changed.
/// No file is opened.
pub fn renormalize(
    db: &mut Db,
    settings: &AppSettings,
    geocoder: Option<&dyn ReverseGeocoder>,
) -> CoreResult<usize> {
    let ctx = NormalizeContext::system(settings.filesystem_date_fallback);
    let rows = db.items_for_renormalize()?;
    let mut updates = Vec::with_capacity(rows.len());
    for row in rows {
        let raw: RawMetadata = row
            .metadata_json
            .as_deref()
            .and_then(|j| serde_json::from_str(j).ok())
            .unwrap_or_default();
        let facts = FileFacts {
            kind: row.kind,
            mtime_ms: row.mtime,
            sniffed_format: row.detected_type.as_deref(),
            manual_place: row.manual_place.as_deref(),
        };
        let d = derive(&raw, &facts, settings, &ctx, geocoder);
        let n = d.normalized;
        updates.push(DerivedUpdate {
            id: row.id,
            captured: n.captured,
            gps: n.gps,
            place_city: n.place_text.city,
            place_region: n.place_text.region,
            place_country: n.place_text.country,
            place_label: d.place.label,
            location_source: d.place.source,
            place_status: d.place.status,
            review_reasons: n.review_reasons,
        });
    }
    db.update_derived(&updates)?;
    Ok(updates.len())
}
