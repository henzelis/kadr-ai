use serde::{Deserialize, Serialize};

use super::captured_date::CapturedDate;
use super::location::{GeoPoint, LocationSource, PlaceStatus};

/// What a file is, as far as Kadr can tell. The extension is only a first hint;
/// stage B confirms it from the signature and metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Photo,
    RawPhoto,
    Video,
    /// Companion file (AAE, XMP, live-photo MOV) that travels with its parent.
    Sidecar,
    Unsupported,
}

impl MediaKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Photo => "photo",
            Self::RawPhoto => "raw_photo",
            Self::Video => "video",
            Self::Sidecar => "sidecar",
            Self::Unsupported => "unsupported",
        }
    }
}

/// How far metadata extraction got. Reading metadata, making a thumbnail and
/// fully decoding a format are separate capabilities and are tracked apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataStatus {
    NotRead,
    Read,
    Partial,
    /// No helper could read this file; the reason is in `last_error`.
    Unavailable,
    Failed,
}

/// One indexed file. Mirrors the `media_item` table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaItem {
    pub id: i64,
    pub source_root_id: i64,
    pub path: String,
    pub file_name: String,
    pub extension: String,
    pub kind: MediaKind,
    pub size_bytes: u64,
    /// Modification time, Unix milliseconds.
    pub mtime_ms: i64,
    pub sha256: Option<String>,
    pub detected_type: Option<String>,
    pub metadata_status: MetadataStatus,
    pub captured: Option<CapturedDate>,
    pub gps: Option<GeoPoint>,
    pub place_label: Option<String>,
    pub location_source: LocationSource,
    pub place_status: PlaceStatus,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration_ms: Option<u64>,
    pub last_error: Option<String>,
    pub sidecar_parent_id: Option<i64>,
}
