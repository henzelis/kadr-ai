use serde::{Deserialize, Serialize};

/// GPS position from metadata. Coordinates alone do not name a place.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GeoPoint {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: Option<f64>,
}

impl GeoPoint {
    /// Builds a point only when both coordinates are finite and in range.
    pub fn new(latitude: f64, longitude: f64, altitude: Option<f64>) -> Option<Self> {
        let valid = latitude.is_finite()
            && longitude.is_finite()
            && (-90.0..=90.0).contains(&latitude)
            && (-180.0..=180.0).contains(&longitude);
        valid.then_some(Self {
            latitude,
            longitude,
            altitude: altitude.filter(|a| a.is_finite()),
        })
    }
}

/// Where the place label came from, in the priority order of spec §7.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationSource {
    /// Set by the user; survives rescans.
    Manual,
    /// Explicit city/region text fields in the metadata.
    MetadataText,
    /// Reverse geocoding against the local geobase.
    OfflineGeocoder,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaceStatus {
    Resolved,
    /// GPS is present, but no place name could be determined.
    GpsNoPlace,
    NoGps,
}

/// Language of folder names and place names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    #[default]
    Uk,
    En,
}

/// Which administrative level names the place folder. When the chosen level
/// is unknown for a point, the next broader one is used (city → region →
/// country), the way the iPhone Photos app falls back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaceGranularity {
    #[default]
    City,
    Region,
    Country,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_out_of_range_and_non_finite_coordinates() {
        assert!(GeoPoint::new(48.257, 24.41, None).is_some());
        assert!(GeoPoint::new(91.0, 0.0, None).is_none());
        assert!(GeoPoint::new(0.0, -180.5, None).is_none());
        assert!(GeoPoint::new(f64::NAN, 10.0, None).is_none());
    }

    #[test]
    fn drops_non_finite_altitude_only() {
        let p = GeoPoint::new(50.45, 30.52, Some(f64::INFINITY));
        assert_eq!(p.map(|p| p.altitude), Some(None));
    }
}
