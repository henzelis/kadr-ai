//! Thumbnails and previews, made on demand and cached on disk in the app
//! data directory (never next to the originals). Sources, per format:
//! - common raster formats: decoded here (`image`), EXIF orientation applied;
//! - RAW: the JPEG preview embedded by the camera (via ExifTool);
//! - HEIC/HEIF/AVIF/JPEG XL and video: FFmpeg (a frame near the start);
//! - anything else: no thumbnail, with the reason remembered.
//!
//! Generation is bounded by a small semaphore so a fast-scrolling gallery
//! cannot start hundreds of decoders at once.

use std::ffi::OsStr;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageDecoder, ImageReader, metadata::Orientation};
use sha2::{Digest, Sha256};

use super::process;
use crate::domain::media_item::MediaKind;

/// Bump to regenerate every cached thumbnail.
const THUMB_VERSION: u32 = 1;
const JPEG_QUALITY: u8 = 82;
const FFMPEG_TIMEOUT: Duration = Duration::from_secs(45);
const EXIFTOOL_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbSize {
    /// Gallery cards.
    Grid,
    /// Inspector preview.
    Preview,
}

impl ThumbSize {
    pub fn max_side(self) -> u32 {
        match self {
            Self::Grid => 400,
            Self::Preview => 1600,
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "grid" => Some(Self::Grid),
            "preview" => Some(Self::Preview),
            _ => None,
        }
    }

    fn tag(self) -> &'static str {
        match self {
            Self::Grid => "g",
            Self::Preview => "p",
        }
    }
}

/// What the thumbnailer needs to know about one indexed file.
#[derive(Debug, Clone)]
pub struct ThumbSource {
    pub path: PathBuf,
    pub kind: MediaKind,
    pub extension: String,
    pub size_bytes: u64,
    pub mtime_ms: i64,
    pub duration_ms: Option<u64>,
    /// EXIF orientation 1..=8 of the original, for embedded previews.
    pub orientation: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThumbError {
    /// No decoder for this format on this machine.
    Unsupported,
    /// The file could not be decoded.
    Failed(String),
}

impl std::fmt::Display for ThumbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported => write!(f, "no thumbnail decoder for this format"),
            Self::Failed(m) => write!(f, "{m}"),
        }
    }
}

/// Counting semaphore (std only).
struct Permits {
    free: Mutex<usize>,
    cv: Condvar,
}

impl Permits {
    fn acquire(&self) -> PermitGuard<'_> {
        let mut free = self
            .free
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while *free == 0 {
            free = self
                .cv
                .wait(free)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        *free -= 1;
        PermitGuard(self)
    }
}

struct PermitGuard<'a>(&'a Permits);

impl Drop for PermitGuard<'_> {
    fn drop(&mut self) {
        let mut free = self
            .0
            .free
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *free += 1;
        self.0.cv.notify_one();
    }
}

pub struct Thumbnailer {
    cache_dir: PathBuf,
    exiftool: Option<PathBuf>,
    ffmpeg: Option<PathBuf>,
    permits: Permits,
}

/// Formats decoded in-process.
fn native_format(ext: &str) -> bool {
    matches!(
        ext,
        "jpg"
            | "jpeg"
            | "jpe"
            | "jfif"
            | "png"
            | "gif"
            | "bmp"
            | "dib"
            | "tif"
            | "tiff"
            | "webp"
            | "ico"
    )
}

impl Thumbnailer {
    pub fn new(
        cache_dir: PathBuf,
        exiftool: Option<PathBuf>,
        ffmpeg: Option<PathBuf>,
        workers: usize,
    ) -> Self {
        Self {
            cache_dir,
            exiftool,
            ffmpeg,
            permits: Permits {
                free: Mutex::new(workers.max(1)),
                cv: Condvar::new(),
            },
        }
    }

    fn key(&self, src: &ThumbSource, size: ThumbSize) -> String {
        let mut h = Sha256::new();
        h.update(src.path.to_string_lossy().as_bytes());
        h.update(
            format!(
                "|{}|{}|v{THUMB_VERSION}|{}",
                src.size_bytes,
                src.mtime_ms,
                size.tag()
            )
            .as_bytes(),
        );
        hex::encode(&h.finalize()[..16])
    }

    fn paths(&self, key: &str) -> (PathBuf, PathBuf) {
        let dir = self.cache_dir.join(&key[..2]);
        (
            dir.join(format!("{key}.jpg")),
            dir.join(format!("{key}.fail")),
        )
    }

