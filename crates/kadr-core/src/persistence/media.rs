//! Media index queries: source roots, scan bookkeeping, writing scanned items
//! without losing manual overrides, and paged library queries.

use std::collections::HashMap;
use std::path::Path;

use chrono::NaiveDateTime;
use rusqlite::{OptionalExtension, Row, Transaction, params};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::Db;
use crate::domain::captured_date::CapturedDate;
use crate::domain::location::{GeoPoint, LocationSource, PlaceStatus};
use crate::domain::media_item::{MediaKind, MetadataStatus};
use crate::errors::CoreResult;

pub const LOCAL_TIME_FORMAT: &str = "%Y-%m-%dT%H:%M:%S";

/// Stored form of serde enums: their snake_case name.
pub(crate) fn enum_str<T: Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

pub(crate) fn enum_parse<T: DeserializeOwned>(s: &str) -> Option<T> {
    serde_json::from_value(serde_json::Value::String(s.to_owned())).ok()
}

/// Key that identifies a file across scans. Windows paths are
/// case-insensitive, so they are folded.
pub fn normalized_path(path: &Path) -> String {
    let s = path.to_string_lossy();
    if cfg!(windows) {
        s.replace('/', "\\").to_lowercase()
    } else {
        s.into_owned()
    }
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SourceRoot {
    pub id: i64,
    pub path: String,
    pub added_at: i64,
    pub is_available: bool,
}

/// What the index already knows about a file, to decide whether to re-read it.
#[derive(Debug, Clone)]
pub struct Existing {
    pub id: i64,
    pub size: u64,
    pub mtime: i64,
    pub provider_key: Option<String>,
    pub sha256: Option<String>,
    pub manual_place: Option<String>,
}

/// One scanned file, ready to be written.
#[derive(Debug, Clone)]
pub struct IndexedFile {
    pub root_id: i64,
    pub path: String,
    pub normalized_path: String,
    pub file_name: String,
    pub extension: String,
    pub kind: MediaKind,
    pub size: u64,
    pub mtime: i64,
    pub file_identity: Option<String>,
    pub sha256: Option<String>,
    pub detected_type: Option<String>,
    pub metadata_status: MetadataStatus,
    pub provider_key: String,
    pub metadata_json: Option<String>,
    pub captured: Option<CapturedDate>,
    pub gps: Option<GeoPoint>,
    pub place_city: Option<String>,
    pub place_region: Option<String>,
    pub place_country: Option<String>,
    pub place_label: Option<String>,
    pub location_source: LocationSource,
    pub place_status: PlaceStatus,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration_ms: Option<u64>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub review_reasons: Vec<String>,
    pub warnings: Vec<String>,
    pub last_error: Option<String>,
}

pub fn search_text(
    file_name: &str,
    path: &str,
    place: Option<&str>,
    captured_local: Option<&str>,
) -> String {
    let date = captured_local
        .and_then(|d| NaiveDateTime::parse_from_str(d, LOCAL_TIME_FORMAT).ok())
        .map(|d| d.format("%d.%m.%Y").to_string())
        .unwrap_or_default();
    format!("{file_name}\n{path}\n{}\n{date}", place.unwrap_or_default()).to_lowercase()
}

#[derive(Debug, Clone, Serialize)]
pub struct LastScan {
    pub job_id: i64,
    pub finished_at: i64,
    /// `done`, `cancelled` or `interrupted`.
    pub state: String,
    pub found: u64,
    pub processed: u64,
    pub unchanged: u64,
    pub skipped: u64,
    pub errors: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibrarySection {
    #[default]
    All,
    Review,
    NoDate,
    NoGps,
    Duplicates,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KindFilter {
    #[default]
    All,
    Photo,
    Video,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortKey {
    #[default]
    Date,
    Name,
    Size,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MediaQuery {
    pub section: LibrarySection,
    pub kind: KindFilter,
    pub search: String,
    pub sort: SortKey,
    pub offset: u32,
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaRow {
    pub id: i64,
    pub path: String,
    /// Modification time, ms; also busts cached thumbnails in the UI.
    pub mtime: i64,
    pub file_name: String,
    pub extension: String,
    pub kind: MediaKind,
    pub size_bytes: u64,
    pub sha256: Option<String>,
    pub detected_type: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration_ms: Option<u64>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub captured: Option<CapturedDate>,
    pub gps: Option<GeoPoint>,
    pub place_label: Option<String>,
    pub location_source: LocationSource,
    pub place_status: PlaceStatus,
    pub metadata_status: MetadataStatus,
    pub metadata_provider: Option<String>,
    pub review_reasons: Vec<String>,
    pub warnings: Vec<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaPage {
    pub items: Vec<MediaRow>,
    pub total: u64,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct SectionCounts {
    pub all: u64,
    pub review: u64,
    pub nodate: u64,
    pub nogps: u64,
    pub duplicates: u64,
    pub with_date: u64,
    pub with_gps: u64,
}

const VISIBLE: &str = "m.is_sidecar = 0 AND m.is_missing = 0";
const REVIEW: &str = "(m.review_reasons <> '[]' OR m.metadata_status IN ('failed', 'unavailable', 'partial') \
                      OR m.date_status IN ('needs_review', 'file_system_fallback'))";
const NO_DATE: &str = "m.captured_at_local IS NULL";
const NO_GPS: &str = "m.place_status <> 'resolved'";
const DUPLICATE: &str = "m.sha256 IN (SELECT sha256 FROM media_item WHERE sha256 IS NOT NULL \
                         AND is_sidecar = 0 AND is_missing = 0 GROUP BY sha256 HAVING COUNT(*) > 1)";

fn section_sql(section: LibrarySection) -> &'static str {
    match section {
        LibrarySection::All => "1",
        LibrarySection::Review => REVIEW,
        LibrarySection::NoDate => NO_DATE,
        LibrarySection::NoGps => NO_GPS,
        LibrarySection::Duplicates => DUPLICATE,
    }
}

const ROW_COLUMNS: &str = "m.id, m.path, m.file_name, m.extension, m.media_kind, m.size_bytes, m.sha256, \
    m.detected_type, m.width, m.height, m.duration_ms, m.video_codec, m.audio_codec, \
    m.captured_at_raw, m.captured_at_local, m.timezone_offset_min, m.date_source, m.date_status, m.date_note, \
    m.latitude, m.longitude, m.altitude, m.place_label, m.location_source, m.place_status, \
    m.metadata_status, m.metadata_provider, m.review_reasons, m.warnings, m.last_error, m.mtime";

fn json_list(s: Option<String>) -> Vec<String> {
    s.and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn read_row(r: &Row<'_>) -> rusqlite::Result<MediaRow> {
    let text = |i: usize| r.get::<_, Option<String>>(i);
    let captured = match (text(13)?, text(14)?) {
        (Some(raw), Some(local)) => NaiveDateTime::parse_from_str(&local, LOCAL_TIME_FORMAT)
            .ok()
            .and_then(|local| {
                Some(CapturedDate {
                    raw,
                    local,
                    offset_minutes: r.get(15).ok()?,
                    source: enum_parse(&text(16).ok()??)?,
                    status: enum_parse(&text(17).ok()??)?,
                    note: text(18).ok()?,
                })
            }),
        _ => None,
    };
    let gps = match (r.get::<_, Option<f64>>(19)?, r.get::<_, Option<f64>>(20)?) {
        (Some(lat), Some(lon)) => GeoPoint::new(lat, lon, r.get(21)?),
        _ => None,
    };
    Ok(MediaRow {
        id: r.get(0)?,
        path: r.get(1)?,
        file_name: r.get(2)?,
        extension: r.get(3)?,
        kind: enum_parse(&r.get::<_, String>(4)?).unwrap_or(MediaKind::Unsupported),
        size_bytes: u64::try_from(r.get::<_, i64>(5)?).unwrap_or(0),
        sha256: r.get(6)?,
        detected_type: r.get(7)?,
        width: r.get(8)?,
        height: r.get(9)?,
        duration_ms: r
            .get::<_, Option<i64>>(10)?
            .and_then(|d| u64::try_from(d).ok()),
        video_codec: r.get(11)?,
        audio_codec: r.get(12)?,
        captured,
        gps,
        place_label: r.get(22)?,
        location_source: enum_parse(&r.get::<_, String>(23)?).unwrap_or(LocationSource::None),
        place_status: enum_parse(&r.get::<_, String>(24)?).unwrap_or(PlaceStatus::NoGps),
        metadata_status: enum_parse(&r.get::<_, String>(25)?).unwrap_or(MetadataStatus::NotRead),
        metadata_provider: r.get(26)?,
        review_reasons: json_list(r.get(27)?),
        warnings: json_list(r.get(28)?),
        last_error: r.get(29)?,
        mtime: r.get(30)?,
    })
}

impl Db {
    // ------------------------------------------------------------ roots

    /// Adds a source folder; adding the same path again returns the existing one.
    pub fn add_source_root(&self, path: &str) -> CoreResult<SourceRoot> {
        self.conn.execute(
            "INSERT INTO source_root (path, added_at) VALUES (?1, ?2) ON CONFLICT(path) DO NOTHING",
            params![path, now_ms()],
        )?;
        Ok(self.conn.query_row(
            "SELECT id, path, added_at, is_available FROM source_root WHERE path = ?1",
            params![path],
            |r| {
                Ok(SourceRoot {
                    id: r.get(0)?,
                    path: r.get(1)?,
                    added_at: r.get(2)?,
                    is_available: r.get(3)?,
                })
            },
        )?)
    }

    pub fn source_roots(&self) -> CoreResult<Vec<SourceRoot>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, path, added_at, is_available FROM source_root ORDER BY path")?;
        let rows = stmt.query_map([], |r| {
            Ok(SourceRoot {
                id: r.get(0)?,
                path: r.get(1)?,
                added_at: r.get(2)?,
                is_available: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Forgets a source folder and its index rows. Files on disk are untouched.
    pub fn remove_source_root(&self, id: i64) -> CoreResult<()> {
        self.conn
            .execute("DELETE FROM source_root WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn set_root_available(&self, id: i64, available: bool) -> CoreResult<()> {
        self.conn.execute(
            "UPDATE source_root SET is_available = ?2, last_seen_at = CASE WHEN ?2 THEN ?3 ELSE last_seen_at END WHERE id = ?1",
            params![id, available, now_ms()],
        )?;
        Ok(())
    }

    // ------------------------------------------------------------ scan jobs

    pub fn start_scan_job(&self, options_json: &str) -> CoreResult<i64> {
        self.conn.execute(
            "INSERT INTO scan_job (started_at, state, options_json) VALUES (?1, 'running', ?2)",
            params![now_ms(), options_json],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_scan_job(
        &self,
        id: i64,
        found: u64,
        processed: u64,
        unchanged: u64,
        skipped: u64,
        errors: u64,
    ) -> CoreResult<()> {
        let n = |v: u64| i64::try_from(v).unwrap_or(i64::MAX);
        self.conn.execute(
            "UPDATE scan_job SET found = ?2, processed = ?3, unchanged = ?4, skipped = ?5, errors = ?6 WHERE id = ?1",
            params![id, n(found), n(processed), n(unchanged), n(skipped), n(errors)],
        )?;
        Ok(())
    }

    pub fn finish_scan_job(
        &self,
        id: i64,
        state: &str,
        error_detail: Option<&str>,
    ) -> CoreResult<()> {
        self.conn.execute(
            "UPDATE scan_job SET state = ?2, finished_at = ?3, error_detail = ?4 WHERE id = ?1",
            params![id, state, now_ms(), error_detail],
        )?;
        Ok(())
    }

    /// The most recent finished scan, for "last updated" in the UI.
    pub fn last_scan(&self) -> CoreResult<Option<LastScan>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, finished_at, state, found, processed, unchanged, skipped, errors FROM scan_job
                 WHERE finished_at IS NOT NULL ORDER BY id DESC LIMIT 1",
                [],
                |r| {
                    let n = |i: usize| r.get::<_, i64>(i).map(|v| u64::try_from(v).unwrap_or(0));
                    Ok(LastScan {
                        job_id: r.get(0)?,
                        finished_at: r.get(1)?,
                        state: r.get(2)?,
                        found: n(3)?,
                        processed: n(4)?,
                        unchanged: n(5)?,
                        skipped: n(6)?,
                        errors: n(7)?,
                    })
                },
            )
            .optional()?)
    }

    /// Jobs still marked running belong to a session that crashed or was closed.
    pub fn mark_interrupted_scans(&self) -> CoreResult<usize> {
        Ok(self.conn.execute(
            "UPDATE scan_job SET state = 'interrupted', finished_at = ?1 WHERE state = 'running'",
            params![now_ms()],
        )?)
    }

    pub fn add_scan_issues(
        &mut self,
        job_id: i64,
        issues: &[(String, &'static str, String)],
    ) -> CoreResult<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO scan_issue (job_id, path, kind, message) VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (path, kind, message) in issues {
                stmt.execute(params![job_id, path, kind, message])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    // ------------------------------------------------------------ items

    pub fn existing_for_root(&self, root_id: i64) -> CoreResult<HashMap<String, Existing>> {
        let mut stmt = self.conn.prepare(
            "SELECT normalized_path, id, size_bytes, mtime, metadata_provider, sha256, manual_place_override
             FROM media_item WHERE source_root_id = ?1",
        )?;
        let rows = stmt.query_map(params![root_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                Existing {
                    id: r.get(1)?,
                    size: u64::try_from(r.get::<_, i64>(2)?).unwrap_or(0),
                    mtime: r.get(3)?,
                    provider_key: r.get(4)?,
                    sha256: r.get(5)?,
                    manual_place: r.get(6)?,
                },
            ))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Writes a batch of scanned files in one transaction.
    pub fn write_indexed(&mut self, files: &[IndexedFile], scan_ts: i64) -> CoreResult<()> {
        let tx = self.conn.transaction()?;
        for f in files {
            upsert(&tx, f, scan_ts)?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Files that did not change keep their data; only the scan time moves.
    pub fn touch_unchanged(&mut self, ids: &[i64], scan_ts: i64) -> CoreResult<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "UPDATE media_item SET scan_timestamp = ?2, is_missing = 0 WHERE id = ?1",
            )?;
            for id in ids {
                stmt.execute(params![id, scan_ts])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// After a complete scan of a root: rows not seen in it are marked missing
    /// (kept, so manual overrides survive a temporarily unplugged drive).
    pub fn mark_missing(&self, root_id: i64, scan_ts: i64) -> CoreResult<usize> {
        Ok(self.conn.execute(
            "UPDATE media_item SET is_missing = 1 WHERE source_root_id = ?1 AND scan_timestamp < ?2 AND is_missing = 0",
            params![root_id, scan_ts],
        )?)
    }

    // ------------------------------------------------------------ queries

    pub fn query_media(&self, q: &MediaQuery) -> CoreResult<MediaPage> {
        let kind = match q.kind {
            KindFilter::All => "1",
            KindFilter::Photo => "m.media_kind IN ('photo', 'raw_photo')",
            KindFilter::Video => "m.media_kind = 'video'",
        };
        let search = q.search.trim().to_lowercase();
        let pattern = format!(
            "%{}%",
            search
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );
        let filter = format!(
            "FROM media_item m WHERE {VISIBLE} AND {} AND {kind} AND (?1 = '' OR m.search_text LIKE ?2 ESCAPE '\\')",
            section_sql(q.section)
        );
        let order = match q.sort {
            SortKey::Date => "m.captured_at_local IS NULL, m.captured_at_local DESC, m.file_name",
            SortKey::Name => "m.file_name COLLATE NOCASE, m.id",
            SortKey::Size => "m.size_bytes DESC, m.id",
        };
        let total: i64 = self.conn.query_row(
            &format!("SELECT COUNT(*) {filter}"),
            params![search, pattern],
            |r| r.get(0),
        )?;
        let limit = if q.limit == 0 { 200 } else { q.limit.min(1000) };
        let sql = format!("SELECT {ROW_COLUMNS} {filter} ORDER BY {order} LIMIT ?3 OFFSET ?4");
        let mut stmt = self.conn.prepare(&sql)?;
        let items = stmt
            .query_map(params![search, pattern, limit, q.offset], read_row)?
            .collect::<Result<_, _>>()?;
        Ok(MediaPage {
            items,
            total: u64::try_from(total).unwrap_or(0),
        })
    }

    pub fn media_details(&self, id: i64) -> CoreResult<Option<MediaRow>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {ROW_COLUMNS} FROM media_item m WHERE m.id = ?1"),
                params![id],
                read_row,
            )
            .optional()?)
    }

    /// Facts the thumbnailer needs for one item.
    pub fn thumb_source(
        &self,
        id: i64,
    ) -> CoreResult<Option<crate::services::thumbnail::ThumbSource>> {
        Ok(self
            .conn
            .query_row(
                "SELECT path, media_kind, extension, size_bytes, mtime, duration_ms, metadata_json
                 FROM media_item WHERE id = ?1",
                params![id],
                |r| {
                    let json: Option<String> = r.get(6)?;
                    let orientation = json
                        .and_then(|j| {
                            serde_json::from_str::<crate::services::metadata::RawMetadata>(&j).ok()
                        })
                        .and_then(|m| m.f64("IFD0:Orientation"))
                        .and_then(|o| u8::try_from(o as i64).ok());
                    Ok(crate::services::thumbnail::ThumbSource {
                        path: std::path::PathBuf::from(r.get::<_, String>(0)?),
                        kind: enum_parse(&r.get::<_, String>(1)?).unwrap_or(MediaKind::Unsupported),
                        extension: r.get(2)?,
                        size_bytes: u64::try_from(r.get::<_, i64>(3)?).unwrap_or(0),
                        mtime_ms: r.get(4)?,
                        duration_ms: r
                            .get::<_, Option<i64>>(5)?
                            .and_then(|d| u64::try_from(d).ok()),
                        orientation,
                    })
                },
            )
            .optional()?)
    }

    pub fn section_counts(&self) -> CoreResult<SectionCounts> {
        let sql = format!(
            "SELECT COUNT(*),
                    COALESCE(SUM({REVIEW}), 0),
                    COALESCE(SUM({NO_DATE}), 0),
                    COALESCE(SUM({NO_GPS}), 0),
                    COALESCE(SUM({DUPLICATE}), 0),
                    COALESCE(SUM(m.captured_at_local IS NOT NULL AND m.date_status <> 'file_system_fallback'), 0),
                    COALESCE(SUM(m.latitude IS NOT NULL), 0)
             FROM media_item m WHERE {VISIBLE}"
        );
        Ok(self.conn.query_row(&sql, [], |r| {
            let n = |i: usize| r.get::<_, i64>(i).map(|v| u64::try_from(v).unwrap_or(0));
            Ok(SectionCounts {
                all: n(0)?,
                review: n(1)?,
                nodate: n(2)?,
                nogps: n(3)?,
                duplicates: n(4)?,
                with_date: n(5)?,
                with_gps: n(6)?,
            })
        })?)
    }

    /// Sets (or clears, with `None`) the user's place name for items. It is
    /// never overwritten by a rescan. Call `renormalize` afterwards.
    pub fn set_manual_place(&mut self, ids: &[i64], place: Option<&str>) -> CoreResult<()> {
        let place = place.map(str::trim).filter(|p| !p.is_empty());
        let tx = self.conn.transaction()?;
        {
            let mut stmt =
                tx.prepare("UPDATE media_item SET manual_place_override = ?2 WHERE id = ?1")?;
            for id in ids {
                stmt.execute(params![id, place])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Rows whose date and place can be recomputed from stored metadata.
    pub fn items_for_renormalize(&self) -> CoreResult<Vec<StoredFacts>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, media_kind, mtime, metadata_json, detected_type, manual_place_override
             FROM media_item WHERE is_sidecar = 0",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(StoredFacts {
                id: r.get(0)?,
                kind: enum_parse(&r.get::<_, String>(1)?).unwrap_or(MediaKind::Unsupported),
                mtime: r.get(2)?,
                metadata_json: r.get(3)?,
                detected_type: r.get(4)?,
                manual_place: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn update_derived(&mut self, updates: &[DerivedUpdate]) -> CoreResult<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "UPDATE media_item SET
                    captured_at_raw = ?2, captured_at_local = ?3, timezone_offset_min = ?4, date_source = ?5,
                    date_status = ?6, date_note = ?7, latitude = ?8, longitude = ?9, altitude = ?10,
                    place_city = ?11, place_region = ?12, place_country = ?13, place_label = ?14,
                    location_source = ?15, place_status = ?16, review_reasons = ?17,
                    search_text = ?18
                 WHERE id = ?1",
            )?;
            for u in updates {
                let d = Derived::from_parts(u.captured.as_ref(), u.gps);
                let search: String = tx.query_row(
                    "SELECT file_name || char(10) || path FROM media_item WHERE id = ?1",
                    params![u.id],
                    |r| r.get(0),
                )?;
                let (name, path) = search.split_once('\n').unwrap_or((&search, ""));
                stmt.execute(params![
                    u.id,
                    d.raw,
                    d.local,
                    d.offset,
                    d.source,
                    d.status,
                    d.note,
                    d.lat,
                    d.lon,
                    d.alt,
                    u.place_city,
                    u.place_region,
                    u.place_country,
                    u.place_label,
                    enum_str(&u.location_source),
                    enum_str(&u.place_status),
                    serde_json::to_string(&u.review_reasons)?,
                    search_text(name, path, u.place_label.as_deref(), d.local.as_deref()),
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}

/// What re-normalization needs from a stored row.
pub struct StoredFacts {
    pub id: i64,
    pub kind: MediaKind,
    pub mtime: i64,
    pub metadata_json: Option<String>,
    pub detected_type: Option<String>,
    pub manual_place: Option<String>,
}

/// New date/place values for one row after re-normalization.
pub struct DerivedUpdate {
    pub id: i64,
    pub captured: Option<CapturedDate>,
    pub gps: Option<GeoPoint>,
    pub place_city: Option<String>,
    pub place_region: Option<String>,
    pub place_country: Option<String>,
    pub place_label: Option<String>,
    pub location_source: LocationSource,
    pub place_status: PlaceStatus,
    pub review_reasons: Vec<String>,
}

struct Derived {
    raw: Option<String>,
    local: Option<String>,
    offset: Option<i32>,
    source: Option<String>,
    status: Option<String>,
    note: Option<String>,
    lat: Option<f64>,
    lon: Option<f64>,
    alt: Option<f64>,
}

impl Derived {
    fn of(f: &IndexedFile) -> Self {
        Self::from_parts(f.captured.as_ref(), f.gps)
    }

    fn from_parts(c: Option<&CapturedDate>, gps: Option<GeoPoint>) -> Self {
        Self {
            raw: c.map(|c| c.raw.clone()),
            local: c.map(|c| c.local.format(LOCAL_TIME_FORMAT).to_string()),
            offset: c.and_then(|c| c.offset_minutes),
            source: c.map(|c| enum_str(&c.source)),
            status: c.map(|c| enum_str(&c.status)),
            note: c.and_then(|c| c.note.clone()),
            lat: gps.map(|g| g.latitude),
            lon: gps.map(|g| g.longitude),
            alt: gps.and_then(|g| g.altitude),
        }
    }
}

fn upsert(tx: &Transaction<'_>, f: &IndexedFile, scan_ts: i64) -> CoreResult<()> {
    let d = Derived::of(f);
    let to_i64 = |v: u64| i64::try_from(v).unwrap_or(i64::MAX);
    tx.execute(
        "INSERT INTO media_item (
            source_root_id, path, normalized_path, file_name, extension, media_kind, size_bytes, mtime,
            file_identity, sha256, fingerprint_updated_at, detected_type, metadata_status, metadata_provider,
            metadata_json, captured_at_raw, captured_at_local, timezone_offset_min, date_source, date_status,
            date_note, latitude, longitude, altitude, place_city, place_region, place_country, place_label,
            location_source, place_status, width, height, duration_ms, video_codec, audio_codec,
            review_reasons, warnings, last_error, is_sidecar, scan_timestamp, is_missing, search_text
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20,
                   ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38,
                   ?39, ?40, 0, ?41)
         ON CONFLICT(normalized_path) DO UPDATE SET
            source_root_id = excluded.source_root_id, path = excluded.path, file_name = excluded.file_name,
            extension = excluded.extension, media_kind = excluded.media_kind, size_bytes = excluded.size_bytes,
            mtime = excluded.mtime, file_identity = excluded.file_identity, sha256 = excluded.sha256,
            fingerprint_updated_at = excluded.fingerprint_updated_at, detected_type = excluded.detected_type,
            metadata_status = excluded.metadata_status, metadata_provider = excluded.metadata_provider,
            metadata_json = excluded.metadata_json, captured_at_raw = excluded.captured_at_raw,
            captured_at_local = excluded.captured_at_local, timezone_offset_min = excluded.timezone_offset_min,
            date_source = excluded.date_source, date_status = excluded.date_status, date_note = excluded.date_note,
            latitude = excluded.latitude, longitude = excluded.longitude, altitude = excluded.altitude,
            place_city = excluded.place_city, place_region = excluded.place_region,
            place_country = excluded.place_country, place_label = excluded.place_label,
            location_source = excluded.location_source, place_status = excluded.place_status,
            width = excluded.width, height = excluded.height, duration_ms = excluded.duration_ms,
            video_codec = excluded.video_codec, audio_codec = excluded.audio_codec,
            review_reasons = excluded.review_reasons, warnings = excluded.warnings,
            last_error = excluded.last_error, is_sidecar = excluded.is_sidecar,
            scan_timestamp = excluded.scan_timestamp, is_missing = 0, search_text = excluded.search_text",
        params![
            f.root_id,
            f.path,
            f.normalized_path,
            f.file_name,
            f.extension,
            enum_str(&f.kind),
            to_i64(f.size),
            f.mtime,
            f.file_identity,
            f.sha256,
            f.sha256.as_ref().map(|_| scan_ts),
            f.detected_type,
            enum_str(&f.metadata_status),
            f.provider_key,
            f.metadata_json,
            d.raw,
            d.local,
            d.offset,
            d.source,
            d.status,
            d.note,
            d.lat,
            d.lon,
            d.alt,
            f.place_city,
            f.place_region,
            f.place_country,
            f.place_label,
            enum_str(&f.location_source),
            enum_str(&f.place_status),
            f.width,
            f.height,
            f.duration_ms.map(to_i64),
            f.video_codec,
            f.audio_codec,
            serde_json::to_string(&f.review_reasons)?,
            serde_json::to_string(&f.warnings)?,
            f.last_error,
            f.kind == MediaKind::Sidecar,
            scan_ts,
            search_text(&f.file_name, &f.path, f.place_label.as_deref(), d.local.as_deref()),
        ],
    )?;
    Ok(())
}
