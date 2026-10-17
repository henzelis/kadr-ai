//! End-to-end indexing on real fixture files (tests/fixtures/media), covering
//! the acceptance criteria of spec §18 that do not need the UI.
//! Tests that need ExifTool/FFprobe print a notice and pass when the helper
//! is missing, so the suite also runs on machines without them.

#![allow(clippy::unwrap_used, clippy::expect_used)] // test helpers panic on purpose

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use kadr_core::domain::captured_date::{DateSource, DateStatus};
use kadr_core::domain::location::PlaceStatus;
use kadr_core::domain::media_item::{MediaKind, MetadataStatus};
use kadr_core::persistence::Db;
use kadr_core::persistence::media::{LibrarySection, MediaQuery, MediaRow, SourceRoot};
use kadr_core::services::indexer::{Helpers, ScanPhase, ScanRequest, renormalize, run_scan};
use kadr_core::settings::AppSettings;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/media")
}

/// Copies the fixtures into `<tmp>/library/...` with some nesting.
fn library(tmp: &Path) -> PathBuf {
    let lib = tmp.join("library");
    for entry in fs::read_dir(fixtures()).unwrap() {
        let src = entry.unwrap().path();
        let name = src.file_name().unwrap();
        let dest = if src.extension().is_some_and(|e| e == "mov" || e == "mp4") {
            lib.join("Відео").join("2025").join(name)
        } else {
            lib.join("Фото").join(name)
        };
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::copy(&src, &dest).unwrap();
    }
    lib
}

struct Env {
    _tmp: tempfile::TempDir,
    lib: PathBuf,
    db: Db,
    root: SourceRoot,
    settings: AppSettings,
}

fn env() -> Env {
    let tmp = tempfile::tempdir().unwrap();
    let lib = library(tmp.path());
    let db = Db::open(&tmp.path().join("index.sqlite3")).unwrap();
    let root = db.add_source_root(&lib.to_string_lossy()).unwrap();
    Env {
        _tmp: tmp,
        lib,
        db,
        root,
        settings: AppSettings::default(),
    }
}

fn scan(e: &mut Env, helpers: &Helpers) -> kadr_core::services::indexer::ScanProgress {
    let roots = vec![e.root.clone()];
    let req = ScanRequest {
        roots: &roots,
        settings: &e.settings,
        exclude: vec![],
    };
    run_scan(
        &mut e.db,
        &req,
        helpers,
        None,
        &AtomicBool::new(false),
        &mut |_| {},
    )
    .unwrap()
}

fn all(db: &Db, section: LibrarySection) -> Vec<MediaRow> {
    db.query_media(&MediaQuery {
        section,
        limit: 1000,
        ..MediaQuery::default()
    })
    .unwrap()
    .items
}

fn by_name<'a>(rows: &'a [MediaRow], name: &str) -> &'a MediaRow {
    rows.iter()
        .find(|r| r.file_name == name)
        .unwrap_or_else(|| panic!("{name} not indexed"))
}

fn real_helpers() -> Option<Helpers> {
    let (h, _) = Helpers::detect(&AppSettings::default(), None);
    if h.exiftool.is_none() || h.ffprobe.is_none() {
        eprintln!("NOTICE: ExifTool or FFprobe missing — helper-based test skipped");
        return None;
    }
    Some(h)
}