    /// Which decoders exist now; a failure remembered under different
    /// decoders is retried.
    fn decoders_fingerprint(&self) -> String {
        format!(
            "exiftool={} ffmpeg={}",
            self.exiftool.is_some(),
            self.ffmpeg.is_some()
        )
    }

    /// Returns the cached JPEG, generating it first if needed.
    pub fn get(&self, src: &ThumbSource, size: ThumbSize) -> Result<PathBuf, ThumbError> {
        let key = self.key(src, size);
        let (jpg, fail) = self.paths(&key);
        if jpg.is_file() {
            return Ok(jpg);
        }
        if let Ok(note) = fs::read_to_string(&fail)
            && let Some((fingerprint, reason)) = note.split_once('\n')
            && fingerprint == self.decoders_fingerprint()
        {
            return Err(if reason == "unsupported" {
                ThumbError::Unsupported
            } else {
                ThumbError::Failed(reason.to_owned())
            });
        }
        let _permit = self.permits.acquire();
        // Another request may have produced it while we waited.
        if jpg.is_file() {
            return Ok(jpg);
        }
        let result = self
            .render(src, size.max_side())
            .and_then(|img| write_jpeg(&img, &jpg));
        if let Err(err) = &result {
            let reason = match err {
                ThumbError::Unsupported => "unsupported".to_owned(),
                ThumbError::Failed(m) => m.replace('\n', " "),
            };
            if let Some(dir) = fail.parent() {
                let _ = fs::create_dir_all(dir);
            }
            let _ = fs::write(&fail, format!("{}\n{reason}", self.decoders_fingerprint()));
        }
        result.map(|()| jpg)
    }

    fn render(&self, src: &ThumbSource, side: u32) -> Result<DynamicImage, ThumbError> {
        let ext = src.extension.as_str();
        match src.kind {
            MediaKind::Sidecar | MediaKind::Unsupported => Err(ThumbError::Unsupported),
            MediaKind::Video => self.ffmpeg_frame(src, side),
            MediaKind::RawPhoto => self
                .embedded_preview(src)
                .map(|img| shrink(img, side))
                .or_else(|_| self.ffmpeg_frame(src, side)),
            MediaKind::Photo if native_format(ext) => {
                decode_native(&src.path)
                    .map(|img| shrink(img, side))
                    .or_else(|e| {
                        // A mislabelled file may still be something FFmpeg reads.
                        self.ffmpeg_frame(src, side).map_err(|_| e)
                    })
            }
            MediaKind::Photo => self.ffmpeg_frame(src, side),
        }
    }

    /// Largest JPEG the camera embedded in a RAW file, oriented like the
    /// original.
    fn embedded_preview(&self, src: &ThumbSource) -> Result<DynamicImage, ThumbError> {
        let tool = self.exiftool.as_ref().ok_or(ThumbError::Unsupported)?;
        for tag in [
            "-JpgFromRaw",
            "-PreviewImage",
            "-OtherImage",
            "-ThumbnailImage",
        ] {
            let args = [OsStr::new("-b"), OsStr::new(tag), src.path.as_os_str()];
            let Ok(out) = process::run(tool, &args, None, EXIFTOOL_TIMEOUT) else {
                continue;
            };
            if out.stdout.len() < 1024 {
                continue;
            }
            if let Ok(mut img) = image::load_from_memory(&out.stdout) {
                if let Some(o) = src.orientation.and_then(Orientation::from_exif) {
                    img.apply_orientation(o);
                }
                return Ok(img);
            }
        }
        Err(ThumbError::Failed("no embedded preview".to_owned()))
    }

