//! FFprobe adapter for video: container/stream facts and the timestamps and
//! location tags that cameras and phones write. One process per file.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};

use super::{MetadataProvider, RawMetadata, ReadError, Tags};
use crate::services::process::{self, RunError};

const TIMEOUT: Duration = Duration::from_secs(30);
const GROUP: &str = "FFprobe";

pub struct FfProbe {
    program: PathBuf,
    id: String,
}

#[derive(Deserialize)]
struct Probe {
    #[serde(default)]
    format: Format,
    #[serde(default)]
    streams: Vec<Stream>,
}

#[derive(Deserialize, Default)]
struct Format {
    format_name: Option<String>,
    duration: Option<String>,
    #[serde(default)]
    tags: std::collections::BTreeMap<String, Value>,
}

#[derive(Deserialize)]
struct Stream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    #[serde(default)]
    tags: std::collections::BTreeMap<String, Value>,
    #[serde(default)]
    side_data_list: Vec<std::collections::BTreeMap<String, Value>>,
}

impl FfProbe {
    pub fn new(program: PathBuf, version: &str) -> Self {
        Self {
            program,
            id: format!("ffprobe {version}"),
        }
    }

    fn read(&self, path: &Path) -> Result<RawMetadata, ReadError> {
        let args = [
            OsStr::new("-v"),
            OsStr::new("error"),
            OsStr::new("-print_format"),
            OsStr::new("json"),
            OsStr::new("-show_format"),
            OsStr::new("-show_streams"),
            OsStr::new("-i"),
            path.as_os_str(),
        ];
        let out = process::run(&self.program, &args, None, TIMEOUT).map_err(|e| match e {
            RunError::NotFound => ReadError::HelperMissing,
            other => ReadError::HelperFailed(other.to_string()),
        })?;
        if !out.status.success() {
            let msg = String::from_utf8_lossy(&out.stderr);
            return Err(ReadError::Unreadable(first_line(&msg)));
        }
        let probe: Probe = serde_json::from_slice(&out.stdout)
            .map_err(|e| ReadError::HelperFailed(format!("unexpected FFprobe output: {e}")))?;
        Ok(RawMetadata {
            tags: to_tags(&probe),
            providers: vec![self.id.clone()],
            warnings: Vec::new(),
        })
    }
}

fn first_line(s: &str) -> String {
    s.lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("unreadable")
        .trim()
        .to_owned()
}

fn to_tags(probe: &Probe) -> Tags {
    let mut tags = Tags::new();
    let mut put = |name: &str, v: Value| {
        tags.entry(format!("{GROUP}:{name}")).or_insert(v);
    };
    if let Some(f) = &probe.format.format_name {
        put("FormatName", json!(f));
    }
    if let Some(d) = probe
        .format
        .duration
        .as_deref()
        .and_then(|d| d.parse::<f64>().ok())
    {
        put("Duration", json!(d));
    }
    for key in [
        "creation_time",
        "com.apple.quicktime.creationdate",
        "location",
        "com.apple.quicktime.location.ISO6709",
        "com.apple.quicktime.make",
        "com.apple.quicktime.model",
    ] {
        if let Some(v) = probe.format.tags.get(key) {
            put(key, v.clone());
        }
    }
    let video = probe
        .streams
        .iter()
        .find(|s| s.codec_type.as_deref() == Some("video"));
    let audio = probe
        .streams
        .iter()
        .find(|s| s.codec_type.as_deref() == Some("audio"));
    if let Some(v) = video {
        if let Some(c) = &v.codec_name {
            put("VideoCodec", json!(c));
        }
        if let (Some(w), Some(h)) = (v.width, v.height) {
            put("ImageWidth", json!(w));
            put("ImageHeight", json!(h));
        }
        let rotation = v
            .side_data_list
            .iter()
            .find_map(|sd| sd.get("rotation").and_then(Value::as_f64))
            .or_else(|| v.tags.get("rotate").and_then(|r| r.as_str()?.parse().ok()));
        if let Some(r) = rotation {
            put("Rotation", json!(r));
        }
        if let Some(t) = v.tags.get("creation_time") {
            put("stream_creation_time", t.clone());
        }
    }
    if let Some(c) = audio.and_then(|a| a.codec_name.as_ref()) {
        put("AudioCodec", json!(c));
    }
    tags
}

impl MetadataProvider for FfProbe {
    fn id(&self) -> &str {
        &self.id
    }

    fn read_batch(&self, paths: &[PathBuf]) -> Vec<Result<RawMetadata, ReadError>> {
        paths.iter().map(|p| self.read(p)).collect()
    }
}