#[test]
fn full_scan_with_helpers_reads_dates_gps_and_video() {
    let Some(helpers) = real_helpers() else {
        return;
    };
    let mut e = env();
    let p = scan(&mut e, &helpers);
    assert_eq!(p.phase, ScanPhase::Done);
    // 13 fixture files; the AAE is a sidecar and stays out of the library list.
    assert_eq!(p.found, 13);
    let rows = all(&e.db, LibrarySection::All);
    assert_eq!(rows.len(), 12);

    // §18.3 EXIF date with offset → year/month.
    let full = by_name(&rows, "exif_full.jpg");
    let d = full.captured.as_ref().unwrap();
    assert_eq!(
        d.local.format("%Y-%m-%d %H:%M:%S").to_string(),
        "2025-08-13 16:42:07"
    );
    assert_eq!(
        (d.offset_minutes, d.source, d.status),
        (Some(180), DateSource::ExifOriginal, DateStatus::Confirmed)
    );
    // §18.4 GPS stored; §18.5 no place without a geobase → "GPS є, місце не визначено".
    let gps = full.gps.unwrap();
    assert!((gps.latitude - 48.4594).abs() < 1e-6 && (gps.longitude - 24.5513).abs() < 1e-6);
    assert_eq!(
        (full.place_label.as_deref(), full.place_status),
        (None, PlaceStatus::GpsNoPlace)
    );
    assert_eq!((full.width, full.height), (Some(64), Some(48)));
    assert_eq!(full.sha256.as_ref().map(String::len), Some(64));

    // §18.6 no GPS.
    assert_eq!(
        by_name(&rows, "exif_no_offset.jpg").place_status,
        PlaceStatus::NoGps
    );
    // Southern/western hemisphere signs.
    let sw = by_name(&rows, "southwest.jpg").gps.unwrap();
    assert!(sw.latitude < 0.0 && sw.longitude < 0.0);
    // Explicit XMP city is used as the place.
    let xmp = by_name(&rows, "xmp_only.jpg");
    assert_eq!(xmp.place_label.as_deref(), Some("Львів"));
    assert_eq!(xmp.captured.as_ref().unwrap().offset_minutes, Some(120));
    // GPS 0,0 is not a position.
    let zero = by_name(&rows, "gps_zero.jpg");
    assert!(zero.gps.is_none() && !zero.review_reasons.is_empty());
    // §18.7 no date, no invented date.
    assert!(by_name(&rows, "no_metadata.png").captured.is_none());
    assert!(by_name(&rows, "zero_date.jpg").captured.is_none());

    // §18.9 video: local Apple date, GPS, codec and duration via FFprobe.
    let mov = by_name(&rows, "iphone.mov");
    assert_eq!(mov.kind, MediaKind::Video);
    let md = mov.captured.as_ref().unwrap();
    assert_eq!(
        (md.local.format("%H:%M").to_string(), md.offset_minutes),
        ("10:05".to_owned(), Some(180))
    );
    assert_eq!(mov.video_codec.as_deref(), Some("mpeg4"));
    assert_eq!(mov.duration_ms, Some(1000));
    assert!(mov.gps.is_some());
    let mp4 = by_name(&rows, "android.mp4");
    assert_eq!(mp4.captured.as_ref().unwrap().source, DateSource::QuickTime);
    assert!(
        mp4.captured
            .as_ref()
            .unwrap()
            .note
            .as_ref()
            .unwrap()
            .code
            .starts_with("utc")
    );

    // Content beats extension, with a warning.
    let fake = by_name(&rows, "png_named_as.jpg");
    assert_eq!(fake.detected_type.as_deref(), Some("PNG"));
    assert!(
        fake.warnings
            .iter()
            .any(|w| w.code == "ext_mismatch" && w.args["ext"] == "jpg")
    );
    // §18.11 a damaged file is indexed with a status and does not stop the scan.
    let bad = by_name(&rows, "corrupt.jpg");
    assert_eq!(bad.metadata_status, MetadataStatus::Partial);
    assert!(bad.captured.is_none());

    // §18.19 sections.
    let names = |s| {
        let mut v: Vec<String> = all(&e.db, s).into_iter().map(|r| r.file_name).collect();
        v.sort();
        v
    };
    assert!(names(LibrarySection::NoDate).contains(&"no_metadata.png".to_owned()));
    assert!(!names(LibrarySection::NoDate).contains(&"exif_full.jpg".to_owned()));
    assert!(names(LibrarySection::Review).contains(&"gps_zero.jpg".to_owned()));
    assert!(names(LibrarySection::Review).contains(&"corrupt.jpg".to_owned()));
    assert_eq!(
        names(LibrarySection::Duplicates),
        ["no_metadata.png", "png_named_as.jpg"]
    );
    let counts = e.db.section_counts().unwrap();
    assert_eq!(counts.all, 12);
    assert_eq!(counts.duplicates, 2);
}

