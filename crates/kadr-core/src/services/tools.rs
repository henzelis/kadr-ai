//! Detection of external helpers (ExifTool, FFprobe). Helpers are started
//! with an explicit argument list, never through a shell, and are killed
//! when they do not answer in time.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;

const VERSION_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(20);

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

/// Runs `<program> <version args>` and reports what it found.
/// `custom_path` overrides the default program name looked up on PATH.
pub fn probe(tool: Tool, custom_path: Option<&str>) -> ToolStatus {
    let program = custom_path
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map_or_else(|| PathBuf::from(tool.default_program()), PathBuf::from);
    let state = match run_with_timeout(&program, tool.version_args(), VERSION_TIMEOUT) {
        Ok(stdout) => match tool.parse_version(&stdout) {
            Some(version) => ToolState::Available { version },
            None => ToolState::Broken {
                reason: "unexpected version output".to_owned(),
            },
        },
        Err(RunError::NotFound) => ToolState::Missing,
        Err(RunError::Failed(reason)) => ToolState::Broken { reason },
    };
    ToolStatus {
        tool,
        program: program.display().to_string(),
        state,
    }
}

enum RunError {
    NotFound,
    Failed(String),
}

fn run_with_timeout(
    program: &PathBuf,
    args: &[&str],
    timeout: Duration,
) -> Result<String, RunError> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => RunError::NotFound,
            _ => RunError::Failed(e.to_string()),
        })?;

    // Read stdout on a thread so a chatty helper cannot block on a full pipe.
    let reader = child.stdout.take().map(|mut out| {
        thread::spawn(move || {
            let mut buf = String::new();
            let _ = out.read_to_string(&mut buf);
            buf
        })
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(RunError::Failed(format!(
                    "no answer within {}s",
                    timeout.as_secs()
                )));
            }
            Ok(None) => thread::sleep(POLL_INTERVAL),
            Err(e) => return Err(RunError::Failed(e.to_string())),
        }
    };
    let stdout = reader.and_then(|h| h.join().ok()).unwrap_or_default();
    if status.success() {
        Ok(stdout)
    } else {
        Err(RunError::Failed(format!("exited with {status}")))
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
