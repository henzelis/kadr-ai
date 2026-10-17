//! Running helper programs safely: explicit argument lists (never a shell),
//! optional stdin, a hard timeout, and pipes drained on threads so a chatty
//! helper cannot deadlock.

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug)]
pub struct Output {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunError {
    NotFound,
    Timeout(Duration),
    Failed(String),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "program not found"),
            Self::Timeout(d) => write!(f, "no answer within {}s", d.as_secs()),
            Self::Failed(m) => write!(f, "{m}"),
        }
    }
}

pub fn run(
    program: &Path,
    args: &[&std::ffi::OsStr],
    stdin: Option<Vec<u8>>,
    timeout: Duration,
) -> Result<Output, RunError> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    hide_console_window(&mut command);
    let mut child = command.spawn().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => RunError::NotFound,
        _ => RunError::Failed(e.to_string()),
    })?;

    let writer = match (child.stdin.take(), stdin) {
        (Some(mut pipe), Some(data)) => Some(thread::spawn(move || {
            // A helper that exits early closes the pipe; that is not our error.
            let _ = pipe.write_all(&data);
        })),
        _ => None,
    };
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        pipe.map(|mut p| {
            thread::spawn(move || {
                let mut buf = Vec::new();
                let _ = p.read_to_end(&mut buf);
                buf
            })
        })
    };
    let stdout = drain(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let stderr = drain(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(RunError::Timeout(timeout));
            }
            Ok(None) => thread::sleep(POLL_INTERVAL),
            Err(e) => {
                let _ = child.kill();
                return Err(RunError::Failed(e.to_string()));
            }
        }
    };
    if let Some(w) = writer {
        let _ = w.join();
    }
    let collect =
        |h: Option<thread::JoinHandle<Vec<u8>>>| h.and_then(|h| h.join().ok()).unwrap_or_default();
    Ok(Output {
        status,
        stdout: collect(stdout),
        stderr: collect(stderr),
    })
}

/// GUI apps on Windows would flash a console window for every helper call.
#[cfg(windows)]
fn hide_console_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console_window(_command: &mut Command) {}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn passes_stdin_and_collects_stdout() {
        let out = run(
            Path::new("cat"),
            &[],
            Some(b"hello".to_vec()),
            Duration::from_secs(5),
        )
        .unwrap();
        assert!(out.status.success());
        assert_eq!(out.stdout, b"hello");
    }

    #[test]
    fn kills_on_timeout() {
        let started = Instant::now();
        let err = run(
            Path::new("sleep"),
            &[OsStr::new("10")],
            None,
            Duration::from_millis(200),
        )
        .unwrap_err();
        assert!(matches!(err, RunError::Timeout(_)));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn arguments_are_not_interpreted_by_a_shell() {
        let out = run(
            Path::new("echo"),
            &[OsStr::new("$(id); `id`")],
            None,
            Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "$(id); `id`");
    }
}
