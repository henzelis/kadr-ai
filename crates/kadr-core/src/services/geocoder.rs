//! Offline reverse geocoding against a Kadr geobase file (built from GeoNames
//! by `kadr-geobase`). Everything is loaded into memory and searched through a
//! coarse grid; no network is ever used.
//!
//! Picking a name for a point, like the iPhone Photos app:
//! - the closest town whose "reach" (grows with population) covers the point;
//!   a "town" lying deep inside a bigger city (Paris 16 Passy in Paris) counts
//!   as that city;
//! - otherwise the region (oblast) of the nearest place within 60 km;
//! - otherwise the country of the nearest place within 80 km;
//! - otherwise nothing: the point stays "GPS present, place unknown".

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::Serialize;

use super::place::{GeoPlace, ReverseGeocoder};
use crate::domain::location::GeoPoint;
use crate::errors::{CoreError, CoreResult};

const CELL_DEG: f64 = 0.25;
const REGION_RADIUS_KM: f64 = 60.0;
const COUNTRY_RADIUS_KM: f64 = 80.0;
/// A place closer than this share of a bigger city's reach is part of it.
const ABSORB_SHARE: f64 = 0.5;
const EARTH_RADIUS_KM: f64 = 6371.0;

/// Geobase schema version this build reads.
pub const GEOBASE_FORMAT: &str = "1";

#[derive(Debug, Clone, Serialize)]
pub struct GeobaseInfo {
    pub version: String,
    pub source: String,
    pub places: u64,
    pub built_at: String,
}

struct Place {
    lat: f64,
    lon: f64,
    name: String,
    country: u16,
    admin1: Option<u32>,
    population: u64,
    reach_km: f64,
}

pub struct OfflineGeocoder {
    info: GeobaseInfo,
    places: Vec<Place>,
    grid: HashMap<(i32, i32), Vec<u32>>,
    countries: Vec<String>,
    admins: Vec<String>,
}

/// How far from its centre a town still names a photo: 4 km for a village,
/// about 25 km for a city of millions.
pub fn reach_km(population: u64) -> f64 {
    let p = population.max(1000) as f64;
    (4.0 + 6.0 * (p / 1000.0).log10()).clamp(4.0, 30.0)
}

pub fn distance_km(a_lat: f64, a_lon: f64, b_lat: f64, b_lon: f64) -> f64 {
    let (la1, la2) = (a_lat.to_radians(), b_lat.to_radians());
    let dla = la2 - la1;
    let dlo = (b_lon - a_lon).to_radians();
    let h = (dla / 2.0).sin().powi(2) + la1.cos() * la2.cos() * (dlo / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_KM * h.sqrt().min(1.0).asin()
}

fn cell(lat: f64, lon: f64) -> (i32, i32) {
    (
        (lat / CELL_DEG).floor() as i32,
        (lon / CELL_DEG).floor() as i32,
    )
}

fn meta(conn: &Connection, key: &str) -> CoreResult<Option<String>> {
    Ok(conn
        .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
        .optional()?)
}

impl OfflineGeocoder {
    pub fn open(path: &Path) -> CoreResult<Self> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let invalid = |reason: &str| CoreError::InvalidGeobase(reason.to_owned());
        let format = meta(&conn, "format").map_err(|_| invalid("not a Kadr geobase"))?;
        if format.as_deref() != Some(GEOBASE_FORMAT) {
            return Err(invalid("unsupported geobase format"));
        }
        let info = GeobaseInfo {
            version: meta(&conn, "version")?.unwrap_or_default(),
            source: meta(&conn, "source")?.unwrap_or_default(),
            places: 0,
            built_at: meta(&conn, "built_at")?.unwrap_or_default(),
        };

        let mut country_ids: HashMap<String, u16> = HashMap::new();
        let mut countries = Vec::new();
        {
            let mut stmt = conn.prepare("SELECT code, name FROM country ORDER BY code")?;
            let rows =
                stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            for row in rows {
                let (code, name) = row?;
                country_ids.insert(code, u16::try_from(countries.len()).unwrap_or(u16::MAX));
                countries.push(name);
            }
        }
        let mut admin_ids: HashMap<String, u32> = HashMap::new();
        let mut admins = Vec::new();
        {
            let mut stmt = conn.prepare("SELECT key, name FROM admin1 ORDER BY key")?;
            let rows =
                stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            for row in rows {
                let (key, name) = row?;
                admin_ids.insert(key, u32::try_from(admins.len()).unwrap_or(u32::MAX));
                admins.push(name);
            }
        }

        let mut places = Vec::new();
        let mut grid: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        let mut stmt =
            conn.prepare("SELECT lat, lon, name, country, admin1, population FROM place")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, f64>(0)?,
                r.get::<_, f64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, i64>(5)?,
            ))
        })?;
        for row in rows {
            let (lat, lon, name, country, admin1, population) = row?;
            let Some(&country) = country_ids.get(&country) else {
                continue;
            };
            let index = u32::try_from(places.len()).unwrap_or(u32::MAX);
            grid.entry(cell(lat, lon)).or_default().push(index);
            places.push(Place {
                lat,
                lon,
                name,
                country,
                admin1: admin1.and_then(|a| admin_ids.get(&a).copied()),
                population: u64::try_from(population).unwrap_or(0),
                reach_km: reach_km(u64::try_from(population).unwrap_or(0)),
            });
        }
        if places.is_empty() {
            return Err(invalid("geobase has no places"));
        }
        Ok(Self {
            info: GeobaseInfo {
                places: places.len() as u64,
                ..info
            },
            places,
            grid,
            countries,
            admins,
        })
    }

    pub fn info(&self) -> &GeobaseInfo {
        &self.info
    }

    /// Places within `radius_km`, with their distance.
    fn near(&self, lat: f64, lon: f64, radius_km: f64) -> impl Iterator<Item = (&Place, f64)> {
        let dlat = radius_km / 111.0;
        let dlon = radius_km / (111.0 * lat.to_radians().cos().abs().max(0.01));
        let (r0, c0) = cell(lat - dlat, lon - dlon);
        let (r1, c1) = cell(lat + dlat, lon + dlon);
        (r0..=r1)
            .flat_map(move |r| (c0..=c1).map(move |c| (r, c)))
            .filter_map(|key| self.grid.get(&key))
            .flatten()
            .filter_map(move |&i| {
                let p = self.places.get(i as usize)?;
                let d = distance_km(lat, lon, p.lat, p.lon);
                (d <= radius_km).then_some((p, d))
            })
    }
}

