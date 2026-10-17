//! Application settings, stored as one JSON document in `app_settings`.
//! Unknown or missing fields fall back to defaults, so older and newer
//! builds can read each other's settings.

use serde::{Deserialize, Serialize};

use crate::domain::location::PlaceGranularity;
use crate::domain::operation::ConflictPolicy;
use crate::domain::sort_template::{DEFAULT_TEMPLATE, MonthFormat};
use crate::errors::{CoreError, CoreResult};

/// Upper bound for helper and thumbnail parallelism.
pub const MAX_WORKERS: u8 = 32;

/// Longest folder label a user may type (one path segment).
const MAX_LABEL_CHARS: usize = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub folder_template: String,
    pub month_format: MonthFormat,
    pub unknown_date_label: String,
    pub unknown_place_label: String,
    pub place_granularity: PlaceGranularity,
    /// Name a folder by raw coordinates when GPS exists but no place is known.
    /// Off by default: it would create one folder per spot.
    pub coordinates_as_place: bool,
    /// Use the file modification time when no capture date exists. Such dates
    /// are always marked as file-system dates.
    pub filesystem_date_fallback: bool,
    /// Use the local geobase for reverse geocoding (no network involved).
    pub offline_geocoding: bool,
    /// Any network access at all. Off by default and not exposed in stage A.
    pub network_enabled: bool,
    pub conflict_policy: ConflictPolicy,
    pub follow_symlinks: bool,
    pub skip_system_dirs: bool,
    pub metadata_workers: u8,
    pub thumbnail_workers: u8,
    /// Custom helper locations; `None` means "look on PATH / next to the app".
    pub exiftool_path: Option<String>,
    pub ffprobe_path: Option<String>,
    pub theme: Theme,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            folder_template: DEFAULT_TEMPLATE.to_owned(),
            month_format: MonthFormat::default(),
            unknown_date_label: "Невідома дата".to_owned(),
            unknown_place_label: "Без геолокації".to_owned(),
            place_granularity: PlaceGranularity::default(),
            coordinates_as_place: false,
            filesystem_date_fallback: false,
            offline_geocoding: true,
            network_enabled: false,
            conflict_policy: ConflictPolicy::default(),
            follow_symlinks: false,
            skip_system_dirs: true,
            metadata_workers: 4,
            thumbnail_workers: 2,
            exiftool_path: None,
            ffprobe_path: None,
            theme: Theme::default(),
        }
    }
}

impl AppSettings {
    /// Rejects values that would produce unsafe or nonsensical folders.
    /// Full template validation comes with the template parser (stage D).
    pub fn validate(&self) -> CoreResult<()> {
        validate_label("unknown_date_label", &self.unknown_date_label)?;
        validate_label("unknown_place_label", &self.unknown_place_label)?;
        validate_workers("metadata_workers", self.metadata_workers)?;
        validate_workers("thumbnail_workers", self.thumbnail_workers)?;
        if !self.folder_template.contains("{filename}") {
            return Err(CoreError::InvalidSetting {
                field: "folder_template",
                reason: "template must end with {filename}".to_owned(),
            });
        }
        if self.network_enabled {
            return Err(CoreError::InvalidSetting {
                field: "network_enabled",
                reason: "network features are not available in this build".to_owned(),
            });
        }
        Ok(())
    }
}

fn validate_label(field: &'static str, value: &str) -> CoreResult<()> {
    let trimmed = value.trim();
    let reason = if trimmed.is_empty() {
        Some("must not be empty")
    } else if trimmed.chars().count() > MAX_LABEL_CHARS {
        Some("is too long")
    } else if value != trimmed || trimmed.ends_with('.') {
        Some("must not start or end with spaces or end with a dot")
    } else if is_windows_reserved(trimmed) {
        Some("is a reserved device name on Windows")
    } else if trimmed
        .chars()
        .any(|c| matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control())
    {
        Some("contains characters not allowed in folder names")
    } else {
        None
    };
    match reason {
        Some(reason) => Err(CoreError::InvalidSetting {
            field,
            reason: reason.to_owned(),
        }),
        None => Ok(()),
    }
}

/// `CON`, `NUL`, `COM1`... are device names on Windows, also with an extension.
fn is_windows_reserved(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0')
}

fn validate_workers(field: &'static str, value: u8) -> CoreResult<()> {
    if (1..=MAX_WORKERS).contains(&value) {
        Ok(())
    } else {
        Err(CoreError::InvalidSetting {
            field,
            reason: format!("must be between 1 and {MAX_WORKERS}"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_safe_and_valid() {
        let s = AppSettings::default();
        s.validate().unwrap();
        assert!(!s.network_enabled);
        assert!(!s.filesystem_date_fallback);
        assert!(!s.follow_symlinks);
        assert_eq!(s.conflict_policy, ConflictPolicy::AddSuffix);
        assert_eq!(s.place_granularity, PlaceGranularity::City);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let s: AppSettings = serde_json::from_str(r#"{"theme":"dark","future_field":1}"#).unwrap();
        assert_eq!(s.theme, Theme::Dark);
        assert_eq!(s.unknown_place_label, "Без геолокації");
    }

    #[test]
    fn rejects_unsafe_labels() {
        for bad in [
            "", "   ", "..", ".", " x", "x.", "a/b", "a\\b", "C:", "x\u{7}", "CON", "nul.txt", "COM1",
        ] {
            let s = AppSettings {
                unknown_place_label: bad.to_owned(),
                ..AppSettings::default()
            };
            assert!(s.validate().is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn accepts_ordinary_labels() {
        for ok in ["Без геолокації", "Unknown place", "COM10", "Console", "2025 літо"] {
            let s = AppSettings {
                unknown_place_label: ok.to_owned(),
                ..AppSettings::default()
            };
            assert!(s.validate().is_ok(), "rejected {ok:?}");
        }
    }

    #[test]
    fn rejects_worker_limits_out_of_range() {
        let s = AppSettings {
            metadata_workers: 0,
            ..AppSettings::default()
        };
        assert!(s.validate().is_err());
        let s = AppSettings {
            thumbnail_workers: MAX_WORKERS + 1,
            ..AppSettings::default()
        };
        assert!(s.validate().is_err());
    }

    #[test]
    fn network_cannot_be_switched_on_yet() {
        let s = AppSettings {
            network_enabled: true,
            ..AppSettings::default()
        };
        assert!(s.validate().is_err());
    }
}
