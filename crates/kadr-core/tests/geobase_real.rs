//! Spot checks against a real geobase built by `kadr-geobase`. Runs only when
//! KADR_GEOBASE points to the file:
//! `KADR_GEOBASE=/path/geobase.sqlite3 cargo test -p kadr-core --test geobase_real -- --nocapture`

#![allow(clippy::unwrap_used)] // test helpers panic on purpose

use kadr_core::domain::location::{GeoPoint, Language};
use kadr_core::services::geocoder::OfflineGeocoder;
use kadr_core::services::place::ReverseGeocoder;

#[test]
fn known_places() {
    let Ok(path) = std::env::var("KADR_GEOBASE") else {
        eprintln!("NOTICE: KADR_GEOBASE not set — real geobase checks skipped");
        return;
    };
    let g = OfflineGeocoder::open(std::path::Path::new(&path)).unwrap();
    let cases: &[(&str, f64, f64)] = &[
        ("Яремче, центр", 48.4594, 24.5513),
        ("Говерла", 48.1603, 24.5003),
        ("Київ, Майдан", 50.4501, 30.5234),
        ("Київ, Троєщина", 50.5100, 30.6000),
        ("Львів, Ринок", 49.8419, 24.0315),
        ("Одеса, Ланжерон", 46.4746, 30.7593),
        ("Париж, Ейфелева вежа", 48.8584, 2.2945),
        ("Буенос-Айрес", -34.6037, -58.3816),
        ("Кишинів", 47.0105, 28.8638),
        ("Чорне море", 43.5, 34.0),
        ("Атлантика", 0.0, -30.0),
    ];
    for (label, lat, lon) in cases {
        let p = g.lookup(GeoPoint::new(*lat, *lon, None).unwrap(), Language::Uk);
        let e = g.lookup(GeoPoint::new(*lat, *lon, None).unwrap(), Language::En);
        println!("{label:24} -> {p:?}\n{:24}    {e:?}", "");
    }
    let city = |lat, lon| {
        g.lookup(GeoPoint::new(lat, lon, None).unwrap(), Language::Uk)
            .and_then(|p| p.city)
    };
    assert_eq!(city(48.4594, 24.5513).as_deref(), Some("Яремче"));
    assert_eq!(city(50.4501, 30.5234).as_deref(), Some("Київ"));
    assert_eq!(city(49.8419, 24.0315).as_deref(), Some("Львів"));
    assert_eq!(city(48.8584, 2.2945).as_deref(), Some("Париж"));
    assert!(
        g.lookup(GeoPoint::new(0.0, -30.0, None).unwrap(), Language::Uk)
            .is_none()
    );
}