#[test]
fn rescan_is_incremental_and_keeps_manual_place() {
    let Some(helpers) = real_helpers() else {
        return;
    };
    let mut e = env();
    scan(&mut e, &helpers);
    let rows = all(&e.db, LibrarySection::All);
    let id = by_name(&rows, "exif_no_offset.jpg").id;

    // §18.8 manual place survives a rescan.
    e.db.set_manual_place(&[id], Some("Дача")).unwrap();
    renormalize(&mut e.db, &e.settings, None).unwrap();

    // §18.2 unchanged library: nothing re-read, no duplicate rows.
    let p = scan(&mut e, &helpers);
    assert_eq!((p.processed, p.unchanged), (0, 13));
    let rows = all(&e.db, LibrarySection::All);
    assert_eq!(rows.len(), 12);
    assert_eq!(
        by_name(&rows, "exif_no_offset.jpg").place_label.as_deref(),
        Some("Дача")
    );

    // A changed file is re-read (manual place still kept), a deleted one is hidden.
    let changed = e.lib.join("Фото/exif_no_offset.jpg");
    let mut bytes = fs::read(&changed).unwrap();
    bytes.extend_from_slice(b"\0\0");
    fs::write(&changed, bytes).unwrap();
    fs::remove_file(e.lib.join("Фото/southwest.jpg")).unwrap();
    let p = scan(&mut e, &helpers);
    assert_eq!(p.processed, 1);
    let rows = all(&e.db, LibrarySection::All);
    assert_eq!(rows.len(), 11);
    assert_eq!(
        by_name(&rows, "exif_no_offset.jpg").place_label.as_deref(),
        Some("Дача")
    );
}

#[test]
fn works_without_external_helpers() {
    // §18.22: no ExifTool/FFprobe → photos via the built-in reader, videos
    // indexed with a clear "unavailable" status, nothing crashes.
    let mut e = env();
    let p = scan(&mut e, &Helpers::default());
    assert_eq!(p.phase, ScanPhase::Done);
    let rows = all(&e.db, LibrarySection::All);
    assert_eq!(rows.len(), 12);
    let full = by_name(&rows, "exif_full.jpg");
    assert_eq!(full.captured.as_ref().unwrap().offset_minutes, Some(180));
    assert!(full.gps.is_some());
    let mov = by_name(&rows, "iphone.mov");
    assert_eq!(mov.metadata_status, MetadataStatus::Unavailable);
    assert!(mov.last_error.as_ref().unwrap().code == "no_helper");
    assert!(
        all(&e.db, LibrarySection::Review)
            .iter()
            .any(|r| r.file_name == "iphone.mov")
    );
}

#[test]
fn filesystem_date_only_when_enabled_and_marked() {
    let mut e = env();
    scan(&mut e, &Helpers::default());
    assert!(
        by_name(&all(&e.db, LibrarySection::All), "no_metadata.png")
            .captured
            .is_none()
    );
    e.settings.filesystem_date_fallback = true;
    renormalize(&mut e.db, &e.settings, None).unwrap();
    let d = by_name(&all(&e.db, LibrarySection::All), "no_metadata.png")
        .captured
        .clone()
        .unwrap();
    assert_eq!(
        (d.source, d.status),
        (DateSource::FileSystem, DateStatus::FileSystemFallback)
    );
}

#[test]
fn cancelled_scan_leaves_consistent_index() {
    // §18.10: cancel before reading starts; rescan afterwards completes.
    let mut e = env();
    let cancel = AtomicBool::new(false);
    let roots = vec![e.root.clone()];
    let req = ScanRequest {
        roots: &roots,
        settings: &e.settings,
        exclude: vec![],
    };
    let p = run_scan(
        &mut e.db,
        &req,
        &Helpers::default(),
        None,
        &cancel,
        &mut |p| {
            if p.phase == kadr_core::services::indexer::ScanPhase::Reading {
                cancel.store(true, Ordering::Relaxed);
            }
        },
    )
    .unwrap();
    assert_eq!(p.phase, ScanPhase::Cancelled);
    assert!(all(&e.db, LibrarySection::All).len() < 12);
    let p = scan(&mut e, &Helpers::default());
    assert_eq!(p.phase, ScanPhase::Done);
    assert_eq!(all(&e.db, LibrarySection::All).len(), 12);
}

