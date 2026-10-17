//! Folder naming: the destination template, month names in both folder
//! languages, and making each path segment safe on Windows, macOS and Linux.

use chrono::{Datelike, NaiveDateTime};
use serde::{Deserialize, Serialize};

use super::location::Language;

/// Default destination layout: `{year}/{month}/{place}/{filename}`, where
/// `{month}` is rendered according to [`MonthFormat`].
pub const DEFAULT_TEMPLATE: &str = "{year}/{month}/{place}/{filename}";

/// Longest folder or file name written, in characters. Windows allows 255
/// UTF-16 units per name; staying well below leaves room for suffixes.
pub const MAX_SEGMENT_CHARS: usize = 120;

const MONTH_NAMES_UK: [&str; 12] = [
    "Січень",
    "Лютий",
    "Березень",
    "Квітень",
    "Травень",
    "Червень",
    "Липень",
    "Серпень",
    "Вересень",
    "Жовтень",
    "Листопад",
    "Грудень",
];
const MONTH_NAMES_EN: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MonthFormat {
    /// `08 - Серпень` / `08 - August` — keeps folders in chronological order.
    #[default]
    NumberName,
    /// `08`
    Number,
    /// `Серпень` / `August`
    Name,
}

/// Month name for 1..=12 in the folder language, `None` otherwise.
pub fn month_name(month: u32, lang: Language) -> Option<&'static str> {
    let index = usize::try_from(month.checked_sub(1)?).ok()?;
    match lang {
        Language::Uk => MONTH_NAMES_UK.get(index).copied(),
        Language::En => MONTH_NAMES_EN.get(index).copied(),
    }
}

