use serde::Serialize;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::util::tail;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum CommandStatus {
    Passed,
    Failed,
    Unavailable,
    TimedOut,
}

#[derive(Debug, Serialize)]
pub(crate) struct CommandOutcome {
    pub(crate) status: CommandStatus,
    pub(crate) command: Vec<String>,
    pub(crate) working_directory: String,
    pub(crate) exit_code: Option<i32>,
    pub(crate) duration_ms: u128,
    pub(crate) stdout_tail: String,
    pub(crate) stderr_tail: String,
    pub(crate) reason: Option<String>,
    #[serde(skip)]
    pub(crate) stdout: String,
    #[serde(skip)]
    pub(crate) stderr: String,
}

pub(crate) struct CommandRequest<'a> {
    pub(crate) program: &'a str,
    pub(crate) args: &'a [String],
    pub(crate) current_dir: &'a Path,
    pub(crate) environment: BTreeMap<String, String>,
    pub(crate) timeout: Duration,
    pub(crate) tail_lines: usize,
}

impl<'a> CommandRequest<'a> {
    pub(crate) fn new(program: &'a str, args: &'a [String], current_dir: &'a Path) -> Self {
        Self {
            program,
            args,
            current_dir,
            environment: BTreeMap::new(),
            timeout: Duration::from_secs(600),
            tail_lines: 40,
        }
    }
}

pub(crate) fn run(request: CommandRequest<'_>) -> CommandOutcome {
    let started = Instant::now();
    let command = std::iter::once(request.program.to_string())
        .chain(request.args.iter().cloned())
        .collect::<Vec<_>>();
    let working_directory = display_path(request.current_dir);
    let mut process = Command::new(request.program);
    process
        .args(request.args)
        .current_dir(request.current_dir)
        .envs(&request.environment)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = match process.spawn() {
        Ok(child) => child,
        Err(error) => {
            return CommandOutcome {
                status: CommandStatus::Unavailable,
                command,
                working_directory,
                exit_code: None,
                duration_ms: started.elapsed().as_millis(),
                stdout_tail: String::new(),
                stderr_tail: String::new(),
                reason: Some(error.to_string()),
                stdout: String::new(),
                stderr: String::new(),
            };
        }
    };

    let stdout_reader = child.stdout.take().map(read_in_background);
    let stderr_reader = child.stderr.take().map(read_in_background);
    let deadline = started + request.timeout;
    let (status, timed_out) = loop {
        match child.try_wait() {
            Ok(Some(status)) => break (Some(status), false),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                break (child.wait().ok(), true);
            }
            Err(_) => break (child.wait().ok(), false),
        }
    };
    let stdout = join_reader(stdout_reader);
    let stderr = join_reader(stderr_reader);
    let execution_status = if timed_out {
        CommandStatus::TimedOut
    } else if status
        .as_ref()
        .is_some_and(std::process::ExitStatus::success)
    {
        CommandStatus::Passed
    } else {
        CommandStatus::Failed
    };
    CommandOutcome {
        status: execution_status,
        command,
        working_directory,
        exit_code: status.and_then(|status| status.code()),
        duration_ms: started.elapsed().as_millis(),
        stdout_tail: tail(&stdout, request.tail_lines),
        stderr_tail: tail(&stderr, request.tail_lines),
        reason: timed_out
            .then(|| format!("command exceeded {} seconds", request.timeout.as_secs())),
        stdout,
        stderr,
    }
}

fn read_in_background<R>(mut reader: R) -> thread::JoinHandle<Vec<u8>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = reader.read_to_end(&mut bytes);
        bytes
    })
}

fn join_reader(reader: Option<thread::JoinHandle<Vec<u8>>>) -> String {
    reader
        .and_then(|reader| reader.join().ok())
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default()
}

fn display_path(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| PathBuf::from(path))
        .to_string_lossy()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{CommandRequest, CommandStatus, run};

    #[test]
    fn records_successful_commands() {
        let args = vec!["--version".to_string()];
        let outcome = run(CommandRequest::new(
            "cargo",
            &args,
            std::path::Path::new("."),
        ));
        assert_eq!(outcome.status, CommandStatus::Passed);
        assert_eq!(outcome.exit_code, Some(0));
    }

    #[test]
    fn unavailable_commands_are_explicit() {
        let outcome = run(CommandRequest::new(
            "rqlens-command-that-does-not-exist",
            &[],
            std::path::Path::new("."),
        ));
        assert_eq!(outcome.status, CommandStatus::Unavailable);
        assert!(outcome.reason.is_some());
    }
}
