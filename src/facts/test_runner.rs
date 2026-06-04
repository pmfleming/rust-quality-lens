use anyhow::Result;
use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use crate::config::LensConfig;
use crate::util::{dedupe, resolve_project_path, tail};

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
    let started = Instant::now();
    let output = Command::new("cargo")
        .arg("test")
        .current_dir(&config.project_root)
        .output()?;
    let duration = started.elapsed().as_secs_f64();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut statuses = parse_cargo_test_statuses(&stdout);
    statuses.insert(
        "__run__".to_string(),
        TestStatus {
            status: run_status(output.status.success()),
            duration: Some(duration),
            stdout_tail: Some(tail(&stdout, 40)),
            stderr_tail: Some(tail(&stderr, 40)),
        },
    );
    Ok(statuses)
}

fn run_status(success: bool) -> String {
    if success { "passed" } else { "failed" }.to_string()
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
#[allow(clippy::unwrap_used)]
mod tests {
    use super::correctness_paths;
    use crate::config::LensConfig;
    use crate::util::normalize_slashes;
    use std::path::PathBuf;

    #[test]
    fn cargo_target_paths_are_read_from_toml_structure() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("src")).unwrap();
        std::fs::create_dir_all(root.path().join("tests/fixtures")).unwrap();
        std::fs::write(root.path().join("src/lib.rs"), "").unwrap();
        std::fs::write(root.path().join("tests/smoke.rs"), "").unwrap();
        std::fs::write(root.path().join("tests/fixtures/not_a_target.rs"), "").unwrap();
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
        )
        .unwrap();
        let config = LensConfig {
            project_name: "target-paths".to_string(),
            project_root: root.path().to_path_buf(),
            source_roots: vec![root.path().join("src").to_string_lossy().to_string()],
            output_dir: root.path().join("target/analysis"),
            helper_manifest: PathBuf::from("unused"),
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
    }
}
