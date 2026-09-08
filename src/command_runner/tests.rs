use super::{CommandRequest, CommandStatus, run};
use std::path::Path;
use std::time::{Duration, Instant};

#[test]
fn records_successful_commands() {
    let args = vec!["--version".to_string()];
    let outcome = run(CommandRequest::new("cargo", &args, Path::new(".")));
    assert_eq!(outcome.status, CommandStatus::Passed);
    assert_eq!(outcome.exit_code, Some(0));
    assert!(outcome.stdout.starts_with("cargo "));
    assert!(!outcome.stdout_truncated);
}

#[test]
fn unavailable_commands_are_explicit() {
    let outcome = run(CommandRequest::new(
        "rqlens-command-that-does-not-exist",
        &[],
        Path::new("."),
    ));
    assert_eq!(outcome.status, CommandStatus::Unavailable);
    assert!(outcome.reason.is_some());
}

#[cfg(unix)]
fn shell(script: &str, timeout: Duration, limit: u64) -> super::CommandOutcome {
    let args = vec!["-c".to_string(), script.to_string()];
    let mut request = CommandRequest::new("sh", &args, Path::new("."));
    request.timeout = timeout;
    request.max_output_bytes = limit;
    run(request)
}

#[cfg(unix)]
#[test]
fn timeout_terminates_descendants_holding_output_handles() {
    let started = Instant::now();
    let outcome = shell("sleep 30 & wait", Duration::from_millis(100), 1024);
    assert_eq!(outcome.status, CommandStatus::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[cfg(unix)]
#[test]
fn successful_parent_with_live_descendant_does_not_hang_capture() {
    let started = Instant::now();
    let outcome = shell("sleep 30 & printf done", Duration::from_millis(100), 1024);
    assert_eq!(outcome.status, CommandStatus::Passed);
    assert_eq!(outcome.stdout, "done");
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "freebsd"))]
#[test]
fn successful_parent_descendants_are_cleaned_up() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let args = vec![
        "-c".to_string(),
        "(sleep 0.3; printf leaked > marker) & printf done".to_string(),
    ];
    let outcome = run(CommandRequest::new("sh", &args, root.path()));
    assert_eq!(outcome.status, CommandStatus::Passed);
    std::thread::sleep(Duration::from_millis(500));
    assert!(!root.path().join("marker").exists());
    Ok(())
}

#[cfg(unix)]
#[test]
fn noisy_commands_fail_with_bounded_output() {
    let started = Instant::now();
    let outcome = shell(
        "while :; do printf 'large-output-line\\n'; done",
        Duration::from_secs(3),
        4096,
    );
    assert_eq!(outcome.status, CommandStatus::Failed);
    assert!(outcome.stdout_truncated);
    assert!(outcome.stdout.len() <= 4096);
    assert!(
        outcome
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("output limit"))
    );
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[cfg(unix)]
#[test]
fn completed_commands_exceeding_output_limit_cannot_pass() {
    let outcome = shell(
        "printf 1234567890; printf abcdefghij >&2",
        Duration::from_secs(3),
        4,
    );
    assert_eq!(outcome.status, CommandStatus::Failed);
    assert!(outcome.stdout_truncated && outcome.stderr_truncated);
    assert_eq!(outcome.stdout, "1234");
    assert_eq!(outcome.stderr, "abcd");
    assert_eq!(outcome.stdout_tail, "7890");
    assert_eq!(outcome.stderr_tail, "ghij");
}

#[cfg(unix)]
#[test]
fn captures_both_streams_environment_working_directory_and_exit_status() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let args = vec![
        "-c".to_string(),
        "printf '%s' \"$RQLENS_RUNNER_TEST\"; pwd >&2; exit 7".to_string(),
    ];
    let mut request = CommandRequest::new("sh", &args, root.path());
    request
        .environment
        .insert("RQLENS_RUNNER_TEST".to_string(), "value".to_string());
    let outcome = run(request);
    assert_eq!(outcome.status, CommandStatus::Failed);
    assert_eq!(outcome.exit_code, Some(7));
    assert_eq!(outcome.stdout, "value");
    assert_eq!(outcome.stderr.trim(), root.path().to_string_lossy());
    Ok(())
}

#[cfg(unix)]
#[test]
fn checked_commands_reject_failure_instead_of_exposing_partial_json() {
    let mut command = std::process::Command::new("sh");
    command.args(["-c", "printf '{'; exit 1"]);
    assert!(super::checked_output(&mut command, Duration::from_secs(1)).is_err());
}

#[cfg(windows)]
#[test]
fn timeout_terminates_descendants_holding_output_handles() {
    let args = vec!["/C".to_string(), "ping -n 30 127.0.0.1".to_string()];
    let mut request = CommandRequest::new("cmd", &args, Path::new("."));
    request.timeout = Duration::from_millis(100);
    let started = Instant::now();
    assert_eq!(run(request).status, CommandStatus::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(3));
}
