//! Turns raw tags into a capture date, GPS position and place text, following
//! the priorities of spec §6–§7. Pure functions: no I/O, no clock, no time
//! zone lookups except through [`NormalizeContext`].

use chrono::{Datelike, Duration, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};
use serde::{Deserialize, Serialize};

use super::RawMetadata;
use crate::domain::captured_date::{CapturedDate, DateSource, DateStatus};
use crate::domain::location::GeoPoint;
use crate::domain::media_item::MediaKind;

/// The first photograph was made in 1826; anything earlier is junk.
const MIN_YEAR: i32 = 1826;
/// Sources further apart than this are reported as contradicting. Wider than
/// any time-zone difference, so UTC vs. local time alone never triggers it.
const CONTRADICTION_HOURS: i64 = 26;

/// What normalization may depend on besides the tags.
pub struct NormalizeContext<'a> {
    pub filesystem_date_fallback: bool,
    /// "Now" in local wall-clock time; dates after now + 1 day are rejected.
    pub now_local: NaiveDateTime,
    /// UTC offset of this computer's time zone at a given UTC instant, in
    /// minutes. Used to show UTC video timestamps in local time.
    pub local_offset_at: &'a dyn Fn(NaiveDateTime) -> Option<i32>,
}

impl NormalizeContext<'_> {
    /// Context backed by the system clock and time zone.
    pub fn system(filesystem_date_fallback: bool) -> NormalizeContext<'static> {
        NormalizeContext {
            filesystem_date_fallback,
            now_local: chrono::Local::now().naive_local(),
            local_offset_at: &|utc| {
                Some(
                    chrono::Local
                        .from_utc_datetime(&utc)
                        .offset()
                        .local_minus_utc()
                        / 60,
                )
            },
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlaceText {
    pub city: Option<String>,
    pub region: Option<String>,
    pub country: Option<String>,
}

impl PlaceText {
    pub fn is_empty(&self) -> bool {
        self.city.is_none() && self.region.is_none() && self.country.is_none()
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Normalized {
    pub captured: Option<CapturedDate>,
    pub gps: Option<GeoPoint>,
    pub place_text: PlaceText,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration_ms: Option<u64>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub detected_type: Option<String>,
    /// Short Ukrainian sentences for the "До перевірки" list.
    pub review_reasons: Vec<String>,
}

// ---------------------------------------------------------------- dates

#[derive(Debug, Clone, PartialEq)]
pub enum ParsedDate {
    /// Blank or the all-zero placeholder cameras write: no information.
    Empty,
    /// Something was written but it is not a usable date.
    Invalid(String),
    Valid {
        local: NaiveDateTime,
        offset_minutes: Option<i32>,
        /// Only a date was present; the time is set to midnight.
        date_only: bool,
    },
}

/// Parses EXIF (`2025:08:13 16:42:07`), ISO 8601 (`2025-08-14T07:05:31.000Z`)
/// and the mixed forms helpers produce, with optional sub-seconds and offset.
pub fn parse_datetime(raw: &str) -> ParsedDate {
    let s = raw.trim();
    if s.is_empty()
        || s.chars()
            .all(|c| matches!(c, '0' | ':' | '-' | ' ' | 'T' | '.' | '+' | 'Z'))
    {
        return ParsedDate::Empty;
    }
    let invalid = || ParsedDate::Invalid(s.to_owned());

    // Split off a trailing offset: Z, +HH:MM, -HH:MM, +HHMM.
    let (body, offset_minutes) = split_offset(s);
    let body = body.trim();
    let (date_part, time_part) = match body.find([' ', 'T']) {
        Some(i) => (&body[..i], Some(body[i + 1..].trim())),
        None => (body, None),
    };
    let mut nums = date_part.split([':', '-']).map(|p| p.trim().parse::<u32>());
    let (Some(Ok(y)), Some(Ok(m)), Some(Ok(d)), None) =
        (nums.next(), nums.next(), nums.next(), nums.next())
    else {
        return invalid();
    };
    let Some(date) = i32::try_from(y)
        .ok()
        .and_then(|y| NaiveDate::from_ymd_opt(y, m, d))
    else {
        return invalid();
    };
    let (time, date_only) = match time_part.filter(|t| !t.is_empty()) {
        None => (NaiveTime::MIN, true),
        Some(t) => {
            let main = t.split('.').next().unwrap_or(t);
            match NaiveTime::parse_from_str(main, "%H:%M:%S")
                .or_else(|_| NaiveTime::parse_from_str(main, "%H:%M"))
            {
                Ok(time) => (time, false),
                Err(_) => return invalid(),
            }
        }
    };
    ParsedDate::Valid {
        local: date.and_time(time),
        offset_minutes,
        date_only,
    }
}

fn split_offset(s: &str) -> (&str, Option<i32>) {
    if let Some(body) = s.strip_suffix('Z') {
        return (body, Some(0));
    }
    // Look for a sign after the time part (position > 10 keeps the date's '-').
    if let Some(i) = s.rfind(['+', '-']).filter(|&i| i > 10)
        && let Some(minutes) = parse_offset(&s[i..])
    {
        return (&s[..i], Some(minutes));
    }
    (s, None)
}

/// `+03:00`, `-0330`, `+05` → minutes.
pub fn parse_offset(s: &str) -> Option<i32> {
    let s = s.trim();
    let sign = match s.chars().next()? {
        '+' => 1,
        '-' => -1,
        _ => return None,
    };
    let digits: String = s[1..].chars().filter(char::is_ascii_digit).collect();
    let (h, m) = match digits.len() {
        2 => (digits.parse::<i32>().ok()?, 0),
        4 => (
            digits[..2].parse::<i32>().ok()?,
            digits[2..].parse::<i32>().ok()?,
        ),
        _ => return None,
    };
    (h <= 14 && m < 60).then_some(sign * (h * 60 + m))
}

#[derive(Debug, Clone)]
struct Candidate {
    source: DateSource,
    tag: String,
    raw: String,
    local: NaiveDateTime,
    offset_minutes: Option<i32>,
    note: Option<String>,
}

impl Candidate {
    /// UTC instant when the offset is known.
    fn instant(&self) -> Option<NaiveDateTime> {
        self.offset_minutes
            .map(|o| self.local - Duration::minutes(i64::from(o)))
    }
}

/// How a raw value should be read.
#[derive(Clone, Copy, PartialEq)]
enum Clock {
    /// Wall-clock time where the photo was taken (EXIF, XMP, IPTC).
    Local,
    /// UTC by definition (QuickTime CreateDate, ffprobe creation_time).
    Utc,
}

struct DateSpec {
    tag: &'static str,
    source: DateSource,
    clock: Clock,
    offset_tag: Option<&'static str>,
}

const fn spec(
    tag: &'static str,
    source: DateSource,
    clock: Clock,
    offset_tag: Option<&'static str>,
) -> DateSpec {
    DateSpec {
        tag,
        source,
        clock,
        offset_tag,
    }
}

const PHOTO_DATES: &[DateSpec] = &[
    spec(
        "ExifIFD:DateTimeOriginal",
        DateSource::ExifOriginal,
        Clock::Local,
        Some("ExifIFD:OffsetTimeOriginal"),
    ),
    spec(
        "ExifIFD:CreateDate",
        DateSource::ExifOther,
        Clock::Local,
        Some("ExifIFD:OffsetTimeDigitized"),
    ),
    spec(
        "XMP-exif:DateTimeOriginal",
        DateSource::Xmp,
        Clock::Local,
        None,
    ),
    spec(
        "XMP-photoshop:DateCreated",
        DateSource::Xmp,
        Clock::Local,
        None,
    ),
    spec("XMP-xmp:CreateDate", DateSource::Xmp, Clock::Local, None),
    spec("IPTC:DateCreated", DateSource::Iptc, Clock::Local, None),
    spec(
        "Keys:CreationDate",
        DateSource::QuickTime,
        Clock::Local,
        None,
    ),
    spec(
        "QuickTime:CreateDate",
        DateSource::QuickTime,
        Clock::Utc,
        None,
    ),
];

const VIDEO_DATES: &[DateSpec] = &[
    // Apple's local time with offset is the most precise video timestamp.
    spec(
        "Keys:CreationDate",
        DateSource::QuickTime,
        Clock::Local,
        None,
    ),
    spec(
        "FFprobe:com.apple.quicktime.creationdate",
        DateSource::QuickTime,
        Clock::Local,
        None,
    ),
    spec(
        "ExifIFD:DateTimeOriginal",
        DateSource::ExifOriginal,
        Clock::Local,
        Some("ExifIFD:OffsetTimeOriginal"),
    ),
    spec(
        "XMP-exif:DateTimeOriginal",
        DateSource::Xmp,
        Clock::Local,
        None,
    ),
    spec("XMP-xmp:CreateDate", DateSource::Xmp, Clock::Local, None),
    spec(
        "UserData:DateTimeOriginal",
        DateSource::HelperOther,
        Clock::Local,
        None,
    ),
    spec(
        "QuickTime:CreateDate",
        DateSource::QuickTime,
        Clock::Utc,
        None,
    ),
    spec(
        "Track1:MediaCreateDate",
        DateSource::QuickTime,
        Clock::Utc,
        None,
    ),
    spec(
        "FFprobe:creation_time",
        DateSource::QuickTime,
        Clock::Utc,
        None,
    ),
    spec(
        "FFprobe:stream_creation_time",
        DateSource::QuickTime,
        Clock::Utc,
        None,
    ),
];

fn format_offset(minutes: i32) -> String {
    let sign = if minutes < 0 { '−' } else { '+' };
    let m = minutes.abs();
    format!("UTC{sign}{:02}:{:02}", m / 60, m % 60)
}

fn collect_dates(
    raw: &RawMetadata,
    kind: MediaKind,
    ctx: &NormalizeContext<'_>,
    reasons: &mut Vec<String>,
) -> Vec<Candidate> {
    let specs = if kind == MediaKind::Video {
        VIDEO_DATES
    } else {
        PHOTO_DATES
    };
    let latest = ctx.now_local + Duration::days(1);
    let mut out = Vec::new();
    for spec in specs {
        let Some(value) = raw.str(spec.tag) else {
            continue;
        };
        // IPTC keeps date and time apart.
        let joined;
        let value = if spec.tag == "IPTC:DateCreated" {
            joined = match raw.str("IPTC:TimeCreated") {
                Some(t) => format!("{value} {t}"),
                None => value.to_owned(),
            };
            joined.as_str()
        } else {
            value
        };
        let (local, mut offset, date_only) = match parse_datetime(value) {
            ParsedDate::Empty => continue,
            ParsedDate::Invalid(v) => {
                reasons.push(format!("Не вдалося розпізнати дату {} = «{v}»", spec.tag));
                continue;
            }
            ParsedDate::Valid {
                local,
                offset_minutes,
                date_only,
            } => (local, offset_minutes, date_only),
        };
        if offset.is_none()
            && let Some(o) = spec
                .offset_tag
                .and_then(|t| raw.str(t))
                .and_then(parse_offset)
        {
            offset = Some(o);
        }
        let mut note = date_only.then(|| "У метаданих лише дата, без часу".to_owned());
        let (local, offset) = if spec.clock == Clock::Utc {
            // The value is UTC. Show it in this computer's time zone and say so.
            match (ctx.local_offset_at)(local) {
                Some(o) => {
                    note = Some(format!(
                        "Час у файлі записано в UTC; переведено за часовим поясом цього комп'ютера ({})",
                        format_offset(o)
                    ));
                    (local + Duration::minutes(i64::from(o)), Some(o))
                }
                None => {
                    note = Some("Час у файлі записано в UTC".to_owned());
                    (local, Some(0))
                }
            }
        } else {
            (local, offset)
        };
        if local.year() < MIN_YEAR || local > latest {
            reasons.push(format!("Неправдоподібна дата {} = «{value}»", spec.tag));
            continue;
        }
        out.push(Candidate {
            source: spec.source,
            tag: spec.tag.to_owned(),
            raw: value.to_owned(),
            local,
            offset_minutes: offset,
            note,
        });
    }
    out
}

fn contradiction(a: &Candidate, b: &Candidate) -> bool {
    let diff = match (a.instant(), b.instant()) {
        (Some(x), Some(y)) => x - y,
        _ => a.local - b.local,
    };
    diff.num_hours().abs() > CONTRADICTION_HOURS
}

fn pick_date(
    raw: &RawMetadata,
    kind: MediaKind,
    mtime_ms: i64,
    ctx: &NormalizeContext<'_>,
    reasons: &mut Vec<String>,
) -> Option<CapturedDate> {
    let candidates = collect_dates(raw, kind, ctx, reasons);
    if let Some(best) = candidates.first() {
        let conflict = candidates.iter().skip(1).find(|c| contradiction(best, c));
        let status = if conflict.is_some() {
            DateStatus::NeedsReview
        } else {
            DateStatus::Confirmed
        };
        if let Some(c) = conflict {
            reasons.push(format!(
                "Дати суперечать одна одній: {} = «{}», {} = «{}»",
                best.tag, best.raw, c.tag, c.raw
            ));
        }
        return Some(CapturedDate {
            raw: best.raw.clone(),
            local: best.local,
            offset_minutes: best.offset_minutes,
            source: best.source,
            status,
            note: best.note.clone(),
        });
    }
    if ctx.filesystem_date_fallback && mtime_ms > 0 {
        let utc = chrono::DateTime::from_timestamp_millis(mtime_ms)?.naive_utc();
        let offset = (ctx.local_offset_at)(utc);
        let local = utc + Duration::minutes(i64::from(offset.unwrap_or(0)));
        return Some(CapturedDate {
            raw: format!("mtime {}", local.format("%Y-%m-%d %H:%M:%S")),
            local,
            offset_minutes: offset,
            source: DateSource::FileSystem,
            status: DateStatus::FileSystemFallback,
            note: Some("Дата зміни файла, а не час зйомки".to_owned()),
        });
    }
    None
}

// ---------------------------------------------------------------- GPS

/// ISO 6709 as written by phones: `+48.1597+024.4999+1180.000/`.
pub fn parse_iso6709(s: &str) -> Option<(f64, f64, Option<f64>)> {
    let s = s.trim().trim_end_matches('/');
    let mut parts = Vec::new();
    let mut start = 0;
    for (i, c) in s.char_indices().skip(1) {
        if c == '+' || c == '-' {
            parts.push(&s[start..i]);
            start = i;
        }
    }
    parts.push(&s[start..]);
    let nums: Vec<f64> = parts.iter().filter_map(|p| p.parse().ok()).collect();
    match nums.as_slice() {
        [lat, lon] => Some((*lat, *lon, None)),
        [lat, lon, alt] => Some((*lat, *lon, Some(*alt))),
        _ => None,
    }
}

fn pick_gps(raw: &RawMetadata, reasons: &mut Vec<String>) -> Option<GeoPoint> {
    let from_composite = raw
        .f64("Composite:GPSLatitude")
        .zip(raw.f64("Composite:GPSLongitude"))
        .map(|(lat, lon)| (lat, lon, raw.f64("Composite:GPSAltitude")));
    let from_iso = || {
        [
            "FFprobe:com.apple.quicktime.location.ISO6709",
            "FFprobe:location",
        ]
        .iter()
        .find_map(|k| raw.str(k).and_then(parse_iso6709))
    };
    let (lat, lon, alt) = from_composite.or_else(from_iso)?;
    if lat == 0.0 && lon == 0.0 {
        reasons.push("GPS-координати 0°, 0° — найімовірніше, порожнє значення".to_owned());
        return None;
    }
    let point = GeoPoint::new(lat, lon, alt);
    if point.is_none() {
        reasons.push(format!("Некоректні GPS-координати: {lat}, {lon}"));
    }
    point
}

fn first_text(raw: &RawMetadata, tags: &[&str]) -> Option<String> {
    tags.iter().find_map(|tag| {
        raw.all_with_tag(tag).find_map(|(_, v)| {
            v.as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        })
    })
}

fn first_u32(raw: &RawMetadata, keys: &[&str]) -> Option<u32> {
    keys.iter().find_map(|k| {
        if let Some(tag) = k.strip_prefix("*:") {
            raw.all_with_tag(tag)
                .find_map(|(_, v)| v.as_f64())
                .and_then(|f| u32::try_from(f as i64).ok())
        } else {
            raw.f64(k).and_then(|f| u32::try_from(f as i64).ok())
        }
    })
}

pub fn normalize(
    raw: &RawMetadata,
    kind: MediaKind,
    mtime_ms: i64,
    sniffed_format: Option<&str>,
    ctx: &NormalizeContext<'_>,
) -> Normalized {
    let mut reasons = Vec::new();
    let captured = pick_date(raw, kind, mtime_ms, ctx, &mut reasons);
    let gps = pick_gps(raw, &mut reasons);
    let place_text = PlaceText {
        city: first_text(raw, &["City"]),
        region: first_text(raw, &["State", "Province-State"]),
        country: first_text(raw, &["Country", "Country-PrimaryLocationName"]),
    };
    let duration_ms = [
        "QuickTime:Duration",
        "Composite:Duration",
        "FFprobe:Duration",
    ]
    .iter()
    .find_map(|k| raw.f64(k))
    .filter(|d| d.is_finite() && *d > 0.0)
    .map(|d| (d * 1000.0).round() as u64);
    let mut width = first_u32(
        raw,
        &["File:ImageWidth", "*:ImageWidth", "ExifIFD:ExifImageWidth"],
    );
    let mut height = first_u32(
        raw,
        &[
            "File:ImageHeight",
            "*:ImageHeight",
            "ExifIFD:ExifImageHeight",
        ],
    );
    // EXIF orientations 5..=8 turn the picture by 90°: report what is seen.
    if raw
        .f64("IFD0:Orientation")
        .is_some_and(|o| (5.0..=8.0).contains(&o))
    {
        std::mem::swap(&mut width, &mut height);
    }
    Normalized {
        captured,
        gps,
        place_text,
        width,
        height,
        duration_ms,
        video_codec: raw.str("FFprobe:VideoCodec").map(str::to_owned),
        audio_codec: raw.str("FFprobe:AudioCodec").map(str::to_owned),
        detected_type: raw
            .str("File:FileType")
            .map(str::to_owned)
            .or_else(|| sniffed_format.map(str::to_owned)),
        review_reasons: reasons,
    }
}

/// Fixed offset helper for tests and callers that know the zone.
pub fn fixed_offset_minutes(minutes: i32) -> Option<FixedOffset> {
    FixedOffset::east_opt(minutes * 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ctx(fallback: bool) -> NormalizeContext<'static> {
        NormalizeContext {
            filesystem_date_fallback: fallback,
            now_local: NaiveDate::from_ymd_opt(2026, 10, 17)
                .unwrap()
                .and_hms_opt(12, 0, 0)
                .unwrap(),
            // Kyiv summer time for every instant: deterministic.
            local_offset_at: &|_| Some(180),
        }
    }

    fn meta(pairs: &[(&str, serde_json::Value)]) -> RawMetadata {
        RawMetadata {
            tags: pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), v.clone()))
                .collect(),
            ..RawMetadata::default()
        }
    }

    fn dt(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").unwrap()
    }

    #[test]
    fn parses_common_date_shapes() {
        let ok = |s: &str| match parse_datetime(s) {
            ParsedDate::Valid {
                local,
                offset_minutes,
                ..
            } => (local, offset_minutes),
            other => panic!("{s}: {other:?}"),
        };
        assert_eq!(ok("2025:08:13 16:42:07"), (dt("2025-08-13 16:42:07"), None));
        assert_eq!(
            ok("2025:08:14 10:05:31+03:00"),
            (dt("2025-08-14 10:05:31"), Some(180))
        );
        assert_eq!(
            ok("2025-08-14T07:05:31.000000Z"),
            (dt("2025-08-14 07:05:31"), Some(0))
        );
        assert_eq!(
            ok("2025-08-14T10:05:31+0300"),
            (dt("2025-08-14 10:05:31"), Some(180))
        );
        assert_eq!(
            ok("2021:12:31 23:59:59.123-03:00"),
            (dt("2021-12-31 23:59:59"), Some(-180))
        );
        assert!(matches!(
            parse_datetime("2025:05:01"),
            ParsedDate::Valid {
                date_only: true,
                ..
            }
        ));
    }

    #[test]
    fn placeholder_and_garbage_dates() {
        assert_eq!(parse_datetime("0000:00:00 00:00:00"), ParsedDate::Empty);
        assert_eq!(parse_datetime("    :  :     :  :  "), ParsedDate::Empty);
        assert_eq!(parse_datetime(""), ParsedDate::Empty);
        assert!(matches!(
            parse_datetime("2025:02:30 10:00:00"),
            ParsedDate::Invalid(_)
        ));
        assert!(matches!(
            parse_datetime("yesterday"),
            ParsedDate::Invalid(_)
        ));
        assert!(matches!(
            parse_datetime("2025:08:13 25:00:00"),
            ParsedDate::Invalid(_)
        ));
    }

    #[test]
    fn exif_original_with_offset_is_confirmed() {
        let m = meta(&[
            ("ExifIFD:DateTimeOriginal", json!("2025:08:13 16:42:07")),
            ("ExifIFD:OffsetTimeOriginal", json!("+03:00")),
            ("ExifIFD:CreateDate", json!("2025:08:13 16:42:07")),
        ]);
        let n = normalize(&m, MediaKind::Photo, 0, None, &ctx(false));
        let d = n.captured.unwrap();
        assert_eq!(
            (d.local, d.offset_minutes, d.source, d.status),
            (
                dt("2025-08-13 16:42:07"),
                Some(180),
                DateSource::ExifOriginal,
                DateStatus::Confirmed
            )
        );
        assert!(n.review_reasons.is_empty());
    }

    #[test]
    fn date_without_offset_is_not_shifted() {
        let m = meta(&[("ExifIFD:DateTimeOriginal", json!("2025:12:31 23:30:00"))]);
        let d = normalize(&m, MediaKind::Photo, 0, None, &ctx(false))
            .captured
            .unwrap();
        assert_eq!(
            (d.local, d.offset_minutes),
            (dt("2025-12-31 23:30:00"), None)
        );
        assert_eq!((d.year(), d.month()), (2025, 12));
    }

    #[test]
    fn zero_date_falls_through_to_next_source() {
        let m = meta(&[
            ("ExifIFD:DateTimeOriginal", json!("0000:00:00 00:00:00")),
            (
                "XMP-photoshop:DateCreated",
                json!("2023:05:01 10:00:00+02:00"),
            ),
        ]);
        let n = normalize(&m, MediaKind::Photo, 0, None, &ctx(false));
        let d = n.captured.unwrap();
        assert_eq!((d.source, d.offset_minutes), (DateSource::Xmp, Some(120)));
        assert!(n.review_reasons.is_empty());
    }

    #[test]
    fn no_date_stays_unknown_unless_fallback_enabled() {
        let m = RawMetadata::default();
        // 2023-03-02 06:14:00 UTC
        let mtime = 1_677_737_640_000;
        assert!(
            normalize(&m, MediaKind::Photo, mtime, None, &ctx(false))
                .captured
                .is_none()
        );
        let d = normalize(&m, MediaKind::Photo, mtime, None, &ctx(true))
            .captured
            .unwrap();
        assert_eq!(
            (d.source, d.status),
            (DateSource::FileSystem, DateStatus::FileSystemFallback)
        );
        assert_eq!(d.local, dt("2023-03-02 09:14:00"));
    }

    #[test]
    fn contradicting_dates_need_review() {
        let m = meta(&[
            ("ExifIFD:DateTimeOriginal", json!("2019:06:01 12:00:00")),
            ("ExifIFD:CreateDate", json!("2024:01:10 08:00:00")),
        ]);
        let n = normalize(&m, MediaKind::Photo, 0, None, &ctx(false));
        assert_eq!(n.captured.unwrap().status, DateStatus::NeedsReview);
        assert_eq!(n.review_reasons.len(), 1);
    }

    #[test]
    fn future_and_ancient_dates_are_rejected_with_reason() {
        let m = meta(&[
            ("ExifIFD:DateTimeOriginal", json!("2099:01:01 00:00:00")),
            ("ExifIFD:CreateDate", json!("1700:01:01 00:00:00")),
        ]);
        let n = normalize(&m, MediaKind::Photo, 0, None, &ctx(false));
        assert!(n.captured.is_none());
        assert_eq!(n.review_reasons.len(), 2);
    }

    #[test]
    fn iphone_video_prefers_local_keys_date() {
        let m = meta(&[
            ("QuickTime:CreateDate", json!("2025:08:14 07:05:31")),
            ("Keys:CreationDate", json!("2025:08:14 10:05:31+03:00")),
        ]);
        let n = normalize(&m, MediaKind::Video, 0, None, &ctx(false));
        let d = n.captured.unwrap();
        assert_eq!(
            (d.local, d.offset_minutes, d.status),
            (dt("2025-08-14 10:05:31"), Some(180), DateStatus::Confirmed)
        );
        assert!(d.note.is_none());
    }

    #[test]
    fn utc_only_video_is_converted_and_explained() {
        let m = meta(&[(
            "FFprobe:creation_time",
            json!("2025-05-03T18:40:10.000000Z"),
        )]);
        let d = normalize(&m, MediaKind::Video, 0, None, &ctx(false))
            .captured
            .unwrap();
        assert_eq!(
            (d.local, d.offset_minutes),
            (dt("2025-05-03 21:40:10"), Some(180))
        );
        assert!(d.note.unwrap().contains("UTC"));
    }

    #[test]
    fn utc_and_local_of_same_moment_do_not_contradict() {
        let m = meta(&[
            ("Keys:CreationDate", json!("2025:08:14 00:30:00+14:00")),
            ("QuickTime:CreateDate", json!("2025:08:13 10:30:00")),
        ]);
        let n = normalize(&m, MediaKind::Video, 0, None, &ctx(false));
        assert!(n.review_reasons.is_empty(), "{:?}", n.review_reasons);
    }

    #[test]
    fn iso6709_locations() {
        assert_eq!(
            parse_iso6709("+48.1597+024.4999+1180.000/"),
            Some((48.1597, 24.4999, Some(1180.0)))
        );
        assert_eq!(
            parse_iso6709("-34.6037-058.3816/"),
            Some((-34.6037, -58.3816, None))
        );
        assert_eq!(parse_iso6709("garbage"), None);
    }

    #[test]
    fn gps_sources_and_null_island() {
        let m = meta(&[
            ("Composite:GPSLatitude", json!(-34.6037)),
            ("Composite:GPSLongitude", json!(-58.3816)),
        ]);
        let g = normalize(&m, MediaKind::Photo, 0, None, &ctx(false))
            .gps
            .unwrap();
        assert_eq!((g.latitude, g.longitude), (-34.6037, -58.3816));

        let m = meta(&[("FFprobe:location", json!("+47.0105+028.8638/"))]);
        assert!(
            normalize(&m, MediaKind::Video, 0, None, &ctx(false))
                .gps
                .is_some()
        );

        let m = meta(&[
            ("Composite:GPSLatitude", json!(0)),
            ("Composite:GPSLongitude", json!(0)),
        ]);
        let n = normalize(&m, MediaKind::Photo, 0, None, &ctx(false));
        assert!(n.gps.is_none());
        assert_eq!(n.review_reasons.len(), 1);
    }

    #[test]
    fn rotated_photos_report_displayed_size() {
        let m = meta(&[
            ("File:ImageWidth", json!(2400)),
            ("File:ImageHeight", json!(1600)),
            ("IFD0:Orientation", json!(6)),
        ]);
        let n = normalize(&m, MediaKind::Photo, 0, None, &ctx(false));
        assert_eq!((n.width, n.height), (Some(1600), Some(2400)));
    }

    #[test]
    fn place_text_dimensions_and_duration() {
        let m = meta(&[
            ("XMP-photoshop:City", json!("Львів")),
            ("XMP-photoshop:Country", json!("Україна")),
            ("PNG:ImageWidth", json!(64)),
            ("PNG:ImageHeight", json!(48)),
            ("FFprobe:Duration", json!(1.5)),
        ]);
        let n = normalize(&m, MediaKind::Photo, 0, Some("PNG"), &ctx(false));
        assert_eq!(n.place_text.city.as_deref(), Some("Львів"));
        assert_eq!(n.place_text.region, None);
        assert_eq!(
            (n.width, n.height, n.duration_ms),
            (Some(64), Some(48), Some(1500))
        );
        assert_eq!(n.detected_type.as_deref(), Some("PNG"));
    }
}