#[test]
fn search_is_case_insensitive_for_cyrillic() {
    let Some(helpers) = real_helpers() else {
        return;
    };
    let mut e = env();
    scan(&mut e, &helpers);
    let q = |s: &str| {
        e.db.query_media(&MediaQuery {
            search: s.into(),
            limit: 100,
            ..MediaQuery::default()
        })
        .unwrap()
        .total
    };
    assert_eq!(q("ЛЬВІВ"), 1);
    assert_eq!(q("відео"), 2); // folder name in the path
    assert_eq!(q("13.08.2025"), 1);
    assert_eq!(q("100%"), 0);
}

#[cfg(unix)]
#[test]
fn symlink_loop_in_library_is_skipped() {
    // §18.1
    let mut e = env();
    std::os::unix::fs::symlink(&e.lib, e.lib.join("Фото/loop")).unwrap();
    let p = scan(&mut e, &Helpers::default());
    assert_eq!(p.phase, ScanPhase::Done);
    assert_eq!(all(&e.db, LibrarySection::All).len(), 12);
}

#[test]
fn offline_geobase_names_places_with_city_then_region() {
    use kadr_core::domain::location::{LocationSource, PlaceGranularity};
    use kadr_core::services::geocoder::{GeobaseWriter, OfflineGeocoder};

    let mut e = env();
    let geo_path = e._tmp.path().join("geo.sqlite3");
    let w = GeobaseWriter::create(&geo_path, "test", "test").unwrap();
    w.country("UA", "Україна").unwrap();
    w.admin1("UA.06", "Івано-Франківська область").unwrap();
    w.place(1, 48.44886, 24.55335, "Яремче", "UA", Some("UA.06"), 8094)
        .unwrap();
    w.finish().unwrap();
    let geo = OfflineGeocoder::open(&geo_path).unwrap();

    let roots = vec![e.root.clone()];
    let req = ScanRequest {
        roots: &roots,
        settings: &e.settings,
        exclude: vec![],
    };
    run_scan(
        &mut e.db,
        &req,
        &Helpers::default(),
        Some(&geo),
        &AtomicBool::new(false),
        &mut |_| {},
    )
    .unwrap();
    let rows = all(&e.db, LibrarySection::All);
    let full = by_name(&rows, "exif_full.jpg");
    assert_eq!(
        (full.place_label.as_deref(), full.place_status),
        (Some("Яремче"), PlaceStatus::Resolved)
    );
    assert_eq!(full.location_source, LocationSource::OfflineGeocoder);
    // Far from any town in the geobase: region (like iPhone), never a guess.
    assert_eq!(
        by_name(&rows, "southwest.jpg").place_status,
        PlaceStatus::GpsNoPlace
    );

    // Switching to region level recomputes without reading files.
    e.settings.place_granularity = PlaceGranularity::Region;
    renormalize(&mut e.db, &e.settings, Some(&geo)).unwrap();
    let rows = all(&e.db, LibrarySection::All);
    assert_eq!(
        by_name(&rows, "exif_full.jpg").place_label.as_deref(),
        Some("Івано-Франківська область")
    );
}

#[test]
fn manual_date_replaces_metadata_and_survives_rescan() {
    let Some(helpers) = real_helpers() else {
        return;
    };
    let mut e = env();
    scan(&mut e, &helpers);
    let rows = all(&e.db, LibrarySection::All);
    let no_date = by_name(&rows, "no_metadata.png").id;
    let zero = by_name(&rows, "gps_zero.jpg").id;

    // Invalid input is refused and changes nothing.
    assert!(
        e.db.set_manual_date(&[no_date], Some("31.12.2020"))
            .is_err()
    );
    assert!(e.db.set_manual_place(&[no_date], Some("a/b")).is_err());

    e.db.set_manual_date(&[no_date, zero], Some("2020-12-31T23:30"))
        .unwrap();
    kadr_core::services::indexer::renormalize_items(
        &mut e.db,
        Some(&[no_date, zero]),
        &e.settings,
        None,
    )
    .unwrap();
    let rows = all(&e.db, LibrarySection::All);
    let d = by_name(&rows, "no_metadata.png").captured.clone().unwrap();
    assert_eq!(
        (d.source, d.status),
        (DateSource::Manual, DateStatus::Manual)
    );
    assert_eq!(
        d.local.format("%Y-%m-%d %H:%M").to_string(),
        "2020-12-31 23:30"
    );
    // A manual date does not hide GPS problems.
    assert!(!by_name(&rows, "gps_zero.jpg").review_reasons.is_empty());
    assert!(
        !all(&e.db, LibrarySection::NoDate)
            .iter()
            .any(|r| r.file_name == "no_metadata.png")
    );

    // A rescan that re-reads the file keeps the user's date.
    let p = e.lib.join("Фото/no_metadata.png");
    let mut bytes = fs::read(&p).unwrap();
    bytes.push(0);
    fs::write(&p, bytes).unwrap();
    scan(&mut e, &helpers);
    let rows = all(&e.db, LibrarySection::All);
    assert_eq!(
        by_name(&rows, "no_metadata.png")
            .captured
            .as_ref()
            .unwrap()
            .source,
        DateSource::Manual
    );

    // Clearing it restores what the metadata says (nothing).
    e.db.set_manual_date(&[no_date], None).unwrap();
    kadr_core::services::indexer::renormalize_items(&mut e.db, Some(&[no_date]), &e.settings, None)
        .unwrap();
    assert!(
        by_name(&all(&e.db, LibrarySection::All), "no_metadata.png")
            .captured
            .is_none()
    );
}

