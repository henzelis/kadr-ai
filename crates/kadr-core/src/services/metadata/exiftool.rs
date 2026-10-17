//! ExifTool adapter. Files are passed in batches through an argument file on
//! stdin (`-@ -`), so paths never touch a command line and one process serves
//! many files.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::{MetadataProvider, RawMetadata, ReadError, Tags, path_key};
use crate::services::process::{self, RunError};

/// Only these tags are requested: enough for dates, places and the inspector,
/// small enough to store per file.
const TAGS: &[&str] = &[
    "-DateTimeOriginal",
    "-OffsetTimeOriginal",
    "-CreateDate",
    "-OffsetTimeDigitized",
    "-DateCreated",
    "-TimeCreated",
    "-MediaCreateDate",
    "-CreationDate",
    "-ContentCreateDate",
    "-GPSLatitude",
    "-GPSLongitude",
    "-GPSAltitude",
    "-City",
    "-State",
    "-Province-State",
    "-Country",
    "-Country-PrimaryLocationName",
    "-FileType",
    "-MIMEType",
    "-ImageWidth",
    "-ImageHeight",
    "-ExifImageWidth",
    "-ExifImageHeight",
    "-Duration",
    "-Rotation",
    "-Orientation",
    "-ContentIdentifier",
    "-Make",
    "-Model",
    "-Warning",
    "-Error",
];

const BASE_TIMEOUT: Duration = Duration::from_secs(30);
const PER_FILE_TIMEOUT: Duration = Duration::from_secs(2);

pub struct ExifTool {
    program: PathBuf,
    id: String,
}

impl ExifTool {
    pub fn new(program: PathBuf, version: &str) -> Self {
        Self {
            program,
            id: format!("exiftool {version}"),
        }
    }

    fn run_batch(&self, paths: &[PathBuf]) -> Result<HashMap<String, Tags>, RunError> {
        let mut list = String::new();
        for p in paths {
            list.push_str(&p.to_string_lossy());
            list.push('\n');
        }
        let mut args: Vec<&OsStr> = [
            "-json",
            "-a",
            "-G1",
            "-n",
            "-fast",
            "-charset",
            "filename=utf8",
        ]
        .iter()
        .map(OsStr::new)
        .collect();
        args.extend(TAGS.iter().map(OsStr::new));
        args.extend([OsStr::new("-@"), OsStr::new("-")]);
        let files = u32::try_from(paths.len()).unwrap_or(u32::MAX);
        let timeout = BASE_TIMEOUT + PER_FILE_TIMEOUT * files;
        let out = process::run(&self.program, &args, Some(list.into_bytes()), timeout)?;
        // Exit code 1 only means "some file failed"; failed files are simply
        // absent from the JSON, so parse whatever came back.
        if out.stdout.iter().all(u8::is_ascii_whitespace) {
            return Ok(HashMap::new());
        }
        let parsed: Vec<Tags> = serde_json::from_slice(&out.stdout)
            .map_err(|e| RunError::Failed(format!("unexpected ExifTool output: {e}")))?;
        Ok(parsed
            .into_iter()
            .filter_map(|mut tags| {
                let source = tags
                    .remove("SourceFile")?
                    .as_str()
                    .map(|s| path_key(Path::new(s)))?;
                Some((source, tags))
            })
            .collect())
    }

    fn to_result(&self, tags: Option<Tags>) -> Result<RawMetadata, ReadError> {
        let Some(mut tags) = tags else {
            return Err(ReadError::Unreadable(
                "ExifTool could not read the file".to_owned(),
            ));
        };
        let mut warnings = Vec::new();
        for key in ["ExifTool:Warning", "ExifTool:Error"] {
            if let Some(v) = tags.remove(key) {
                warnings.push(v.as_str().map_or_else(|| v.to_string(), str::to_owned));
            }
        }
        Ok(RawMetadata {
            tags,
            providers: vec![self.id.clone()],
            warnings,
        })
    }
}

impl MetadataProvider for ExifTool {
    fn id(&self) -> &str {
        &self.id
    }

    fn read_batch(&self, paths: &[PathBuf]) -> Vec<Result<RawMetadata, ReadError>> {
        // A newline in a file name would split it in the argument file.
        let (safe, unsafe_names): (Vec<_>, Vec<_>) = paths
            .iter()
            .enumerate()
            .partition(|(_, p)| !p.to_string_lossy().contains(['\n', '\r']));
        let safe_paths: Vec<PathBuf> = safe.iter().map(|(_, p)| (*p).clone()).collect();

        let mut by_path = match self.run_batch(&safe_paths) {
            Ok(map) => map,
            Err(RunError::NotFound) => {
                return paths
                    .iter()
                    .map(|_| Err(ReadError::HelperMissing))
                    .collect();
            }
            // One bad file can hang or crash a whole batch: retry one by one
            // so only that file fails.
            Err(_) if safe_paths.len() > 1 => {
                let mut map = HashMap::new();
                for p in &safe_paths {
                    if let Ok(single) = self.run_batch(std::slice::from_ref(p)) {
                        map.extend(single);
                    }
                }
                map
            }
            Err(err) => {
                let mut results: Vec<_> = paths
                    .iter()
                    .map(|_| Err(ReadError::HelperFailed(err.to_string())))
                    .collect();
                for (i, _) in unsafe_names {
                    results[i] = Err(ReadError::Unreadable(
                        "file name contains a line break".to_owned(),
                    ));
                }
                return results;
            }
        };
        paths
            .iter()
            .map(|p| {
                if p.to_string_lossy().contains(['\n', '\r']) {
                    Err(ReadError::Unreadable(
                        "file name contains a line break".to_owned(),
                    ))
                } else {
                    self.to_result(by_path.remove(&path_key(p)))
                }
            })
            .collect()
    }
}

/// Convenience for tests and diagnostics.
pub fn read_one(tool: &ExifTool, path: &Path) -> Result<RawMetadata, ReadError> {
    tool.read_batch(&[path.to_path_buf()])
        .pop()
        .unwrap_or_else(|| Err(ReadError::HelperFailed("no result".to_owned())))
}
