//! Repeated libtest runs attributed by Cargo package, target and qualified test name.
use crate::{
    command_runner::{CommandOutcome, CommandRequest, CommandStatus, run},
    config::LensConfig,
    util::round2,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Duration};

pub(super) fn check(config: &LensConfig) -> Value {
    let runs = config.verification.flaky_test_runs;
    let rule = "rust.tests.flaky-repeat";
    let source = "https://doc.rust-lang.org/cargo/commands/cargo-test.html";
    if runs <= 1 {
        return super::skipped_check(
            rule,
            "Repeated test runs are stable",
            "set verification.flaky_test_runs above 1 to detect intermittent failures",
            source,
        );
    }
    let mut args = config.verification.cargo_arguments("test", true, false);
    args.extend(["--no-run".into(), "--message-format=json".into()]);
    let build = execute(config, "cargo", &args);
    let mut reasons = Vec::new();
    let targets = if build.status == CommandStatus::Passed {
        match executables(&build.stdout) {
            Ok(targets) => targets,
            Err(error) => {
                reasons.push(error.to_string());
                Vec::new()
            }
        }
    } else {
        reasons.push("test executable discovery failed".into());
        Vec::new()
    };
    let mut evidence = vec![json!({"phase": "build", "command": build})];
    let mut tests = Vec::new();
    let mut any_failed = false;
    let mut any_timeout = false;
    for target in targets {
        let list = execute_target(
            config,
            &target,
            &["--list".into(), "--format=pretty".into()],
        );
        let inventory = if list.status == CommandStatus::Passed {
            parse_inventory(&list.stdout)
        } else {
            Err(anyhow::anyhow!(
                "test listing did not complete successfully"
            ))
        };
        let inventory = match inventory {
            Ok(inventory) => inventory,
            Err(error) => {
                reasons.push(format!("{}: {error}", target.id));
                evidence.push(json!({"target": target.id, "phase": "list", "command": list}));
                continue;
            }
        };
        evidence.push(json!({"target": target.id, "phase": "list", "command": list}));
        let mut counts: BTreeMap<String, Counts> = inventory
            .into_iter()
            .map(|name| (name, Counts::default()))
            .collect();
        for iteration in 0..runs {
            let outcome = execute_target(config, &target, &["--format=pretty".into()]);
            any_failed |= outcome.status == CommandStatus::Failed;
            any_timeout |= outcome.status == CommandStatus::TimedOut;
            let statuses = parse_results(&outcome.stdout);
            let complete = record_run(&mut counts, &statuses);
            if !complete || outcome.reason.is_some() || !matches!(outcome.exit_code, Some(0 | 101))
            {
                reasons.push(format!(
                    "{} run {}: incomplete libtest results or abnormal execution",
                    target.id,
                    iteration + 1
                ));
            }
            evidence.push(json!({"target": target.id, "phase": "run", "iteration": iteration + 1, "command": outcome}));
        }
        tests.extend(
            counts
                .into_iter()
                .map(|(name, counts)| counts.row(&target.id, &name, runs)),
        );
    }
    let status = if build.status != CommandStatus::Passed {
        build.status
    } else if any_timeout {
        CommandStatus::TimedOut
    } else if any_failed {
        CommandStatus::Failed
    } else if !reasons.is_empty() {
        CommandStatus::Unavailable
    } else {
        CommandStatus::Passed
    };
    json!({"rule_id": rule, "title": "Repeated test runs are stable", "category": "verified-gate",
        "severity": "error", "status": status, "source": source, "tool": "cargo",
        "tool_version": super::cargo_version(), "evidence": {"requested_runs": runs, "commands": evidence},
        "metrics": {"model_version": 1, "scope": "Cargo-discovered libtest executables; doctests excluded",
            "summary": {"test_count": tests.len(), "flaky_test_count": tests.iter().filter(|test| test["flaky"] == true).count()},
            "tests": tests, "measurement_confidence": super::behavior_confidence("per_test_repeated_execution", &reasons)}})
}