#[test]
fn sidecar_and_live_photo_travel_with_their_photo() {
    let Some(helpers) = real_helpers() else {
        return;
    };
    let mut e = env();
    // A Live Photo pair: same name as the photo, a 1-second MOV.
    let dir = e.lib.join("Фото");
    fs::copy(fixtures().join("iphone.mov"), dir.join("IMG_0001.MOV")).unwrap();
    assert_eq!(scan(&mut e, &helpers).phase, ScanPhase::Done);
    let rows = all(&e.db, LibrarySection::All);
    assert!(
        !rows
            .iter()
            .any(|r| r.file_name == "IMG_0001.AAE" || r.file_name == "IMG_0001.MOV")
    );
    let photo = by_name(&rows, "IMG_0001.JPG");
    let names: Vec<&str> = photo
        .companions
        .iter()
        .map(|c| c.file_name.as_str())
        .collect();
    assert_eq!(names, ["IMG_0001.AAE", "IMG_0001.MOV"]);

    // Renaming the photo away breaks the pair on the next scan: the MOV is a
    // normal video again, nothing disappears.
    fs::rename(dir.join("IMG_0001.JPG"), dir.join("other.jpg")).unwrap();
    scan(&mut e, &helpers);
    let rows = all(&e.db, LibrarySection::All);
    assert!(rows.iter().any(|r| r.file_name == "IMG_0001.MOV"));
}

#[test]
fn unknown_duration_never_hides_a_video() {
    let mut e = env();
    fs::copy(
        fixtures().join("iphone.mov"),
        e.lib.join("Фото/IMG_0001.MOV"),
    )
    .unwrap();
    scan(&mut e, &Helpers::default());
    let rows = all(&e.db, LibrarySection::All);
    assert!(rows.iter().any(|r| r.file_name == "IMG_0001.MOV"));
    // The AAE still belongs to the photo.
    assert_eq!(by_name(&rows, "IMG_0001.JPG").companions.len(), 1);
}

#[test]
fn scan_report_explains_every_skipped_file() {
    let mut e = env();
    fs::write(e.lib.join("Фото/notes.txt"), b"x").unwrap();
    fs::write(e.lib.join("Фото/layers.PSD"), b"x").unwrap();
    fs::write(e.lib.join("Фото/more.psd"), b"x").unwrap();
    fs::create_dir_all(e.lib.join("$RECYCLE.BIN")).unwrap();
    fs::write(e.lib.join("$RECYCLE.BIN/old.jpg"), b"\xFF\xD8\xFF").unwrap();
    scan(&mut e, &Helpers::default());
    let r = e.db.scan_report(None).unwrap().unwrap();
    assert_eq!(r.summary.found, 13);
    assert_eq!(r.in_library + r.companions, 13);
    let psd = r
        .not_media_by_extension
        .iter()
        .find(|x| x.extension == "psd")
        .unwrap();
    assert_eq!(psd.count, 2);
    assert!(
        r.not_media_by_extension
            .iter()
            .any(|x| x.extension == "txt")
    );
    assert!(
        r.issues
            .iter()
            .any(|i| i.kind == "system_dir" && i.path.ends_with("$RECYCLE.BIN"))
    );
}
