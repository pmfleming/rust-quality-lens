use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus};
use std::thread;
use std::time::{Duration, Instant};

mod capture;
#[cfg(test)]
mod tests;

const DEFAULT_OUTPUT_LIMIT: u64 = 64 * 1024 * 1024;

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
    pub(crate) stdout_truncated: bool,
    pub(crate) stderr_truncated: bool,
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
    pub(crate) max_output_bytes: u64,
}

impl<'a> CommandRequest<'a> {
    pub(crate) fn new(program: &'a str, args: &'a [String], current_dir: &'a Path) -> Self {
        Self {
            program,
            args,
            current_dir,
            environment: BTreeMap::new(),
            timeout: Duration::from_mins(10),
            tail_lines: 40,
            max_output_bytes: DEFAULT_OUTPUT_LIMIT,
        }
    }
}

pub(crate) fn run(request: CommandRequest<'_>) -> CommandOutcome {
    let mut command = Command::new(request.program);
    command
        .args(request.args)
        .current_dir(request.current_dir)
        .envs(&request.environment);
    execute(
        &mut command,
        request.timeout,
        request.max_output_bytes,
        request.tail_lines,
    )
}

// For finite commands that cannot use partial output. Preserve native paths and
// environment values through Command rather than converting them to UTF-8.
pub(crate) fn checked_output(
    command: &mut Command,
    timeout: Duration,
) -> anyhow::Result<CommandOutcome> {
    let outcome = execute(command, timeout, DEFAULT_OUTPUT_LIMIT, 40);
    anyhow::ensure!(
        outcome.status == CommandStatus::Passed,
        "command {:?} {:?}: {}\n{}",
        outcome.command,
        outcome.status,
        outcome.reason.as_deref().unwrap_or("nonzero exit status"),
        outcome.stderr_tail
    );
    Ok(outcome)
}

fn execute(
    command: &mut Command,
    timeout: Duration,
    limit: u64,
    tail_lines: usize,
) -> CommandOutcome {
    let started = Instant::now();
    let mut outcome = CommandOutcome {
        status: CommandStatus::Unavailable,
        command: std::iter::once(command.get_program())
            .chain(command.get_args())
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect(),
        working_directory: display_path(
            command.get_current_dir().unwrap_or_else(|| Path::new(".")),
        ),
        exit_code: None,
        duration_ms: 0,
        stdout_tail: String::new(),
        stderr_tail: String::new(),
        stdout_truncated: false,
        stderr_truncated: false,
        reason: None,
        stdout: String::new(),
        stderr: String::new(),
    };
    let capture = match capture::Capture::new(command, limit) {
        Ok(capture) => capture,
        Err(error) => {
            outcome.reason = Some(format!("creating output capture: {error}"));
            outcome.duration_ms = started.elapsed().as_millis();
            return outcome;
        }
    };
    configure_process_group(command);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            outcome.reason = Some(error.to_string());
            outcome.duration_ms = started.elapsed().as_millis();
            return outcome;
        }
    };
    let exited = loop {
        match has_exited(&mut child) {
            Ok(true) => break true,
            Err(error) => {
                outcome.reason = Some(format!("waiting for command: {error}"));
                break false;
            }
            Ok(false) => {}
        }
        if started.elapsed() >= timeout {
            outcome.status = CommandStatus::TimedOut;
            outcome.reason = Some(format!(
                "command exceeded {} milliseconds",
                timeout.as_millis()
            ));
            break false;
        }
        match capture.exceeds_limit() {
            Ok(false) => {}
            Ok(true) => {
                outcome.reason = Some(format!(
                    "command exceeded output limit of {limit} bytes per stream"
                ));
                break false;
            }
            Err(error) => {
                outcome.reason = Some(format!("inspecting command output: {error}"));
                break false;
            }
        }
        thread::sleep(Duration::from_millis(10));
    };
    // Also clean up descendants after a successful direct-child exit. Output is
    // disk-backed, so inherited handles can never block a reader waiting for EOF.
    terminate_process_tree(&mut child, exited);
    let status = reap(child);
    outcome.exit_code = status.as_ref().and_then(ExitStatus::code);
    if outcome.status != CommandStatus::TimedOut {
        outcome.status =
            if outcome.reason.is_none() && status.is_some_and(|status| status.success()) {
                CommandStatus::Passed
            } else {
                CommandStatus::Failed
            };
    }
    match capture.read(tail_lines) {
        Ok((stdout, stderr)) => {
            outcome.stdout = stdout.text;
            outcome.stderr = stderr.text;
            outcome.stdout_tail = stdout.tail;
            outcome.stderr_tail = stderr.tail;
            outcome.stdout_truncated = stdout.truncated;
            outcome.stderr_truncated = stderr.truncated;
            if stdout.truncated || stderr.truncated {
                outcome.reason.get_or_insert_with(|| {
                    format!("command exceeded output limit of {limit} bytes per stream")
                });
                if outcome.status != CommandStatus::TimedOut {
                    outcome.status = CommandStatus::Failed;
                }
            }
        }
        Err(error) => {
            outcome
                .reason
                .get_or_insert_with(|| format!("reading command output: {error}"));
            if outcome.status != CommandStatus::TimedOut {
                outcome.status = CommandStatus::Failed;
            }
        }
    }
    outcome.duration_ms = started.elapsed().as_millis();
    outcome
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "freebsd"))]
fn has_exited(child: &mut Child) -> std::io::Result<bool> {
    use rustix::process::{Pid, WaitId, WaitIdOptions, waitid};
    let pid = Pid::from_raw(child.id() as i32)
        .ok_or_else(|| std::io::Error::other("invalid child PID"))?;
    // Reserve the PID until tree cleanup, avoiding a PID-reuse race between
    // observing the leader's exit and signalling its process group.
    waitid(
        WaitId::Pid(pid),
        WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
    )
    .map(|status| status.is_some())
    .map_err(Into::into)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "freebsd")))]
fn has_exited(child: &mut Child) -> std::io::Result<bool> {
    child.try_wait().map(|status| status.is_some())
}

fn reap(mut child: Child) -> Option<ExitStatus> {
    let deadline = Instant::now() + Duration::from_millis(250);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(5)),
            _ => {
                // Reap eventually without making the caller's deadline depend on
                // an uninterruptible process or a failed termination attempt.
                thread::spawn(move || {
                    let _ = child.wait();
                });
                return None;
            }
        }
    }
}

#[cfg(unix)]
fn configure_process_group(process: &mut Command) {
    use std::os::unix::process::CommandExt;
    process.process_group(0);
}

#[cfg(not(unix))]
fn configure_process_group(_process: &mut Command) {}

fn terminate_process_tree(child: &mut Child, _exited: bool) {
    // On platforms without non-reaping wait, an exited child's PID may already
    // have been reused. Do not signal that PID; file capture remains bounded.
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "freebsd")))]
    if _exited {
        return;
    }
    #[cfg(unix)]
    if let Some(pid) = rustix::process::Pid::from_raw(child.id() as i32) {
        // No shell utility or PATH lookup is involved in timeout cleanup.
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
    }
    #[cfg(windows)]
    {
        // Best effort on Windows; disk-backed capture does not depend on tree
        // termination succeeding in order to return within the deadline.
        if let Ok(mut killer) = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            let deadline = Instant::now() + Duration::from_millis(250);
            while matches!(killer.try_wait(), Ok(None)) && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(5));
            }
            let _ = killer.kill();
            reap(killer);
        }
    }
    let _ = child.kill();
}

fn display_path(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| PathBuf::from(path))
        .to_string_lossy()
        .into_owned()
}
