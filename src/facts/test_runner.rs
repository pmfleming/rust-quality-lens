use anyhow::Result;
use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::command_runner::{CommandRequest, CommandStatus, run};
use crate::config::LensConfig;
use crate::util::{dedupe, resolve_project_path};

#[derive(Clone)]
pub(crate) struct TestStatus {
    pub(crate) status: String,
    pub(crate) duration: Option<f64>,
    pub(crate) stdout_tail: Option<String>,
    pub(crate) stderr_tail: Option<String>,
}

pub(crate) fn correctness_paths(config: &LensConfig) -> Vec<String> {
    let mut paths = config.source_roots.clone();
    for extra in ["tests", "benches", "examples"] {
        paths.extend(top_level_rust_files(&config.project_root.join(extra)));
    }
    paths.extend(cargo_target_paths(config));
    dedupe(paths)
}

fn top_level_rust_files(path: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "rs"))
        .map(|path| path.to_string_lossy().to_string())
        .collect()
}

fn cargo_target_paths(config: &LensConfig) -> Vec<String> {
    let cargo = config.project_root.join("Cargo.toml");
    let Ok(text) = std::fs::read_to_string(cargo) else {
        return Vec::new();
    };
    let Ok(value) = text.parse::<toml::Value>() else {
        return Vec::new();
    };
    ["bin", "test", "bench", "example"]
        .into_iter()
        .flat_map(|target| target_paths(&value, target, &config.project_root))
        .collect()
}

fn target_paths(value: &toml::Value, target: &str, project_root: &Path) -> Vec<String> {
    value
        .get(target)
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("path").and_then(toml::Value::as_str))
        .map(|path| {
            resolve_project_path(PathBuf::from(path), project_root)
                .to_string_lossy()
                .to_string()
        })
        .collect()
}

pub(crate) fn run_tests(config: &LensConfig) -> Result<HashMap<String, TestStatus>> {
    let arguments = config.verification.cargo_arguments("test", false, false);
    let mut request = CommandRequest::new("cargo", &arguments, &config.project_root);
    request.timeout = Duration::from_secs(config.verification.timeout_seconds);
    let outcome = run(request);
    let mut statuses = parse_cargo_test_statuses(&outcome.stdout);
    let status = match outcome.status {
        CommandStatus::Passed => run_status(true, &outcome.stdout, &outcome.stderr),
        CommandStatus::Failed => run_status(false, &outcome.stdout, &outcome.stderr),
        CommandStatus::Unavailable => "unavailable".to_string(),
        CommandStatus::TimedOut => "timed_out".to_string(),
    };
    statuses.insert(
        "__run__".to_string(),
        TestStatus {
            status,
            duration: Some(outcome.duration_ms as f64 / 1000.0),
            stdout_tail: Some(outcome.stdout_tail),
            stderr_tail: Some(outcome.stderr_tail),
        },
    );
    Ok(statuses)
}

fn run_status(success: bool, stdout: &str, stderr: &str) -> String {
    if success {
        "passed"
    } else if stdout.contains("could not compile") || stderr.contains("could not compile") {
        "compile_failed"
    } else {
        "failed"
    }
    .to_string()
}

fn parse_cargo_test_statuses(stdout: &str) -> HashMap<String, TestStatus> {
    let Ok(running_re) =
        Regex::new(r#"\s*Running\s+(?:(?:unittests|tests?)\s+)?(.+?\.rs)(?:\s+\(|$)"#)
    else {
        return HashMap::new();
    };
    let Ok(status_re) = Regex::new(r"test\s+(.+?)\s+\.\.\.\s+(ok|FAILED|ignored)") else {
        return HashMap::new();
    };
    test_statuses_from_lines(stdout.lines(), &running_re, &status_re)
}

fn test_statuses_from_lines<'a>(
    lines: impl Iterator<Item = &'a str>,
    running_re: &Regex,
    status_re: &Regex,
) -> HashMap<String, TestStatus> {
    let mut current_path = None;
    let mut statuses = HashMap::new();
    for line in lines {
        if let Some(cap) = running_re.captures(line) {
            current_path = Some(cap[1].trim_matches('"').replace('\\', "/"));
        } else if let Some(cap) = status_re.captures(line.trim()) {
            record_test_status(&mut statuses, current_path.as_deref(), &cap[1], &cap[2]);
        }
    }
    statuses
}

fn record_test_status(
    statuses: &mut HashMap<String, TestStatus>,
    current_path: Option<&str>,
    name: &str,
    status: &str,
) {
    let record = TestStatus {
        status: test_status(status),
        duration: None,
        stdout_tail: None,
        stderr_tail: None,
    };
    statuses.insert(name.to_string(), record.clone());
    if let Some(path) = current_path {
        statuses.insert(format!("{path}::{name}"), record);
    }
}

fn test_status(status: &str) -> String {
    match status {
        "ok" => "passed",
        "FAILED" => "failed",
        "ignored" => "skipped",
        _ => "unknown",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::{correctness_paths, run_status};
    use crate::config::{LensConfig, SemanticIdentityMode};
    use crate::util::normalize_slashes;
    use std::path::PathBuf;

    #[test]
    fn cargo_target_paths_are_read_from_toml_structure() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        std::fs::create_dir_all(root.path().join("src"))?;
        std::fs::create_dir_all(root.path().join("tests/fixtures"))?;
        std::fs::write(root.path().join("src/lib.rs"), "")?;
        std::fs::write(root.path().join("tests/smoke.rs"), "")?;
        std::fs::write(root.path().join("tests/fixtures/not_a_target.rs"), "")?;
        std::fs::write(
            root.path().join("Cargo.toml"),
            r#"
[package]
name = "target-paths"
version = "0.1.0"

[package.metadata]
path = "not-a-rust-target.rs"

[[bin]]
name = "tool"
path = "src/bin/tool.rs"

[[test]]
name = "integration"
path = "tests/integration.rs"
"#,
        )?;
        let config = LensConfig {
            project_name: "target-paths".to_string(),
            project_root: root.path().to_path_buf(),
            source_roots: vec![root.path().join("src").to_string_lossy().to_string()],
            output_dir: root.path().join("target/analysis"),
            helper_manifest: PathBuf::from("unused"),
            identity_resolution: SemanticIdentityMode::Disabled,
            rust_analyzer: PathBuf::from("rust-analyzer"),
            identity_timeout_seconds: 1,
            identity_offline: true,
            verification: Default::default(),
        };
        let paths = correctness_paths(&config);
        assert!(
            paths
                .iter()
                .any(|path| normalize_slashes(path).ends_with("src/bin/tool.rs"))
        );
        assert!(
            paths
                .iter()
                .any(|path| normalize_slashes(path).ends_with("tests/integration.rs"))
        );
        assert!(
            paths
                .iter()
                .any(|path| normalize_slashes(path).ends_with("tests/smoke.rs"))
        );
        assert!(
            !paths
                .iter()
                .any(|path| normalize_slashes(path).contains("tests/fixtures"))
        );
        assert!(
            !paths
                .iter()
                .any(|path| normalize_slashes(path).contains("not-a-rust-target.rs"))
        );
        Ok(())
    }

    #[test]
    fn run_status_distinguishes_compilation_failure() {
        assert_eq!(run_status(true, "", ""), "passed");
        assert_eq!(
            run_status(false, "", "error: could not compile `demo`"),
            "compile_failed"
        );
        assert_eq!(run_status(false, "test failed", ""), "failed");
    }
}