impl MonthFormat {
    /// Folder name for a month number 1..=12, `None` for anything else.
    pub fn render(self, month: u32, lang: Language) -> Option<String> {
        let name = month_name(month, lang)?;
        Some(match self {
            Self::NumberName => format!("{month:02} - {name}"),
            Self::Number => format!("{month:02}"),
            Self::Name => name.to_owned(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    Text(String),
    Year,
    Month,
    MonthNumber,
    MonthName,
    Day,
    Place,
}

impl Part {
    fn needs_date(&self) -> bool {
        matches!(
            self,
            Self::Year | Self::Month | Self::MonthNumber | Self::MonthName | Self::Day
        )
    }
}

/// A parsed destination template: folder segments, then `{filename}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    folders: Vec<Vec<Part>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateError {
    /// `{filename}` must be the whole last segment, once.
    FilenameNotLast,
    UnknownToken(String),
    EmptySegment,
    /// `..`, a drive or a leading slash.
    NotRelative,
}

impl Template {
    pub fn parse(s: &str) -> Result<Self, TemplateError> {
        let s = s.trim();
        if s.starts_with(['/', '\\']) || s.get(1..2) == Some(":") {
            return Err(TemplateError::NotRelative);
        }
        let segments: Vec<&str> = s.split(['/', '\\']).collect();
        let (last, folders) = segments
            .split_last()
            .ok_or(TemplateError::FilenameNotLast)?;
        if last.trim() != "{filename}" || folders.iter().any(|f| f.contains("{filename}")) {
            return Err(TemplateError::FilenameNotLast);
        }
        let mut parsed = Vec::new();
        for seg in folders {
            let seg = seg.trim();
            if seg.is_empty() {
                return Err(TemplateError::EmptySegment);
            }
            if seg == ".." || seg == "." {
                return Err(TemplateError::NotRelative);
            }
            parsed.push(parse_segment(seg)?);
        }
        Ok(Self { folders: parsed })
    }
}

fn parse_segment(seg: &str) -> Result<Vec<Part>, TemplateError> {
    let mut parts = Vec::new();
    let mut rest = seg;
    while let Some(open) = rest.find('{') {
        if open > 0 {
            parts.push(Part::Text(rest[..open].to_owned()));
        }
        let close = rest[open..]
            .find('}')
            .ok_or_else(|| TemplateError::UnknownToken(rest[open..].to_owned()))?
            + open;
        let token = &rest[open + 1..close];
        parts.push(match token {
            "year" => Part::Year,
            "month" => Part::Month,
            "month_number" => Part::MonthNumber,
            "month_name" => Part::MonthName,
            "day" => Part::Day,
            "place" => Part::Place,
            other => return Err(TemplateError::UnknownToken(other.to_owned())),
        });
        rest = &rest[close + 1..];
    }
    if !rest.is_empty() {
        parts.push(Part::Text(rest.to_owned()));
    }
    Ok(parts)
}

/// What fills a template for one file.
pub struct Fill<'a> {
    pub date: Option<NaiveDateTime>,
    pub place: Option<&'a str>,
    pub month_format: MonthFormat,
    pub language: Language,
    pub unknown_date: &'a str,
    pub unknown_place: &'a str,
}

impl Template {
    /// Folder names for one file, already made safe. Without a date, every
    /// date folder collapses into one "unknown date" folder in place of the
    /// first one, as in spec §8: `Невідома дата/{place}/{filename}`.
    pub fn folders(&self, fill: &Fill<'_>) -> Vec<String> {
        let mut out = Vec::new();
        let mut unknown_date_written = false;
        for seg in &self.folders {
            if fill.date.is_none() && seg.iter().any(Part::needs_date) {
                if !unknown_date_written {
                    out.push(sanitize_segment(fill.unknown_date));
                    unknown_date_written = true;
                }
                continue;
            }
            let mut text = String::new();
            for part in seg {
                match (part, fill.date) {
                    (Part::Text(t), _) => text.push_str(t),
                    (Part::Year, Some(d)) => text.push_str(&format!("{:04}", d.year())),
                    (Part::Month, Some(d)) => {
                        text.push_str(
                            &fill
                                .month_format
                                .render(d.month(), fill.language)
                                .unwrap_or_default(),
                        );
                    }
                    (Part::MonthNumber, Some(d)) => text.push_str(&format!("{:02}", d.month())),
                    (Part::MonthName, Some(d)) => {
                        text.push_str(month_name(d.month(), fill.language).unwrap_or(""))
                    }
                    (Part::Day, Some(d)) => text.push_str(&format!("{:02}", d.day())),
                    (Part::Place, _) => text.push_str(fill.place.unwrap_or(fill.unknown_place)),
                    _ => {}
                }
            }
            out.push(sanitize_segment(&text));
        }
        out
    }
}

/// Device names Windows reserves in any folder, with or without extension.
fn is_windows_reserved(name: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or(name)
        .trim_end()
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.chars().count() == 4
            && stem
                .chars()
                .last()
                .is_some_and(|c| c.is_ascii_digit() && c != '0'))
}

fn clean_chars(s: &str) -> String {
    s.chars()
        .map(|c| {
            if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect()
}

/// Makes one folder name valid everywhere: forbidden characters become `_`,
/// no leading/trailing spaces or trailing dots, no reserved Windows names,
/// no `.`/`..`, bounded length. Never returns an empty string.
pub fn sanitize_segment(s: &str) -> String {
    let mut out: String = clean_chars(s)
        .trim()
        .trim_end_matches(['.', ' '])
        .to_owned();
    if out.chars().count() > MAX_SEGMENT_CHARS {
        out = out
            .chars()
            .take(MAX_SEGMENT_CHARS)
            .collect::<String>()
            .trim_end_matches(['.', ' '])
            .to_owned();
    }
    if out.is_empty() || out == "." || out == ".." {
        return "_".to_owned();
    }
    if is_windows_reserved(&out) {
        out.push('_');
    }
    out
}

/// Same for a file name, keeping its extension when shortening.
pub fn sanitize_file_name(name: &str) -> String {
    let cleaned = clean_chars(name);
    let cleaned = cleaned.trim().trim_end_matches(['.', ' ']);
    let (stem, ext) = match cleaned.rfind('.') {
        Some(i) if i > 0 => (&cleaned[..i], &cleaned[i..]),
        _ => (cleaned, ""),
    };
    let budget = MAX_SEGMENT_CHARS.saturating_sub(ext.chars().count()).max(1);
    let mut stem: String = stem.chars().take(budget).collect();
    stem = stem.trim_end_matches(['.', ' ']).to_owned();
    if stem.is_empty() {
        stem = "_".to_owned();
    }
    if is_windows_reserved(&stem) {
        stem.push('_');
    }
    format!("{stem}{ext}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn date(y: i32, m: u32, d: u32) -> Option<NaiveDateTime> {
        NaiveDate::from_ymd_opt(y, m, d).and_then(|d| d.and_hms_opt(12, 0, 0))
    }

    fn fill<'a>(d: Option<NaiveDateTime>, place: Option<&'a str>, lang: Language) -> Fill<'a> {
        Fill {
            date: d,
            place,
            month_format: MonthFormat::NumberName,
            language: lang,
            unknown_date: if lang == Language::En {
                "Unknown date"
            } else {
                "Невідома дата"
            },
            unknown_place: if lang == Language::En {
                "No location"
            } else {
                "Без геолокації"
            },
        }
    }

    #[test]
    fn renders_all_month_formats_in_both_languages() {
        assert_eq!(
            MonthFormat::NumberName.render(8, Language::Uk).as_deref(),
            Some("08 - Серпень")
        );
        assert_eq!(
            MonthFormat::NumberName.render(8, Language::En).as_deref(),
            Some("08 - August")
        );
        assert_eq!(
            MonthFormat::Number.render(8, Language::En).as_deref(),
            Some("08")
        );
        assert_eq!(
            MonthFormat::Name.render(12, Language::Uk).as_deref(),
            Some("Грудень")
        );
        assert_eq!(MonthFormat::Number.render(13, Language::Uk), None);
    }

    #[test]
    fn number_name_folders_sort_chronologically() {
        for lang in [Language::Uk, Language::En] {
            let mut names: Vec<String> = (1..=12)
                .filter_map(|m| MonthFormat::NumberName.render(m, lang))
                .collect();
            let chronological = names.clone();
            names.sort();
            assert_eq!(names, chronological);
        }
    }

    #[test]
    fn spec_section_8_layouts() {
        let t = Template::parse(DEFAULT_TEMPLATE).unwrap();
        assert_eq!(
            t.folders(&fill(date(2025, 8, 13), Some("Карпати"), Language::Uk)),
            ["2025", "08 - Серпень", "Карпати"]
        );
        assert_eq!(
            t.folders(&fill(date(2025, 8, 13), None, Language::Uk)),
            ["2025", "08 - Серпень", "Без геолокації"]
        );
        assert_eq!(
            t.folders(&fill(None, Some("Париж"), Language::Uk)),
            ["Невідома дата", "Париж"]
        );
        assert_eq!(
            t.folders(&fill(None, None, Language::En)),
            ["Unknown date", "No location"]
        );
        assert_eq!(
            t.folders(&fill(date(2024, 1, 5), Some("Kyiv"), Language::En)),
            ["2024", "01 - January", "Kyiv"]
        );
    }

    #[test]
    fn custom_templates() {
        let t = Template::parse("{year}-{month_number}/{day} {place}/{filename}").unwrap();
        assert_eq!(
            t.folders(&fill(date(2025, 8, 3), Some("Lviv"), Language::En)),
            ["2025-08", "03 Lviv"]
        );
        assert_eq!(
            t.folders(&fill(None, Some("Lviv"), Language::En)),
            ["Unknown date"]
        );
        let flat = Template::parse("{filename}").unwrap();
        assert!(
            flat.folders(&fill(date(2025, 1, 1), None, Language::Uk))
                .is_empty()
        );
    }

    #[test]
    fn rejects_bad_templates() {
        assert_eq!(
            Template::parse("{year}/{month}"),
            Err(TemplateError::FilenameNotLast)
        );
        assert_eq!(
            Template::parse("{filename}/{year}/{filename}"),
            Err(TemplateError::FilenameNotLast)
        );
        assert_eq!(
            Template::parse("{year}/{hour}/{filename}"),
            Err(TemplateError::UnknownToken("hour".into()))
        );
        assert_eq!(
            Template::parse("../{filename}"),
            Err(TemplateError::NotRelative)
        );
        assert_eq!(
            Template::parse("/abs/{filename}"),
            Err(TemplateError::NotRelative)
        );
        assert_eq!(
            Template::parse("C:/x/{filename}"),
            Err(TemplateError::NotRelative)
        );
        assert_eq!(
            Template::parse("{year}//{filename}"),
            Err(TemplateError::EmptySegment)
        );
    }

    #[test]
    fn sanitizes_names_for_every_os() {
        assert_eq!(
            sanitize_segment("Kyiv: \"Podil\" / Center?"),
            "Kyiv_ _Podil_ _ Center_"
        );
        assert_eq!(sanitize_segment("  Trailing dots...  "), "Trailing dots");
        assert_eq!(sanitize_segment("CON"), "CON_");
        assert_eq!(sanitize_segment("com1.txt"), "com1.txt_");
        assert_eq!(sanitize_segment("COM10"), "COM10");
        assert_eq!(sanitize_segment(".."), "_");
        assert_eq!(sanitize_segment(""), "_");
        assert_eq!(sanitize_segment("a\u{7}b"), "a_b");
        assert_eq!(
            sanitize_segment(&"я".repeat(300)).chars().count(),
            MAX_SEGMENT_CHARS
        );
        assert_eq!(
            sanitize_segment("Івано-Франківська область"),
            "Івано-Франківська область"
        );
    }

    #[test]
    fn file_names_keep_their_extension() {
        assert_eq!(sanitize_file_name("IMG_0001.HEIC"), "IMG_0001.HEIC");
        assert_eq!(sanitize_file_name("a:b.jpg"), "a_b.jpg");
        assert_eq!(sanitize_file_name("nul.jpg"), "nul_.jpg");
        let long = format!("{}.jpeg", "x".repeat(300));
        let s = sanitize_file_name(&long);
        assert!(s.ends_with(".jpeg") && s.chars().count() == MAX_SEGMENT_CHARS);
        assert_eq!(sanitize_file_name(".hidden"), ".hidden");
    }
}
