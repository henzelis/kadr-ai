//! Place label for a file, in the priority order of spec §7:
//! manual override → explicit text in metadata → offline reverse geocoding →
//! coordinates (only if enabled) → none. Never invents a name.

use serde::{Deserialize, Serialize};

use super::metadata::normalize::PlaceText;
use crate::domain::location::{GeoPoint, Language, LocationSource, PlaceGranularity, PlaceStatus};

/// Administrative names for a point, broadest last. Any level may be missing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GeoPlace {
    pub city: Option<String>,
    pub region: Option<String>,
    pub country: Option<String>,
}

/// Offline reverse geocoder. Implementations never touch the network.
pub trait ReverseGeocoder: Send + Sync {
    /// Version of the geobase; cached results are tied to it.
    fn version(&self) -> &str;
    fn lookup(&self, point: GeoPoint, lang: Language) -> Option<GeoPlace>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedPlace {
    pub label: Option<String>,
    pub source: LocationSource,
    pub status: PlaceStatus,
}

/// Picks the requested level, falling back to broader ones: a photo far from
/// any town gets its region, like the iPhone Photos app does.
pub fn pick_level(
    city: Option<&str>,
    region: Option<&str>,
    country: Option<&str>,
    granularity: PlaceGranularity,
) -> Option<String> {
    let chain: &[Option<&str>] = match granularity {
        PlaceGranularity::City => &[city, region, country],
        PlaceGranularity::Region => &[region, country],
        PlaceGranularity::Country => &[country],
    };
    chain
        .iter()
        .flatten()
        .map(|s| s.trim())
        .find(|s| !s.is_empty())
        .map(str::to_owned)
}

pub struct PlaceInputs<'a> {
    pub manual: Option<&'a str>,
    pub text: &'a PlaceText,
    pub gps: Option<GeoPoint>,
    pub granularity: PlaceGranularity,
    pub coordinates_as_place: bool,
    /// Language of names taken from the geobase.
    pub language: Language,
}

pub fn resolve(inputs: &PlaceInputs<'_>, geocoder: Option<&dyn ReverseGeocoder>) -> ResolvedPlace {
    let gps_status = if inputs.gps.is_some() {
        PlaceStatus::GpsNoPlace
    } else {
        PlaceStatus::NoGps
    };
    if let Some(manual) = inputs.manual.map(str::trim).filter(|m| !m.is_empty()) {
        return ResolvedPlace {
            label: Some(manual.to_owned()),
            source: LocationSource::Manual,
            status: PlaceStatus::Resolved,
        };
    }
    let t = inputs.text;
    if let Some(label) = pick_level(
        t.city.as_deref(),
        t.region.as_deref(),
        t.country.as_deref(),
        inputs.granularity,
    ) {
        return ResolvedPlace {
            label: Some(label),
            source: LocationSource::MetadataText,
            status: PlaceStatus::Resolved,
        };
    }
    if let (Some(point), Some(geo)) = (inputs.gps, geocoder)
        && let Some(found) = geo.lookup(point, inputs.language)
        && let Some(label) = pick_level(
            found.city.as_deref(),
            found.region.as_deref(),
            found.country.as_deref(),
            inputs.granularity,
        )
    {
        return ResolvedPlace {
            label: Some(label),
            source: LocationSource::OfflineGeocoder,
            status: PlaceStatus::Resolved,
        };
    }
    if inputs.coordinates_as_place
        && let Some(p) = inputs.gps
    {
        return ResolvedPlace {
            label: Some(format!("{:.3}, {:.3}", p.latitude, p.longitude)),
            source: LocationSource::None,
            status: PlaceStatus::GpsNoPlace,
        };
    }
    ResolvedPlace {
        label: None,
        source: LocationSource::None,
        status: gps_status,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake(Option<GeoPlace>);
    impl ReverseGeocoder for Fake {
        fn version(&self) -> &str {
            "test"
        }
        fn lookup(&self, _: GeoPoint, _: Language) -> Option<GeoPlace> {
            self.0.clone()
        }
    }

    fn inputs<'a>(
        text: &'a PlaceText,
        gps: Option<GeoPoint>,
        manual: Option<&'a str>,
    ) -> PlaceInputs<'a> {
        PlaceInputs {
            manual,
            text,
            gps,
            granularity: PlaceGranularity::City,
            coordinates_as_place: false,
            language: Language::Uk,
        }
    }

    fn kyiv() -> Option<GeoPoint> {
        GeoPoint::new(50.45, 30.52, None)
    }

    #[test]
    fn granularity_falls_back_to_broader_levels() {
        assert_eq!(
            pick_level(
                None,
                Some("Івано-Франківська область"),
                Some("Україна"),
                PlaceGranularity::City
            )
            .as_deref(),
            Some("Івано-Франківська область")
        );
        assert_eq!(
            pick_level(Some("Яремче"), Some("обл."), None, PlaceGranularity::Region).as_deref(),
            Some("обл.")
        );
        assert_eq!(
            pick_level(Some("  "), None, None, PlaceGranularity::City),
            None
        );
    }

    #[test]
    fn priority_manual_then_text_then_geocoder() {
        let text = PlaceText {
            city: Some("Львів".into()),
            ..PlaceText::default()
        };
        let geo = Fake(Some(GeoPlace {
            city: Some("Київ".into()),
            ..GeoPlace::default()
        }));
        let r = resolve(&inputs(&text, kyiv(), Some("Дача")), Some(&geo));
        assert_eq!(
            (r.label.as_deref(), r.source),
            (Some("Дача"), LocationSource::Manual)
        );
        let r = resolve(&inputs(&text, kyiv(), None), Some(&geo));
        assert_eq!(
            (r.label.as_deref(), r.source),
            (Some("Львів"), LocationSource::MetadataText)
        );
        let empty = PlaceText::default();
        let r = resolve(&inputs(&empty, kyiv(), None), Some(&geo));
        assert_eq!(
            (r.label.as_deref(), r.source),
            (Some("Київ"), LocationSource::OfflineGeocoder)
        );
    }

    #[test]
    fn never_invents_a_place() {
        let empty = PlaceText::default();
        let r = resolve(&inputs(&empty, kyiv(), None), None);
        assert_eq!((r.label, r.status), (None, PlaceStatus::GpsNoPlace));
        let r = resolve(&inputs(&empty, kyiv(), None), Some(&Fake(None)));
        assert_eq!(r.status, PlaceStatus::GpsNoPlace);
        let r = resolve(&inputs(&empty, None, None), None);
        assert_eq!(r.status, PlaceStatus::NoGps);
    }

    #[test]
    fn coordinates_label_only_when_enabled() {
        let empty = PlaceText::default();
        let mut i = inputs(&empty, kyiv(), None);
        i.coordinates_as_place = true;
        let r = resolve(&i, None);
        assert_eq!(
            (r.label.as_deref(), r.status),
            (Some("50.450, 30.520"), PlaceStatus::GpsNoPlace)
        );
    }
}
