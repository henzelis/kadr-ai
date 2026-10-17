//! Detection of external helpers (ExifTool, FFprobe). Helpers are started
//! with an explicit argument list, never through a shell, and are killed
//! when they do not answer in time.

use std::ffi::OsStr;
use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;

use super::process::{self, RunError};

const VERSION_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    Exiftool,
    Ffprobe,
}

impl Tool {
    fn default_program(self) -> &'static str {
        match self {
            Self::Exiftool => "exiftool",
            Self::Ffprobe => "ffprobe",
        }
    }

    fn version_args(self) -> &'static [&'static str] {
        match self {
            Self::Exiftool => &["-ver"],
            Self::Ffprobe => &["-hide_banner", "-version"],
        }
    }

    /// Extracts the version number from the helper's `--version` output.
    fn parse_version(self, stdout: &str) -> Option<String> {
        let first = stdout.lines().map(str::trim).find(|l| !l.is_empty())?;
        match self {
            // `12.76`
            Self::Exiftool => Some(first.to_owned()),
            // `ffprobe version 6.1.1-3ubuntu5 Copyright (c) ...`
            Self::Ffprobe => first
                .strip_prefix("ffprobe version ")
                .and_then(|rest| rest.split_whitespace().next())
                .map(str::to_owned),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ToolState {
    Available {
        version: String,
    },
    /// Not found at the configured path or on PATH.
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
            ToolState::Available { version } => Some(version),
            _ => None,
        }
    }
}

/// Runs `<program> <version args>` and reports what it found.
/// `custom_path` overrides the default program name looked up on PATH.
pub fn probe(tool: Tool, custom_path: Option<&str>) -> ToolStatus {
    let program = custom_path
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map_or_else(|| PathBuf::from(tool.default_program()), PathBuf::from);
    let args: Vec<&OsStr> = tool.version_args().iter().map(OsStr::new).collect();
    let state = match process::run(&program, &args, None, VERSION_TIMEOUT) {
        Ok(out) if out.status.success() => {
            match tool.parse_version(&String::from_utf8_lossy(&out.stdout)) {
                Some(version) => ToolState::Available { version },
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
    };
    ToolStatus {
        tool,
        program: program.display().to_string(),
        state,
    }
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
        assert_eq!(Tool::Ffprobe.parse_version("garbage"), None);
        assert_eq!(Tool::Exiftool.parse_version("\n\n"), None);
    }

    #[test]
    fn missing_program_is_reported_as_missing() {
        let status = probe(Tool::Exiftool, Some("kadr-no-such-helper-7f3a"));
        assert_eq!(status.state, ToolState::Missing);
    }

    #[test]
    fn serializes_with_state_tag() {
        let status = ToolStatus {
            tool: Tool::Ffprobe,
            program: "ffprobe".to_owned(),
            state: ToolState::Available {
                version: "6.1".to_owned(),
            },
        };
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["tool"], "ffprobe");
        assert_eq!(json["state"], "available");
        assert_eq!(json["version"], "6.1");
    }
}
