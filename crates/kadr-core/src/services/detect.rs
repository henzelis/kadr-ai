//! File type detection. The extension is only a first hint: the first bytes
//! of the file confirm it or reveal the real format.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::domain::media_item::MediaKind;

/// Kind implied by the extension alone (lower-case, without the dot).
pub fn kind_from_extension(ext: &str) -> MediaKind {
    match ext {
        "jpg" | "jpeg" | "jpe" | "jfif" | "png" | "webp" | "gif" | "bmp" | "dib" | "tif"
        | "tiff" | "heic" | "heif" | "hif" | "avif" | "jxl" | "ico" => MediaKind::Photo,
        "dng" | "cr2" | "cr3" | "crw" | "nef" | "nrw" | "arw" | "srf" | "sr2" | "raf" | "orf"
        | "rw2" | "pef" | "srw" | "x3f" | "3fr" | "iiq" | "erf" | "mrw" | "kdc" | "dcr" | "rwl" => {
            MediaKind::RawPhoto
        }
        "mp4" | "mov" | "m4v" | "mkv" | "avi" | "webm" | "3gp" | "3g2" | "mpeg" | "mpg" | "mts"
        | "m2ts" | "m2t" | "ts" | "wmv" | "flv" | "ogv" | "insv" | "mod" | "tod" | "vob"
        | "divx" | "mxf" | "f4v" | "asf" | "qt" | "rm" | "rmvb" | "dv" => MediaKind::Video,
        "aae" | "xmp" | "thm" => MediaKind::Sidecar,
        _ => MediaKind::Unsupported,
    }
}

/// What the file signature says. `None` when the bytes match nothing known,
/// which is normal for many RAW variants and sidecars.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sniffed {
    pub kind: MediaKind,
    /// Short format name, e.g. `JPEG`, `PNG`, `MP4`.
    pub format: &'static str,
}

const SNIFF_LEN: usize = 64;

pub fn sniff_file(path: &Path) -> std::io::Result<Option<Sniffed>> {
    let mut buf = [0_u8; SNIFF_LEN];
    let mut file = File::open(path)?;
    let mut filled = 0;
    while filled < SNIFF_LEN {
        let n = file.read(&mut buf[filled..])?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    Ok(sniff(&buf[..filled]))
}

pub fn sniff(b: &[u8]) -> Option<Sniffed> {
    let photo = |format| {
        Some(Sniffed {
            kind: MediaKind::Photo,
            format,
        })
    };
    let raw = |format| {
        Some(Sniffed {
            kind: MediaKind::RawPhoto,
            format,
        })
    };
    let video = |format| {
        Some(Sniffed {
            kind: MediaKind::Video,
            format,
        })
    };

    if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return photo("JPEG");
    }
    if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        return photo("PNG");
    }
    if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") {
        return photo("GIF");
    }
    if b.starts_with(b"BM") && b.len() >= 14 {
        return photo("BMP");
    }
    if b.starts_with(&[0, 0, 1, 0]) && b.len() >= 6 {
        return photo("ICO");
    }
    if b.starts_with(&[0xFF, 0x0A]) || b.starts_with(b"\0\0\0\x0CJXL \r\n\x87\n") {
        return photo("JXL");
    }
    if b.starts_with(b"FUJIFILMCCD-RAW") {
        return raw("RAF");
    }
    if b.starts_with(b"IIRO") || b.starts_with(b"IIRS") || b.starts_with(b"MMOR") {
        return raw("ORF");
    }
    if b.starts_with(b"IIU\0") {
        return raw("RW2");
    }
    if b.starts_with(b"II*\0") || b.starts_with(b"MM\0*") {
        // Plain TIFF and most TIFF-based RAW formats (DNG, NEF, ARW, CR2...)
        // share this header; the extension decides between them.
        return photo("TIFF");
    }
    if b.len() >= 12 && b.starts_with(b"RIFF") {
        return match &b[8..12] {
            b"WEBP" => photo("WEBP"),
            b"AVI " => video("AVI"),
            _ => None,
        };
    }
    if b.len() >= 12 && &b[4..8] == b"ftyp" {
        return match &b[8..12] {
            b"heic" | b"heix" | b"hevc" | b"heim" | b"heis" | b"mif1" | b"msf1" => photo("HEIF"),
            b"avif" | b"avis" => photo("AVIF"),
            b"crx " => raw("CR3"),
            b"qt  " => video("MOV"),
            b"3gp4" | b"3gp5" | b"3gp6" | b"3g2a" => video("3GP"),
            _ => video("MP4"),
        };
    }
    if b.len() >= 8 && matches!(&b[4..8], b"moov" | b"mdat" | b"wide" | b"free" | b"skip") {
        return video("MOV");
    }
    if b.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        return video("MKV/WEBM");
    }
    if b.starts_with(&[0x30, 0x26, 0xB2, 0x75, 0x8E, 0x66, 0xCF, 0x11]) {
        return video("WMV");
    }
    if b.starts_with(b"FLV") {
        return video("FLV");
    }
    if b.starts_with(b"OggS") {
        return video("OGG");
    }
    if b.starts_with(&[0x00, 0x00, 0x01, 0xBA]) || b.starts_with(&[0x00, 0x00, 0x01, 0xB3]) {
        return video("MPEG");
    }
    // MPEG-TS: sync byte every 188 bytes; M2TS/MTS: every 192 with a 4-byte prefix.
    if b.len() >= 5 && b[0] == 0x47 && (b.len() < 189 || b[188] == 0x47) {
        return video("MPEG-TS");
    }
    if b.len() >= 5 && b[4] == 0x47 {
        return video("M2TS");
    }
    None
}

