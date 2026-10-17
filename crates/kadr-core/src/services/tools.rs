//! Detection of external helpers (ExifTool, FFprobe, FFmpeg). Helpers are
//! looked up in order: a path set in Settings, the copy bundled with the app
//! (`<resources>/helpers/...`), then PATH. They are started with an explicit
//! argument list, never through a shell, and killed when they hang.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;

use super::process::{self, RunError};

const VERSION_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    Exiftool,
    Ffprobe,
    Ffmpeg,
}

impl Tool {
    fn program_name(self) -> &'static str {
        match (self, cfg!(windows)) {
            (Self::Exiftool, true) => "exiftool.exe",
            (Self::Exiftool, false) => "exiftool",
            (Self::Ffprobe, true) => "ffprobe.exe",
            (Self::Ffprobe, false) => "ffprobe",
            (Self::Ffmpeg, true) => "ffmpeg.exe",
            (Self::Ffmpeg, false) => "ffmpeg",
        }
    }

    /// Location inside the bundled helpers directory.
    fn bundled_path(self, helpers: &Path) -> PathBuf {
        let dir = match self {
            Self::Exiftool => "exiftool",
            Self::Ffprobe | Self::Ffmpeg => "ffmpeg",
        };
        helpers.join(dir).join(self.program_name())
    }

    fn version_args(self) -> &'static [&'static str] {
        match self {
            Self::Exiftool => &["-ver"],
            Self::Ffprobe | Self::Ffmpeg => &["-hide_banner", "-version"],
        }
    }

    /// Extracts the version number from the helper's version output.
    fn parse_version(self, stdout: &str) -> Option<String> {
        let first = stdout.lines().map(str::trim).find(|l| !l.is_empty())?;
        let prefix = match self {
            // `12.76`
            Self::Exiftool => return Some(first.to_owned()),
            // `ffprobe version 6.1.1-3ubuntu5 Copyright (c) ...`
            Self::Ffprobe => "ffprobe version ",
            Self::Ffmpeg => "ffmpeg version ",
        };
        first
            .strip_prefix(prefix)
            .and_then(|rest| rest.split_whitespace().next())
            .map(str::to_owned)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolSource {
    /// Path chosen in Settings.
    Custom,
    /// Shipped with Kadr.
    Bundled,
    /// Found on PATH.
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ToolState {
    Available {
        version: String,
        source: ToolSource,
    },
    /// Not found anywhere.
    Missing,
    /// Started but failed, timed out or printed something unexpected.
    Broken {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolStatus {
    pub tool: Tool,
    pub program: String,
    #[serde(flatten)]
    pub state: ToolState,
}

impl ToolStatus {
    pub fn version(&self) -> Option<&str> {
        match &self.state {
            ToolState::Available { version, .. } => Some(version),
            _ => None,
        }
    }
}

fn probe_one(tool: Tool, program: &Path) -> ToolState {
    let args: Vec<&OsStr> = tool.version_args().iter().map(OsStr::new).collect();
    match process::run(program, &args, None, VERSION_TIMEOUT) {
        Ok(out) if out.status.success() => {
            match tool.parse_version(&String::from_utf8_lossy(&out.stdout)) {
                Some(version) => ToolState::Available {
                    version,
                    source: ToolSource::System,
                },
                None => ToolState::Broken {
                    reason: "unexpected version output".to_owned(),
                },
            }
        }
        Ok(out) => ToolState::Broken {
            reason: format!("exited with {}", out.status),
        },
        Err(RunError::NotFound) => ToolState::Missing,
        Err(err) => ToolState::Broken {
            reason: err.to_string(),
        },
    }
}

/// Finds a working copy of `tool`. A broken custom path is reported as such
/// rather than silently replaced, so the user sees their setting is wrong.
pub fn probe(tool: Tool, custom_path: Option<&str>, bundled_helpers: Option<&Path>) -> ToolStatus {
    let mut candidates: Vec<(PathBuf, ToolSource)> = Vec::new();
    if let Some(custom) = custom_path.map(str::trim).filter(|p| !p.is_empty()) {
        candidates.push((PathBuf::from(custom), ToolSource::Custom));
    }
    if let Some(dir) = bundled_helpers {
        let p = tool.bundled_path(dir);
        if p.is_file() {
            candidates.push((p, ToolSource::Bundled));
        }
    }
    candidates.push((PathBuf::from(tool.program_name()), ToolSource::System));

    let mut first_problem: Option<ToolStatus> = None;
    for (program, source) in candidates {
        let state = match probe_one(tool, &program) {
            ToolState::Available { version, .. } => ToolState::Available { version, source },
            other => other,
        };
        let status = ToolStatus {
            tool,
            program: program.display().to_string(),
            state,
        };
        match (&status.state, source) {
            (ToolState::Available { .. }, _) => return status,
            (ToolState::Broken { .. }, ToolSource::Custom) => return status,
            (ToolState::Broken { .. }, _) if first_problem.is_none() => {
                first_problem = Some(status)
            }
            _ => {}
        }
    }
    first_problem.unwrap_or(ToolStatus {
        tool,
        program: tool.program_name().to_owned(),
        state: ToolState::Missing,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_version_outputs() {
        assert_eq!(
            Tool::Exiftool.parse_version("12.76\n").as_deref(),
            Some("12.76")
        );
        assert_eq!(
            Tool::Ffprobe
                .parse_version("ffprobe version 6.1.1-3ubuntu5 Copyright (c) 2007-2023\n")
                .as_deref(),
            Some("6.1.1-3ubuntu5")
        );
        assert_eq!(
            Tool::Ffmpeg
                .parse_version("ffmpeg version n8.1.3-18-g46d899d7a0-20261016 Copyright")
                .as_deref(),
            Some("n8.1.3-18-g46d899d7a0-20261016")
        );
        assert_eq!(Tool::Ffprobe.parse_version("garbage"), None);
        assert_eq!(Tool::Exiftool.parse_version("\n\n"), None);
    }

    #[test]
    fn missing_custom_program_falls_back_or_reports() {
        let status = probe(Tool::Exiftool, Some("kadr-no-such-helper-7f3a"), None);
        // A custom path that does not exist is "missing" there; the search
        // continues on PATH, which may or may not have ExifTool.
        assert!(matches!(
            status.state,
            ToolState::Missing
                | ToolState::Available {
                    source: ToolSource::System,
                    ..
                }
        ));
    }

    #[cfg(unix)]
    #[test]
    fn bundled_copy_is_preferred_over_path() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("exiftool").join("exiftool");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, "#!/bin/sh\necho 99.01\n").unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        let status = probe(Tool::Exiftool, None, Some(dir.path()));
        assert_eq!(
            status.state,
            ToolState::Available {
                version: "99.01".to_owned(),
                source: ToolSource::Bundled
            }
        );
    }

    #[test]
    fn serializes_with_state_tag() {
        let status = ToolStatus {
            tool: Tool::Ffprobe,
            program: "ffprobe".to_owned(),
            state: ToolState::Available {
                version: "6.1".to_owned(),
                source: ToolSource::Bundled,
            },
        };
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["tool"], "ffprobe");
        assert_eq!(json["state"], "available");
        assert_eq!(json["source"], "bundled");
    }
}
