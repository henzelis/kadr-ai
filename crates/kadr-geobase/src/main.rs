//! Builds `geobase.sqlite3` from GeoNames dumps (CC BY 4.0):
//!
//! ```text
//! kadr-geobase <geonames-dir> <out-file> <version>
//! ```
//!
//! `<geonames-dir>` must contain `cities1000.txt`, `admin1CodesASCII.txt`,
//! `countryInfo.txt` and `alternateNamesV2.txt` from
//! <https://download.geonames.org/export/dump/>. Names are taken in Ukrainian
//! where GeoNames has them, otherwise the GeoNames default name is kept.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use kadr_core::services::geocoder::GeobaseWriter;

const SOURCE: &str = "GeoNames (geonames.org), CC BY 4.0";

/// Sections of towns, historical, abandoned or destroyed places never name a photo.
const SKIP_FEATURES: &[&str] = &["PPLX", "PPLH", "PPLW", "PPLQ", "PPLCH"];

fn lines(path: &Path) -> std::io::Result<impl Iterator<Item = String>> {
    let file = File::open(path)?;
    Ok(BufReader::with_capacity(1 << 20, file)
        .lines()
        .map_while(Result::ok))
}

struct City {
    id: i64,
    name: String,
    lat: f64,
    lon: f64,
    country: String,
    admin1: String,
    population: i64,
}

#[derive(Default)]
struct UkName {
    name: String,
    rank: u8,
}

/// GeoNames often writes "Львівська Область"; Ukrainian spells the generic
/// part in lower case unless it starts the name.
fn tidy(name: &str) -> String {
    const GENERIC: &[&str] = &[
        "Область",
        "Район",
        "Округ",
        "Край",
        "Повіт",
        "Провінція",
        "Регіон",
        "Муніципалітет",
    ];
    name.split(' ')
        .enumerate()
        .map(|(i, w)| {
            if i > 0 && GENERIC.contains(&w) {
                w.to_lowercase()
            } else {
                w.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_cyrillic(s: &str) -> bool {
    s.chars().any(|c| ('\u{0400}'..='\u{04FF}').contains(&c))
}

fn run(dir: &Path, out: &Path, version: &str) -> Result<(), String> {
    fn err(what: &'static str) -> impl Fn(std::io::Error) -> String {
        move |e| format!("{what}: {e}")
    }

    let mut cities = Vec::new();
    for line in lines(&dir.join("cities1000.txt")).map_err(err("cities1000.txt"))? {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 15 || SKIP_FEATURES.contains(&f[7]) {
            continue;
        }
        let (Ok(id), Ok(lat), Ok(lon)) = (f[0].parse(), f[4].parse(), f[5].parse()) else {
            continue;
        };
        cities.push(City {
            id,
            name: f[1].to_owned(),
            lat,
            lon,
            country: f[8].to_owned(),
            admin1: f[10].to_owned(),
            population: f[14].parse().unwrap_or(0),
        });
    }

    let mut admins: Vec<(String, String, i64)> = Vec::new();
    for line in lines(&dir.join("admin1CodesASCII.txt")).map_err(err("admin1CodesASCII.txt"))? {
        let f: Vec<&str> = line.split('\t').collect();
        if let [key, name, _, id] = f.as_slice()
            && let Ok(id) = id.parse()
        {
            admins.push(((*key).to_owned(), (*name).to_owned(), id));
        }
    }

    let mut countries: Vec<(String, String, i64)> = Vec::new();
    for line in lines(&dir.join("countryInfo.txt")).map_err(err("countryInfo.txt"))? {
        if line.starts_with('#') {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() > 16
            && let Ok(id) = f[16].parse()
        {
            countries.push((f[0].to_owned(), f[4].to_owned(), id));
        }
    }

    // Ukrainian names: preferred > short (countries) > other; never
    // historic, colloquial or Latin transliterations tagged "uk".
    let short_wins: HashSet<i64> = countries.iter().map(|c| c.2).collect();
    let wanted: HashSet<i64> = cities
        .iter()
        .map(|c| c.id)
        .chain(admins.iter().map(|a| a.2))
        .chain(short_wins.iter().copied())
        .collect();
    let mut uk: HashMap<i64, UkName> = HashMap::new();
    for line in lines(&dir.join("alternateNamesV2.txt")).map_err(err("alternateNamesV2.txt"))? {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 8 || f[2] != "uk" {
            continue;
        }
        let Ok(id) = f[1].parse::<i64>() else {
            continue;
        };
        let (name, preferred, short, colloquial, historic) =
            (f[3], f[4] == "1", f[5] == "1", f[6] == "1", f[7] == "1");
        if !wanted.contains(&id) || colloquial || historic || !is_cyrillic(name) {
            continue;
        }
        let rank = match (preferred, short && short_wins.contains(&id)) {
            (_, true) => 3,
            (true, _) => 2,
            _ => 1,
        };
        let slot = uk.entry(id).or_default();
        if rank > slot.rank {
            *slot = UkName {
                name: tidy(name),
                rank,
            };
        }
    }
    let name_of = |id: i64, fallback: &str| {
        uk.get(&id)
            .map_or_else(|| fallback.to_owned(), |n| n.name.clone())
    };

    let w = GeobaseWriter::create(out, version, SOURCE).map_err(|e| e.to_string())?;
    for (code, name, id) in &countries {
        w.country(code, &name_of(*id, name))
            .map_err(|e| e.to_string())?;
    }
    for (key, name, id) in &admins {
        w.admin1(key, &name_of(*id, name))
            .map_err(|e| e.to_string())?;
    }
    let mut translated = 0;
    for c in &cities {
        if uk.contains_key(&c.id) {
            translated += 1;
        }
        let admin = format!("{}.{}", c.country, c.admin1);
        w.place(
            c.id,
            c.lat,
            c.lon,
            &name_of(c.id, &c.name),
            &c.country,
            (!c.admin1.is_empty()).then_some(admin.as_str()),
            c.population,
        )
        .map_err(|e| e.to_string())?;
    }
    w.finish().map_err(|e| e.to_string())?;
    println!(
        "{} places ({} with Ukrainian names), {} regions, {} countries -> {}",
        cities.len(),
        translated,
        admins.len(),
        countries.len(),
        out.display()
    );
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let [_, dir, out, version] = args.as_slice() else {
        eprintln!("usage: kadr-geobase <geonames-dir> <out-file> <version>");
        return ExitCode::from(2);
    };
    match run(&PathBuf::from(dir), &PathBuf::from(out), version) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("kadr-geobase: {e}");
            ExitCode::FAILURE
        }
    }
}
