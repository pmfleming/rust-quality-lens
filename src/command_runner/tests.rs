use super::{CommandRequest, CommandStatus, run};
use std::path::Path;
use std::time::{Duration, Instant};

#[test]
fn command_availability_and_success_are_explicit() {
    let args = vec!["--version".to_string()];
    let outcome = run(CommandRequest::new("cargo", &args, Path::new(".")));
    assert_eq!(outcome.status, CommandStatus::Passed);
    assert_eq!(outcome.exit_code, Some(0));
    assert!(outcome.stdout.starts_with("cargo "));
    assert!(!outcome.stdout_truncated);
    assert!(outcome.completed());
    let outcome = run(CommandRequest::new(
        "rqlens-command-that-does-not-exist",
        &[],
        Path::new("."),
    ));
    assert_eq!(outcome.status, CommandStatus::Unavailable);
    assert!(outcome.reason.is_some());
    assert!(!outcome.completed());
    assert!(super::cargo_artifacts("not JSON").is_err());
    assert!(super::cargo_artifacts(r#"{"reason":"compiler-artifact","package_id":3}"#).is_err());
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
fn descendants_cannot_hang_capture_after_timeout_or_parent_exit() {
    for (script, status, stdout) in [
        ("sleep 30 & wait", CommandStatus::TimedOut, ""),
        ("sleep 30 & printf done", CommandStatus::Passed, "done"),
    ] {
        let started = Instant::now();
        let outcome = shell(script, Duration::from_millis(100), 1024);
        assert_eq!(outcome.status, status, "{script}");
        assert_eq!(outcome.stdout, stdout, "{script}");
        assert!(started.elapsed() < Duration::from_secs(3), "{script}");
    }
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
    assert!(!outcome.completed());
    assert!(outcome.stdout.len() <= 4096);
    assert!(
        outcome
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("output limit"))
    );
    assert!(started.elapsed() < Duration::from_secs(3));
    // Finishing before observation must not bypass the same output limit.
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
    // The checked consumer must not turn failed output into usable JSON.
    let mut command = std::process::Command::new("sh");
    command.args(["-c", "printf '{'; exit 1"]);
    assert!(super::checked_output(&mut command, Duration::from_secs(1)).is_err());
    Ok(())
}

#[test]
fn timeout_terminates_descendants_holding_output_handles() {
    let (program, arguments) = if cfg!(windows) {
        ("cmd", ["/C", "ping -n 30 127.0.0.1"])
    } else {
        ("sh", ["-c", "sleep 30 & wait"])
    };
    let args = arguments.map(str::to_string);
    let mut request = CommandRequest::new(program, &args, Path::new("."));
    request.timeout = Duration::from_millis(100);
    let started = Instant::now();
    assert_eq!(run(request).status, CommandStatus::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(3));
}
