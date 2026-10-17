//! Built-in EXIF reader (kamadak-exif) for JPEG, TIFF-based RAW, HEIF/AVIF,
//! PNG and WebP. Used when ExifTool is not available, so common photos still
//! get dates and GPS. Writes the same tag names ExifTool would.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use exif::{Exif, Field, In, Reader, Tag, Value};
use serde_json::json;

use super::{MetadataProvider, RawMetadata, ReadError, Tags};

pub const ID: &str = "native-exif 0.6";

pub struct NativeExif;

fn ascii(field: Option<&Field>) -> Option<String> {
    match &field?.value {
        Value::Ascii(parts) => parts
            .first()
            .map(|b| String::from_utf8_lossy(b).trim().to_owned())
            .filter(|s| !s.is_empty()),
        _ => None,
    }
}

fn uint(field: Option<&Field>) -> Option<u32> {
    field?.value.get_uint(0)
}

/// Degrees/minutes/seconds rationals → signed decimal degrees.
fn dms(exif: &Exif, value: Tag, reference: Tag, negative: &[u8]) -> Option<f64> {
    let Value::Rational(parts) = &exif.get_field(value, In::PRIMARY)?.value else {
        return None;
    };
    let [d, m, s] = parts.as_slice() else {
        return None;
    };
    let degrees = d.to_f64() + m.to_f64() / 60.0 + s.to_f64() / 3600.0;
    let negative = ascii(exif.get_field(reference, In::PRIMARY))
        .is_some_and(|r| r.as_bytes().first().is_some_and(|c| negative.contains(c)));
    degrees
        .is_finite()
        .then_some(if negative { -degrees } else { degrees })
}

fn to_tags(exif: &Exif) -> Tags {
    let mut tags = Tags::new();
    let field = |t| exif.get_field(t, In::PRIMARY);
    let text = [
        (Tag::DateTimeOriginal, "ExifIFD:DateTimeOriginal"),
        (Tag::OffsetTimeOriginal, "ExifIFD:OffsetTimeOriginal"),
        (Tag::DateTimeDigitized, "ExifIFD:CreateDate"),
        (Tag::OffsetTimeDigitized, "ExifIFD:OffsetTimeDigitized"),
        (Tag::Make, "IFD0:Make"),
        (Tag::Model, "IFD0:Model"),
    ];
    for (tag, key) in text {
        if let Some(v) = ascii(field(tag)) {
            tags.insert(key.to_owned(), json!(v));
        }
    }
    if let Some(o) = uint(field(Tag::Orientation)) {
        tags.insert("IFD0:Orientation".to_owned(), json!(o));
    }
    if let Some(w) = uint(field(Tag::PixelXDimension)) {
        tags.insert("ExifIFD:ExifImageWidth".to_owned(), json!(w));
    }
    if let Some(h) = uint(field(Tag::PixelYDimension)) {
        tags.insert("ExifIFD:ExifImageHeight".to_owned(), json!(h));
    }
    let lat = dms(exif, Tag::GPSLatitude, Tag::GPSLatitudeRef, b"Ss");
    let lon = dms(exif, Tag::GPSLongitude, Tag::GPSLongitudeRef, b"Ww");
    if let (Some(lat), Some(lon)) = (lat, lon) {
        tags.insert("Composite:GPSLatitude".to_owned(), json!(lat));
        tags.insert("Composite:GPSLongitude".to_owned(), json!(lon));
        if let Some(Field {
            value: Value::Rational(alt),
            ..
        }) = field(Tag::GPSAltitude)
            && let Some(a) = alt.first()
        {
            let below_sea = uint(field(Tag::GPSAltitudeRef)) == Some(1);
            let a = a.to_f64();
            tags.insert(
                "Composite:GPSAltitude".to_owned(),
                json!(if below_sea { -a } else { a }),
            );
        }
    }
    tags
}

impl NativeExif {
    fn read(path: &Path) -> Result<RawMetadata, ReadError> {
        let file = File::open(path).map_err(|e| ReadError::Unreadable(e.to_string()))?;
        let mut reader = BufReader::new(file);
        match Reader::new().read_from_container(&mut reader) {
            Ok(exif) => Ok(RawMetadata {
                tags: to_tags(&exif),
                providers: vec![ID.to_owned()],
                warnings: Vec::new(),
            }),
            // "No Exif data" is a normal answer: the file is fine, it just
            // carries no metadata.
            Err(exif::Error::NotFound(_)) => Ok(RawMetadata {
                tags: Tags::new(),
                providers: vec![ID.to_owned()],
                warnings: Vec::new(),
            }),
            Err(e) => Err(ReadError::Unreadable(e.to_string())),
        }
    }
}

impl MetadataProvider for NativeExif {
    fn id(&self) -> &str {
        ID
    }

    fn read_batch(&self, paths: &[PathBuf]) -> Vec<Result<RawMetadata, ReadError>> {
        paths.iter().map(|p| Self::read(p)).collect()
    }
}