fn execute(config: &LensConfig, program: &str, args: &[String]) -> CommandOutcome {
    let mut request = CommandRequest::new(program, args, &config.project_root);
    request.timeout = Duration::from_secs(config.verification.timeout_seconds);
    run(request)
}

fn execute_target(config: &LensConfig, target: &Target, args: &[String]) -> CommandOutcome {
    let mut request = CommandRequest::new(&target.executable, args, &target.directory);
    request.timeout = Duration::from_secs(config.verification.timeout_seconds);
    request.environment.insert(
        "CARGO_MANIFEST_DIR".into(),
        target.directory.to_string_lossy().into_owned(),
    );
    run(request)
}

struct Target {
    id: String,
    executable: String,
    directory: std::path::PathBuf,
}

fn executables(stdout: &str) -> Result<Vec<Target>> {
    let mut targets = BTreeMap::new();
    for line in stdout.lines() {
        let message: Value =
            serde_json::from_str(line).context("invalid Cargo JSON during test discovery")?;
        if message["reason"] != "compiler-artifact" || message["profile"]["test"] != true {
            continue;
        }
        let Some(executable) = message["executable"].as_str() else {
            continue;
        };
        let package = message["package_id"]
            .as_str()
            .context("test artifact lacks package identity")?;
        let name = message["target"]["name"]
            .as_str()
            .context("test artifact lacks target name")?;
        let kind = &message["target"]["kind"];
        ensure!(kind.is_array(), "test artifact lacks target kind");
        let id = format!("{package}::{kind}::{name}");
        targets.insert(
            id.clone(),
            Target {
                id,
                executable: executable.to_string(),
                directory: std::path::Path::new(
                    message["manifest_path"]
                        .as_str()
                        .context("test artifact lacks manifest path")?,
                )
                .parent()
                .context("invalid test manifest path")?
                .to_path_buf(),
            },
        );
    }
    ensure!(
        !targets.is_empty(),
        "no supported test executables were discovered"
    );
    Ok(targets.into_values().collect())
}

fn parse_inventory(stdout: &str) -> Result<Vec<String>> {
    let summary = regex::Regex::new(r"^(\d+) tests?, (\d+) benchmarks?$")?;
    let mut names = Vec::new();
    let mut expected = None;
    for line in stdout.lines().filter(|line| !line.is_empty()) {
        if let Some(name) = line.strip_suffix(": test") {
            names.push(name.to_string());
        } else if let Some(captures) = summary.captures(line) {
            ensure!(
                &captures[2] == "0",
                "benchmark harness attribution is unsupported"
            );
            expected = Some(captures[1].parse::<usize>()?);
        } else {
            anyhow::bail!("unsupported libtest listing: {line}");
        }
    }
    ensure!(
        expected == Some(names.len()),
        "missing or inconsistent libtest inventory summary"
    );
    let original = names.len();
    names.sort();
    names.dedup();
    ensure!(
        names.len() == original,
        "duplicate names within test executable"
    );
    Ok(names)
}

fn parse_results(stdout: &str) -> BTreeMap<String, Vec<String>> {
    let mut statuses: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for line in stdout.lines() {
        if let Some((name, status)) = line
            .strip_prefix("test ")
            .and_then(|line| line.rsplit_once(" ... "))
        {
            let status = if status.starts_with("ignored") {
                "ignored"
            } else {
                status
            };
            if matches!(status, "ok" | "FAILED" | "ignored") {
                statuses
                    .entry(name.to_string())
                    .or_default()
                    .push(status.to_string());
            }
        }
    }
    statuses
}

#[derive(Default)]
struct Counts {
    passed: usize,
    failed: usize,
    ignored: usize,
    missing: usize,
}