impl ReverseGeocoder for OfflineGeocoder {
    fn version(&self) -> &str {
        &self.info.version
    }

    fn lookup(&self, point: GeoPoint) -> Option<GeoPlace> {
        let (lat, lon) = (point.latitude, point.longitude);
        let covering: Vec<(&Place, f64)> = self
            .near(lat, lon, 30.0)
            .filter(|(p, d)| *d <= p.reach_km)
            .collect();
        let best = covering
            .iter()
            .min_by(|(a, da), (b, db)| (da / a.reach_km).total_cmp(&(db / b.reach_km)))
            .map(|(p, _)| *p);
        // Districts registered as separate places belong to the big city around them.
        let city = best.map(|b| {
            covering
                .iter()
                .map(|(p, _)| *p)
                .filter(|p| {
                    p.population > b.population
                        && distance_km(b.lat, b.lon, p.lat, p.lon) < ABSORB_SHARE * p.reach_km
                })
                .max_by_key(|p| p.population)
                .unwrap_or(b)
        });
        let nearest = |radius| {
            self.near(lat, lon, radius)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(p, _)| p)
        };
        let anchor = city.or_else(|| nearest(REGION_RADIUS_KM));
        let region = anchor
            .and_then(|p| p.admin1)
            .and_then(|a| self.admins.get(a as usize))
            .cloned();
        let country_place = anchor.or_else(|| nearest(COUNTRY_RADIUS_KM))?;
        let country = self
            .countries
            .get(usize::from(country_place.country))
            .cloned();
        Some(GeoPlace {
            city: city.map(|p| p.name.clone()),
            region,
            country,
        })
    }
}

/// Creates a geobase file. Used by the `kadr-geobase` builder and by tests.
pub struct GeobaseWriter {
    conn: Connection,
}