    /// One frame via FFmpeg, already scaled. FFmpeg applies rotation metadata.
    fn ffmpeg_frame(&self, src: &ThumbSource, side: u32) -> Result<DynamicImage, ThumbError> {
        let tool = self.ffmpeg.as_ref().ok_or(ThumbError::Unsupported)?;
        // Quotes protect the commas inside min() from the filtergraph parser.
        let scale = format!(
            "scale=w='min(iw,{side})':h='min(ih,{side})':force_original_aspect_ratio=decrease"
        );
        let grab = |seek: Option<String>| -> Result<DynamicImage, ThumbError> {
            let mut args: Vec<&OsStr> = vec![
                OsStr::new("-hide_banner"),
                OsStr::new("-loglevel"),
                OsStr::new("error"),
            ];
            if let Some(s) = &seek {
                args.extend([OsStr::new("-ss"), OsStr::new(s.as_str())]);
            }
            args.extend([
                OsStr::new("-i"),
                src.path.as_os_str(),
                OsStr::new("-frames:v"),
                OsStr::new("1"),
                OsStr::new("-vf"),
                OsStr::new(scale.as_str()),
                OsStr::new("-f"),
                OsStr::new("image2pipe"),
                OsStr::new("-c:v"),
                OsStr::new("png"),
                OsStr::new("-"),
            ]);
            let out = process::run(tool, &args, None, FFMPEG_TIMEOUT)
                .map_err(|e| ThumbError::Failed(e.to_string()))?;
            if !out.status.success() || out.stdout.is_empty() {
                let msg = String::from_utf8_lossy(&out.stderr);
                return Err(ThumbError::Failed(
                    msg.lines()
                        .last()
                        .unwrap_or("ffmpeg failed")
                        .trim()
                        .to_owned(),
                ));
            }
            image::load_from_memory(&out.stdout).map_err(|e| ThumbError::Failed(e.to_string()))
        };
        if src.kind == MediaKind::Video {
            // A frame one second in avoids black fade-ins; short clips use the middle.
            let at_ms = src.duration_ms.map_or(1000, |d| (d / 2).min(1000));
            grab(Some(format!("{:.3}", at_ms as f64 / 1000.0))).or_else(|_| grab(None))
        } else {
            grab(None)
        }
    }
}

/// Downscale only: small pictures keep their size.
fn shrink(img: DynamicImage, side: u32) -> DynamicImage {
    if img.width() <= side && img.height() <= side {
        img
    } else {
        img.thumbnail(side, side)
    }
}

fn decode_native(path: &Path) -> Result<DynamicImage, ThumbError> {
    let fail = |e: &dyn std::fmt::Display| ThumbError::Failed(e.to_string());
    let reader = ImageReader::open(path)
        .map_err(|e| fail(&e))?
        .with_guessed_format()
        .map_err(|e| fail(&e))?;
    let mut decoder = reader.into_decoder().map_err(|e| fail(&e))?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut img = DynamicImage::from_decoder(decoder).map_err(|e| fail(&e))?;
    img.apply_orientation(orientation);
    Ok(img)
}