/// Final kind: the signature wins over the extension, except that TIFF
/// signatures under a RAW extension stay RAW and unknown signatures keep the
/// extension's guess. Returns the kind and whether the two disagreed.
pub fn resolve_kind(by_ext: MediaKind, sniffed: Option<Sniffed>) -> (MediaKind, bool) {
    match sniffed {
        None => (by_ext, false),
        Some(s) if s.format == "TIFF" && by_ext == MediaKind::RawPhoto => (by_ext, false),
        Some(s) if by_ext == MediaKind::Sidecar => (by_ext, s.kind != MediaKind::Sidecar),
        Some(s) => (s.kind, s.kind != by_ext),
    }
}

/// Whether an extension is a normal name for a sniffed format. Used only for
/// a warning: a PNG saved as `.jpg` is still sorted, but the user should know.
pub fn extension_matches(ext: &str, format: &str) -> bool {
    let expected: &[&str] = match format {
        "JPEG" => &["jpg", "jpeg", "jpe", "jfif", "thm"],
        "PNG" => &["png"],
        "GIF" => &["gif"],
        "BMP" => &["bmp", "dib"],
        "ICO" => &["ico"],
        "JXL" => &["jxl"],
        "WEBP" => &["webp"],
        "HEIF" => &["heic", "heif", "hif"],
        "AVIF" => &["avif"],
        "AVI" => &["avi"],
        "MOV" => &["mov", "qt"],
        "MKV/WEBM" => &["mkv", "webm"],
        "WMV" => &["wmv", "asf"],
        "FLV" => &["flv"],
        "OGG" => &["ogv", "ogg"],
        "MPEG" => &["mpg", "mpeg", "vob"],
        "MPEG-TS" | "M2TS" => &["ts", "mts", "m2ts"],
        // TIFF headers cover TIFF and many RAW formats; ISO-BMFF covers a
        // whole family of containers. Too broad to warn about.
        _ => return true,
    };
    expected.contains(&ext) || kind_from_extension(ext) == MediaKind::RawPhoto
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_hints() {
        assert_eq!(kind_from_extension("heic"), MediaKind::Photo);
        assert_eq!(kind_from_extension("nef"), MediaKind::RawPhoto);
        assert_eq!(kind_from_extension("mts"), MediaKind::Video);
        assert_eq!(kind_from_extension("aae"), MediaKind::Sidecar);
        assert_eq!(kind_from_extension("exe"), MediaKind::Unsupported);
    }

    #[test]
    fn signatures() {
        assert_eq!(
            sniff(&[0xFF, 0xD8, 0xFF, 0xE0]).map(|s| s.format),
            Some("JPEG")
        );
        assert_eq!(
            sniff(b"\0\0\0\x18ftypheic\0\0\0\0").map(|s| s.format),
            Some("HEIF")
        );
        assert_eq!(
            sniff(b"\0\0\0\x14ftypqt  \0\0\0\0").map(|s| s.kind),
            Some(MediaKind::Video)
        );
        assert_eq!(
            sniff(b"\0\0\0\x18ftypcrx \0\0\0\0").map(|s| s.kind),
            Some(MediaKind::RawPhoto)
        );
        assert_eq!(
            sniff(b"RIFF\0\0\0\0WEBPVP8 ").map(|s| s.format),
            Some("WEBP")
        );
        assert_eq!(sniff(b"plain text"), None);
        assert_eq!(sniff(&[]), None);
    }

    #[test]
    fn extension_mismatch() {
        assert!(extension_matches("jpg", "JPEG"));
        assert!(!extension_matches("jpg", "PNG"));
        assert!(extension_matches("mp4", "MP4"));
        assert!(extension_matches("nef", "TIFF"));
    }

    #[test]
    fn signature_beats_extension_but_tiff_raw_stays_raw() {
        let png = sniff(b"\x89PNG\r\n\x1a\n");
        assert_eq!(
            resolve_kind(MediaKind::Photo, png),
            (MediaKind::Photo, false)
        );
        let tiff = sniff(b"II*\0\x08\0\0\0");
        assert_eq!(
            resolve_kind(MediaKind::RawPhoto, tiff),
            (MediaKind::RawPhoto, false)
        );
        let mp4 = sniff(b"\0\0\0\x18ftypisom\0\0\0\0");
        assert_eq!(
            resolve_kind(MediaKind::Photo, mp4),
            (MediaKind::Video, true)
        );
        assert_eq!(
            resolve_kind(MediaKind::RawPhoto, None),
            (MediaKind::RawPhoto, false)
        );
    }
}