impl GeobaseWriter {
    pub fn create(path: &Path, version: &str, source: &str) -> CoreResult<Self> {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = OFF;
             PRAGMA synchronous = OFF;
             CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE country (code TEXT PRIMARY KEY, name TEXT NOT NULL);
             CREATE TABLE admin1 (key TEXT PRIMARY KEY, name TEXT NOT NULL);
             CREATE TABLE place (id INTEGER PRIMARY KEY, lat REAL NOT NULL, lon REAL NOT NULL,
                                 name TEXT NOT NULL, country TEXT NOT NULL, admin1 TEXT,
                                 population INTEGER NOT NULL);
             BEGIN;",
        )?;
        let built_at = chrono::Utc::now().format("%Y-%m-%d").to_string();
        for (k, v) in [
            ("format", GEOBASE_FORMAT),
            ("version", version),
            ("source", source),
            ("built_at", &built_at),
        ] {
            conn.execute("INSERT INTO meta (key, value) VALUES (?1, ?2)", [k, v])?;
        }
        Ok(Self { conn })
    }

    pub fn country(&self, code: &str, name: &str) -> CoreResult<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO country VALUES (?1, ?2)",
            [code, name],
        )?;
        Ok(())
    }

    pub fn admin1(&self, key: &str, name: &str) -> CoreResult<()> {
        self.conn
            .execute("INSERT OR REPLACE INTO admin1 VALUES (?1, ?2)", [key, name])?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn place(
        &self,
        id: i64,
        lat: f64,
        lon: f64,
        name: &str,
        country: &str,
        admin1: Option<&str>,
        population: i64,
    ) -> CoreResult<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO place VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![id, lat, lon, name, country, admin1, population],
        )?;
        Ok(())
    }

    pub fn finish(self) -> CoreResult<()> {
        self.conn.execute_batch("COMMIT; VACUUM;")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(dir: &Path) -> OfflineGeocoder {
        let path = dir.join("geo.sqlite3");
        let w = GeobaseWriter::create(&path, "test-1", "test data").unwrap();
        w.country("UA", "Україна").unwrap();
        w.country("MD", "Молдова").unwrap();
        w.admin1("UA.09", "Івано-Франківська область").unwrap();
        w.admin1("UA.12", "Київ").unwrap();
        w.place(1, 50.4501, 30.5234, "Київ", "UA", Some("UA.12"), 2_900_000)
            .unwrap();
        w.place(2, 48.4590, 24.5560, "Яремче", "UA", Some("UA.09"), 7_800)
            .unwrap();
        w.place(3, 48.2500, 24.5600, "Ворохта", "UA", Some("UA.09"), 4_000)
            .unwrap();
        w.place(4, 50.5200, 30.4500, "Мале село", "UA", Some("UA.12"), 1_000)
            .unwrap();
        w.place(5, 47.0105, 28.8638, "Кишинів", "MD", None, 640_000)
            .unwrap();
        w.place(6, 48.8534, 2.3488, "Париж", "FR", None, 2_138_551)
            .unwrap();
        w.place(7, 48.8520, 2.2700, "Paris 16 Passy", "FR", None, 159_386)
            .unwrap();
        w.place(8, 50.5110, 30.7909, "Бровари", "UA", Some("UA.12"), 100_000)
            .unwrap();
        w.country("FR", "Франція").unwrap();
        w.finish().unwrap();
        OfflineGeocoder::open(&path).unwrap()
    }

    fn at(lat: f64, lon: f64) -> GeoPoint {
        GeoPoint::new(lat, lon, None).unwrap()
    }

    #[test]
    fn reach_grows_with_population() {
        assert_eq!(reach_km(0), 4.0);
        assert!((reach_km(10_000) - 10.0).abs() < 1e-9);
        assert!(reach_km(3_000_000) > 24.0 && reach_km(3_000_000) <= 30.0);
        assert!((distance_km(50.45, 30.52, 49.84, 24.03) - 467.0).abs() < 5.0);
    }

    #[test]
    fn town_region_country_fallbacks() {
        let dir = tempfile::tempdir().unwrap();
        let g = sample(dir.path());
        assert_eq!(g.info().places, 8);
        // A district listed as its own place is part of the big city.
        assert_eq!(
            g.lookup(at(48.8584, 2.2945)).unwrap().city.as_deref(),
            Some("Париж")
        );
        // A real neighbouring town keeps its own name.
        assert_eq!(
            g.lookup(at(50.5110, 30.7909)).unwrap().city.as_deref(),
            Some("Бровари")
        );

        // In Yaremche.
        let p = g.lookup(at(48.4594, 24.5513)).unwrap();
        assert_eq!(
            (p.city.as_deref(), p.region.as_deref()),
            (Some("Яремче"), Some("Івано-Франківська область"))
        );
        // Mountains 20 km from any town: region only.
        let p = g.lookup(at(48.16, 24.50)).unwrap();
        assert_eq!(
            (p.city, p.region.as_deref()),
            (None, Some("Івано-Франківська область"))
        );
        // Kyiv outskirts closer to a village centre than to Kyiv's centre, but
        // well inside Kyiv's reach relative to the village's tiny one.
        let p = g.lookup(at(50.50, 30.48)).unwrap();
        assert_eq!(p.city.as_deref(), Some("Київ"));
        // Moldova without admin regions: city and country.
        let p = g.lookup(at(47.02, 28.86)).unwrap();
        assert_eq!(
            (p.city.as_deref(), p.region, p.country.as_deref()),
            (Some("Кишинів"), None, Some("Молдова"))
        );
        // Middle of the ocean: nothing.
        assert!(g.lookup(at(0.0, -30.0)).is_none());
    }

    #[test]
    fn rejects_files_that_are_not_geobases() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.sqlite3");
        Connection::open(&path)
            .unwrap()
            .execute_batch("CREATE TABLE t (x)")
            .unwrap();
        assert!(matches!(
            OfflineGeocoder::open(&path),
            Err(CoreError::InvalidGeobase(_))
        ));
        std::fs::write(dir.path().join("y"), b"not sqlite").unwrap();
        assert!(OfflineGeocoder::open(&dir.path().join("y")).is_err());
    }
}
