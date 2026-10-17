//! Metadata providers. Each one turns a file into a flat map of
//! `"Group:Tag" -> value` using ExifTool's group names, so the normalizer
//! works the same whichever provider read the file.

pub mod exiftool;
pub mod ffprobe;
pub mod native_image;
pub mod normalize;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub type Tags = BTreeMap<String, serde_json::Value>;

/// What the providers found for one file. Stored as `metadata_json`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RawMetadata {
    pub tags: Tags,
    /// Providers that contributed, e.g. `["exiftool 12.76", "ffprobe 6.1.1"]`.
    pub providers: Vec<String>,
    /// Problems reported by the providers (damaged file, unknown format...).
    pub warnings: Vec<String>,
}

impl RawMetadata {
    pub fn merge(&mut self, other: RawMetadata) {
        for (k, v) in other.tags {
            self.tags.entry(k).or_insert(v);
        }
        self.providers.extend(other.providers);
        self.warnings.extend(other.warnings);
    }

    pub fn str(&self, key: &str) -> Option<&str> {
        self.tags
            .get(key)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }

    pub fn f64(&self, key: &str) -> Option<f64> {
        match self.tags.get(key)? {
            serde_json::Value::Number(n) => n.as_f64(),
            serde_json::Value::String(s) => s.trim().parse().ok(),
            _ => None,
        }
    }

    /// Values of every `<any group>:<tag>` key, in key order.
    pub fn all_with_tag<'a>(
        &'a self,
        tag: &'a str,
    ) -> impl Iterator<Item = (&'a str, &'a serde_json::Value)> + 'a {
        self.tags.iter().filter_map(move |(k, v)| {
            let (group, name) = k.split_once(':')?;
            (name == tag).then_some((group, v))
        })
    }
}

/// Why a provider produced nothing for a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    /// The helper is not installed / not configured.
    HelperMissing,
    /// The helper ran but this file could not be read.
    Unreadable(String),
    /// The helper crashed or timed out.
    HelperFailed(String),
}

/// Batch of files → one result per input path, in the same order.
pub trait MetadataProvider: Send + Sync {
    /// Name plus version; part of the cache key, so a new helper version
    /// re-reads files.
    fn id(&self) -> &str;
    fn read_batch(&self, paths: &[PathBuf]) -> Vec<Result<RawMetadata, ReadError>>;
}

pub(crate) fn path_key(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
