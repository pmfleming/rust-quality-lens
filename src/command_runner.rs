use serde::Serialize;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
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
    configure_process_group(&mut process);
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
                terminate_process_tree(&mut child);
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

#[cfg(unix)]
fn configure_process_group(process: &mut Command) {
    use std::os::unix::process::CommandExt;
    process.process_group(0);
}

#[cfg(not(unix))]
fn configure_process_group(_process: &mut Command) {}

fn terminate_process_tree(child: &mut Child) {
    #[cfg(unix)]
    {
        // The child is the leader of the process group configured above. Killing
        // the group closes pipes inherited by Cargo's rustc and test descendants.
        let group = format!("-{}", child.id());
        let _ = Command::new("kill")
            .args(["-KILL", "--", &group])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(windows)]
    {
        let pid = child.id().to_string();
        let _ = Command::new("taskkill")
            .args(["/PID", pid.as_str(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    // Keep a direct-child fallback for platforms without a tree-kill utility and
    // for the race where the process exits before the utility runs.
    let _ = child.kill();
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

    #[cfg(unix)]
    #[test]
    fn timeout_terminates_descendants_holding_output_pipes() {
        assert_descendant_timeout("sh", vec!["-c".to_string(), "sleep 30 & wait".to_string()]);
    }

    #[cfg(windows)]
    #[test]
    fn timeout_terminates_descendants_holding_output_pipes() {
        assert_descendant_timeout(
            "cmd",
            vec!["/C".to_string(), "ping -n 30 127.0.0.1".to_string()],
        );
    }

    #[cfg(any(unix, windows))]
    fn assert_descendant_timeout(program: &str, args: Vec<String>) {
        let mut request = CommandRequest::new(program, &args, std::path::Path::new("."));
        request.timeout = std::time::Duration::from_millis(100);
        let started = std::time::Instant::now();

        let outcome = run(request);

        assert_eq!(outcome.status, CommandStatus::TimedOut);
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }
}
