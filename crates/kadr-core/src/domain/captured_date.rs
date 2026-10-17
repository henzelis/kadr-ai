use chrono::{Datelike, NaiveDateTime};
use serde::{Deserialize, Serialize};

/// Where a capture date came from, in the priority order of spec §6.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateSource {
    /// EXIF `DateTimeOriginal`, with `OffsetTimeOriginal` when present.
    ExifOriginal,
    ExifOther,
    Xmp,
    Iptc,
    /// QuickTime / container timestamps; often UTC, see `CapturedDate::offset_minutes`.
    QuickTime,
    HelperOther,
    /// File modification time. Only used when the user enabled this fallback.
    FileSystem,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateStatus {
    Confirmed,
    /// Taken from the file system; the UI must say it is not a capture time.
    FileSystemFallback,
    /// Unparsable or contradicting sources.
    NeedsReview,
    Manual,
}

/// A capture date with its provenance. `None` on the media item means the
/// date is unknown; Kadr never invents one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapturedDate {
    /// The value exactly as the metadata helper returned it.
    pub raw: String,
    /// Wall-clock time at the place of capture. Folders are built from this.
    pub local: NaiveDateTime,
    /// UTC offset in minutes when known. A date without an offset is *not*
    /// treated as UTC.
    pub offset_minutes: Option<i32>,
    pub source: DateSource,
    pub status: DateStatus,
    /// How the raw value was normalized, for the inspector.
    pub note: Option<super::note::Note>,
}

impl CapturedDate {
    pub fn year(&self) -> i32 {
        self.local.year()
    }

    /// Month number 1..=12.
    pub fn month(&self) -> u32 {
        self.local.month()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn date(y: i32, m: u32, d: u32, h: u32) -> CapturedDate {
        let local = NaiveDate::from_ymd_opt(y, m, d)
            .and_then(|d| d.and_hms_opt(h, 30, 0))
            .unwrap_or_default();
        CapturedDate {
            raw: format!("{y:04}:{m:02}:{d:02} {h:02}:30:00"),
            local,
            offset_minutes: None,
            source: DateSource::ExifOriginal,
            status: DateStatus::Confirmed,
            note: None,
        }
    }

    #[test]
    fn year_and_month_come_from_local_capture_time() {
        // 31 Dec 23:30 local stays in December even if it would be January in UTC
        // for a negative offset; without an offset nothing is converted at all.
        let d = date(2025, 12, 31, 23);
        assert_eq!((d.year(), d.month()), (2025, 12));
    }

    #[test]
    fn serializes_with_snake_case_enums() {
        let json = serde_json::to_value(date(2025, 8, 13, 16)).unwrap_or_default();
        assert_eq!(json["source"], "exif_original");
        assert_eq!(json["status"], "confirmed");
        assert!(json["offset_minutes"].is_null());
    }
}
