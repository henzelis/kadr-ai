//! HEIC/AVIF thumbnails through a real FFmpeg (7.1+). Runs only when
//! KADR_FFMPEG points to ffmpeg and KADR_SAMPLES to a folder with
//! `example.heic` and `example.avif` (e.g. from the libheif repository).

#![allow(clippy::unwrap_used)] // test helpers panic on purpose

use std::path::PathBuf;

use kadr_core::domain::media_item::MediaKind;
use kadr_core::services::thumbnail::{ThumbSize, ThumbSource, Thumbnailer};

#[test]
fn heic_and_avif_through_ffmpeg() {
    let (Ok(ffmpeg), Ok(samples)) = (std::env::var("KADR_FFMPEG"), std::env::var("KADR_SAMPLES"))
    else {
        eprintln!("NOTICE: KADR_FFMPEG/KADR_SAMPLES not set — HEIC/AVIF check skipped");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let t = Thumbnailer::new(
        dir.path().to_path_buf(),
        None,
        Some(PathBuf::from(ffmpeg)),
        2,
    );
    for (name, ext) in [("example.heic", "heic"), ("example.avif", "avif")] {
        let path = PathBuf::from(&samples).join(name);
        let src = ThumbSource {
            size_bytes: std::fs::metadata(&path).unwrap().len(),
            path,
            kind: MediaKind::Photo,
            extension: ext.to_owned(),
            mtime_ms: 1,
            duration_ms: None,
            orientation: None,
        };
        let out = t.get(&src, ThumbSize::Grid).unwrap();
        let img = image::open(&out).unwrap();
        println!("{name}: {}x{}", img.width(), img.height());
        assert_eq!(img.width().max(img.height()), 400);
    }
}