fn write_jpeg(img: &DynamicImage, target: &Path) -> Result<(), ThumbError> {
    let fail = |e: &dyn std::fmt::Display| ThumbError::Failed(e.to_string());
    let rgb = img.to_rgb8();
    let mut buf = Cursor::new(Vec::new());
    JpegEncoder::new_with_quality(&mut buf, JPEG_QUALITY)
        .encode_image(&rgb)
        .map_err(|e| fail(&e))?;
    let dir = target
        .parent()
        .ok_or_else(|| ThumbError::Failed("bad cache path".to_owned()))?;
    fs::create_dir_all(dir).map_err(|e| fail(&e))?;
    let tmp = target.with_extension("tmp");
    fs::write(&tmp, buf.into_inner()).map_err(|e| fail(&e))?;
    fs::rename(&tmp, target).map_err(|e| fail(&e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GenericImageView, ImageBuffer, Rgb};

    fn source(path: PathBuf, kind: MediaKind, ext: &str) -> ThumbSource {
        ThumbSource {
            size_bytes: fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
            path,
            kind,
            extension: ext.to_owned(),
            mtime_ms: 1,
            duration_ms: None,
            orientation: None,
        }
    }

    /// 300x100 image, left half red, right half blue.
    fn sample_jpeg(dir: &Path, name: &str, orientation: Option<u16>) -> PathBuf {
        let img = ImageBuffer::from_fn(300, 100, |x, _| {
            if x < 150 {
                Rgb([255, 0, 0])
            } else {
                Rgb([0, 0, 255])
            }
        });
        let path = dir.join(name);
        DynamicImage::ImageRgb8(img).save(&path).unwrap();
        if let Some(o) = orientation {
            // Write the EXIF orientation with ExifTool when it is available.
            let _ = std::process::Command::new("exiftool")
                .args([
                    "-q",
                    "-overwrite_original",
                    "-n",
                    &format!("-Orientation={o}"),
                ])
                .arg(&path)
                .status();
        }
        path
    }

    #[test]
    fn makes_cached_grid_thumbnail_for_jpeg() {
        let dir = tempfile::tempdir().unwrap();
        let src = source(
            sample_jpeg(dir.path(), "a.jpg", None),
            MediaKind::Photo,
            "jpg",
        );
        let t = Thumbnailer::new(dir.path().join("cache"), None, None, 2);
        let out = t.get(&src, ThumbSize::Grid).unwrap();
        let img = image::open(&out).unwrap();
        assert_eq!(
            img.dimensions(),
            (300, 100),
            "small images are not upscaled"
        );
        // Second call is a cache hit (same file).
        assert_eq!(t.get(&src, ThumbSize::Grid).unwrap(), out);
    }

    #[test]
    fn large_images_are_scaled_to_the_requested_side() {
        let dir = tempfile::tempdir().unwrap();
        let big = ImageBuffer::from_pixel(2000, 1000, Rgb([10u8, 200, 10]));
        let path = dir.path().join("big.png");
        DynamicImage::ImageRgb8(big).save(&path).unwrap();
        let t = Thumbnailer::new(dir.path().join("cache"), None, None, 1);
        let out = t
            .get(&source(path, MediaKind::Photo, "png"), ThumbSize::Grid)
            .unwrap();
        assert_eq!(image::open(out).unwrap().dimensions(), (400, 200));
    }

    #[test]
    fn exif_orientation_is_applied() {
        if std::process::Command::new("exiftool")
            .arg("-ver")
            .output()
            .is_err()
        {
            eprintln!("NOTICE: exiftool missing — orientation test skipped");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        // Orientation 6 = rotate 90° clockwise: 300x100 becomes 100x300 and
        // the red half ends up on top.
        let src = source(
            sample_jpeg(dir.path(), "r.jpg", Some(6)),
            MediaKind::Photo,
            "jpg",
        );
        let t = Thumbnailer::new(dir.path().join("cache"), None, None, 1);
        let img = image::open(t.get(&src, ThumbSize::Grid).unwrap())
            .unwrap()
            .to_rgb8();
        assert_eq!(img.dimensions(), (100, 300));
        let top = img.get_pixel(50, 20);
        assert!(
            top[0] > 200 && top[2] < 60,
            "top should be red, got {top:?}"
        );
    }

    #[test]
    fn unsupported_and_broken_files_are_remembered() {
        let dir = tempfile::tempdir().unwrap();
        let heic = dir.path().join("x.heic");
        fs::write(&heic, b"not really").unwrap();
        let t = Thumbnailer::new(dir.path().join("cache"), None, None, 1);
        let src = source(heic, MediaKind::Photo, "heic");
        assert_eq!(t.get(&src, ThumbSize::Grid), Err(ThumbError::Unsupported));
        assert_eq!(t.get(&src, ThumbSize::Grid), Err(ThumbError::Unsupported));

        let bad = dir.path().join("bad.jpg");
        fs::write(&bad, b"\xFF\xD8\xFF\xE0garbage").unwrap();
        assert!(matches!(
            t.get(&source(bad, MediaKind::Photo, "jpg"), ThumbSize::Grid),
            Err(ThumbError::Failed(_))
        ));
    }

    #[test]
    fn changed_file_gets_a_new_thumbnail() {
        let dir = tempfile::tempdir().unwrap();
        let path = sample_jpeg(dir.path(), "c.jpg", None);
        let t = Thumbnailer::new(dir.path().join("cache"), None, None, 1);
        let mut src = source(path, MediaKind::Photo, "jpg");
        let first = t.get(&src, ThumbSize::Grid).unwrap();
        src.mtime_ms = 2;
        assert_ne!(t.get(&src, ThumbSize::Grid).unwrap(), first);
    }

    #[test]
    fn video_frame_via_ffmpeg_when_available() {
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/media/android.mp4");
        let ffmpeg = PathBuf::from(if cfg!(windows) {
            "ffmpeg.exe"
        } else {
            "ffmpeg"
        });
        if std::process::Command::new(&ffmpeg)
            .arg("-version")
            .output()
            .is_err()
        {
            eprintln!("NOTICE: ffmpeg missing — video thumbnail test skipped");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let t = Thumbnailer::new(dir.path().join("cache"), None, Some(ffmpeg), 1);
        let mut src = source(fixture, MediaKind::Video, "mp4");
        src.duration_ms = Some(1000);
        let img = image::open(t.get(&src, ThumbSize::Grid).unwrap()).unwrap();
        assert_eq!(img.dimensions(), (96, 64));
    }
}