fn record_run(
    counts: &mut BTreeMap<String, Counts>,
    statuses: &BTreeMap<String, Vec<String>>,
) -> bool {
    let mut complete = statuses.keys().all(|name| counts.contains_key(name));
    for (name, count) in counts {
        match statuses.get(name).map(Vec::as_slice) {
            Some([status]) if status == "ok" => count.passed += 1,
            Some([status]) if status == "FAILED" => count.failed += 1,
            Some([status]) if status == "ignored" => count.ignored += 1,
            _ => {
                count.missing += 1;
                complete = false;
            }
        }
    }
    complete
}

impl Counts {
    fn row(&self, target: &str, name: &str, requested: usize) -> Value {
        let executed = self.passed + self.failed;
        json!({"target_id": target, "qualified_name": name, "requested_runs": requested,
            "passed_runs": self.passed, "failed_runs": self.failed, "ignored_runs": self.ignored,
            "missing_runs": self.missing, "executed_runs": executed,
            "observed_failure_rate_percent": (executed > 0).then(|| round2(100.0 * self.failed as f64 / executed as f64)),
            "flaky": self.passed > 0 && self.failed > 0, "complete": self.missing == 0,
            "duration_seconds": null, "duration_status": "unavailable in stable libtest output"})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_results_preserve_missing_and_ignored_observations() -> Result<()> {
        let names = parse_inventory("same: test\nignored: test\n\n2 tests, 0 benchmarks\n")?;
        let mut counts = names
            .into_iter()
            .map(|name| (name, Counts::default()))
            .collect();
        assert!(record_run(
            &mut counts,
            &parse_results("test same ... ok\ntest ignored ... ignored, reason\n")
        ));
        assert!(record_run(
            &mut counts,
            &parse_results("test same ... FAILED\ntest ignored ... ignored\n")
        ));
        assert!(!record_run(
            &mut counts,
            &parse_results("test ignored ... ignored\n")
        ));
        let row = counts["same"].row("package::target", "same", 3);
        assert_eq!(row["observed_failure_rate_percent"], 50.0);
        assert_eq!(row["missing_runs"], 1);
        assert_eq!(row["flaky"], true);
        assert!(
            counts["ignored"].row("target", "ignored", 3)["observed_failure_rate_percent"]
                .is_null()
        );
        assert!(parse_inventory("custom harness output").is_err());
        assert!(parse_inventory("x: test\nx: test\n2 tests, 0 benchmarks").is_err());
        assert!(parse_inventory("0 tests, 1 benchmark").is_err());
        Ok(())
    }
    #[test]
    fn package_and_target_identity_prevent_name_collisions() -> Result<()> {
        let rows = ["a", "b"].map(|package| json!({"reason": "compiler-artifact", "package_id": package,
            "target": {"name": "same", "kind": ["test"]}, "profile": {"test": true}, "executable": "/tmp/test", "manifest_path": "/tmp/Cargo.toml"}).to_string());
        let targets = executables(&rows.join("\n"))?;
        assert_eq!(targets.len(), 2);
        assert_ne!(targets[0].id, targets[1].id);
        Ok(())
    }
    #[test]
    fn live_libtest_attribution() -> Result<()> {
        let root = tempfile::tempdir()?;
        std::fs::create_dir(root.path().join("src"))?;
        std::fs::write(
            root.path().join("Cargo.toml"),
            "[package]\nname='repeat-fixture'\nversion='0.1.0'\n",
        )?;
        std::fs::write(
            root.path().join("src/lib.rs"),
            "#[test] fn pass() {}\n#[test] #[ignore] fn ignored() {}",
        )?;
        let mut config = LensConfig::test_config("repeat", root.path());
        config.verification.flaky_test_runs = 2;
        let result = check(&config);
        assert_eq!(result["status"], "passed", "{result:#}");
        assert_eq!(result["metrics"]["summary"]["test_count"], 2);
        assert_eq!(
            result["metrics"]["measurement_confidence"]["complete"],
            true
        );
        Ok(())
    }
}
