use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use walkdir::WalkDir;

use crate::command_runner::{CommandRequest, CommandStatus, run};
use crate::config::LensConfig;
use crate::util::{normalize_slashes, round2, write_json};

pub(crate) fn run_benchmarks(
    config: &LensConfig,
    baseline: Option<PathBuf>,
    no_run: bool,
) -> Result<PathBuf> {
    // Validate the requested baseline before starting an expensive command.
    let baseline_records = baseline.as_deref().map(read_baseline).transpose()?;
    let target = cargo_target_directory(config)?;
    // Criterion honors CRITERION_HOME. A fresh directory prevents removed or
    // skipped benchmarks from being certified by an unrelated successful run.
    let criterion_root = if no_run {
        target.join("criterion")
    } else {
        let runs = target.join("rqlens-bench-runs");
        fs::create_dir_all(&runs)?;
        tempfile::tempdir_in(runs)?.keep()
    };
    let command = if no_run {
        None
    } else {
        let arguments = config.verification.cargo_arguments("bench", false, false);
        let mut request = CommandRequest::new("cargo", &arguments, &config.project_root);
        request.timeout = Duration::from_secs(config.verification.timeout_seconds);
        request.environment.insert(
            "CRITERION_HOME".to_string(),
            criterion_root.to_string_lossy().into_owned(),
        );
        Some(run(request))
    };
    let records = criterion_records(&criterion_root);
    let comparisons = baseline_records
        .as_deref()
        .map(|baseline| comparisons(&records, baseline))
        .transpose()?
        .unwrap_or_default();
    let incomparable = comparisons
        .iter()
        .any(|row| row["direction"] == "incomparable");
    let command_passed = command
        .as_ref()
        .is_none_or(|outcome| outcome.status == CommandStatus::Passed);
    let confidence = json!({
        "complete": !no_run && command_passed && !records.is_empty() && !incomparable,
        "partial": no_run || !command_passed || records.is_empty() || incomparable,
        "confidence_scope": "criterion_benchmark_execution",
        "required_inputs": ["cargo_bench", "criterion_estimates"],
        "observed_inputs": {"benchmark_count": records.len(), "command_executed": !no_run},
        "missing_input": if records.is_empty() { vec!["no Criterion estimates were found"] } else if incomparable { vec!["current and baseline benchmark IDs do not match"] } else { Vec::<&str>::new() },
        "stale_input": if no_run { vec!["benchmark execution time is unknown because --no-run reused existing estimates"] } else { Vec::<&str>::new() },
        "unsupported_pattern": if command_passed { Vec::<String>::new() } else { vec!["cargo bench did not complete successfully".to_string()] },
    });
    let document = json!({
        "schema_version": 1,
        "generated_from": "rqlens",
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "generator_version": env!("CARGO_PKG_VERSION"),
        "input_fingerprint": config.input_fingerprint(),
        "tool": "performance",
        "measurement_confidence": confidence,
        "summary": {
            "benchmark_count": records.len(),
            "comparison_count": comparisons.len(),
            "regressed": comparisons.iter().filter(|row| row["direction"] == "regressed").count(),
            "command_status": command.as_ref().map(|outcome| outcome.status),
        },
        "data": {
            "records": records,
            "comparisons": comparisons,
            "command": command,
        }
    });
    fs::create_dir_all(&config.output_dir)?;
    let output = config.output_dir.join("performance.json");
    write_json(&output, &document)?;
    Ok(output)
}

fn cargo_target_directory(config: &LensConfig) -> Result<PathBuf> {
    let mut command = std::process::Command::new("cargo");
    command
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(&config.project_root);
    if config.verification.locked {
        command.arg("--locked");
    }
    if config.identity_offline {
        command.arg("--offline");
    }
    let output = crate::command_runner::checked_output(&mut command, Duration::from_secs(30))?;
    let metadata: Value = serde_json::from_str(&output.stdout)?;
    metadata["target_directory"]
        .as_str()
        .map(PathBuf::from)
        .context("Cargo metadata is missing target_directory")
}

fn criterion_records(root: &Path) -> Vec<Value> {
    let mut rows = WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .map(|entry| entry.into_path())
        .filter(|path| normalize_slashes(path).ends_with("/new/estimates.json"))
        .filter_map(|path| criterion_record(root, &path))
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| left["id"].as_str().cmp(&right["id"].as_str()));
    rows
}

fn criterion_record(root: &Path, path: &Path) -> Option<Value> {
    let estimates: Value = serde_json::from_str(&fs::read_to_string(path).ok()?).ok()?;
    let estimate = estimates["slope"]["point_estimate"]
        .as_f64()
        .or_else(|| estimates["mean"]["point_estimate"].as_f64())?;
    let benchmark_dir = path.parent()?.parent()?;
    let id = normalize_slashes(benchmark_dir.strip_prefix(root).ok()?);
    Some(json!({
        "id": id,
        "estimate_nanoseconds": estimate,
        "source": normalize_slashes(path),
        "engine": "criterion",
    }))
}

fn read_baseline(path: &Path) -> Result<Vec<Value>> {
    let document = crate::contracts::read_baseline(path, "performance.json")?;
    document["data"]["records"]
        .as_array()
        .cloned()
        .context("missing baseline benchmark records")
}

pub(crate) fn comparisons(current: &[Value], baseline: &[Value]) -> Result<Vec<Value>> {
    let current = crate::contracts::benchmark_estimates(current)?;
    let old = crate::contracts::benchmark_estimates(baseline)?;
    current.keys().chain(old.keys()).collect::<BTreeSet<_>>().into_iter().map(|id| {
        let (Some(current), Some(baseline)) = (current.get(id), old.get(id)) else {
            return Ok(json!({"id": id, "direction": "incomparable", "reason": if current.contains_key(id) { "benchmark is missing from baseline" } else { "benchmark is missing from current run" }}));
        };
        let delta_percent = (current - baseline) / baseline * 100.0;
        anyhow::ensure!(delta_percent.is_finite(), "benchmark {id} percentage change overflows");
        Ok(json!({
            "id": id,
            "baseline_nanoseconds": baseline,
            "current_nanoseconds": current,
            "delta_percent": round2(delta_percent),
            "direction": if delta_percent > 0.0 { "regressed" } else if delta_percent < 0.0 { "improved" } else { "unchanged" },
        }))
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::{comparisons, run_benchmarks};
    use serde_json::json;

    #[test]
    fn benchmark_runs_ignore_old_estimates_and_honor_cargo_target_directory() -> anyhow::Result<()>
    {
        let root = tempfile::tempdir()?;
        std::fs::create_dir(root.path().join("src"))?;
        std::fs::create_dir(root.path().join(".cargo"))?;
        std::fs::write(root.path().join("src/lib.rs"), "pub fn value() {}")?;
        std::fs::write(
            root.path().join("Cargo.toml"),
            "[package]\nname = \"bench-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )?;
        std::fs::write(
            root.path().join(".cargo/config.toml"),
            "[build]\ntarget-dir = \"custom-target\"\n",
        )?;
        let config_path = root.path().join("rqlens.toml");
        std::fs::write(&config_path, "source_roots = [\"src\"]\n")?;
        let config = crate::config::LensConfig::load(Some(config_path))?;
        let target = super::cargo_target_directory(&config)?;
        // Cargo environment overrides are respected too; do not mutate them in tests.
        if std::env::var_os("CARGO_TARGET_DIR").is_none() {
            assert_eq!(target, root.path().join("custom-target"));
        }
        let old = target.join("criterion/removed/new/estimates.json");
        std::fs::create_dir_all(
            old.parent()
                .ok_or_else(|| anyhow::anyhow!("missing parent"))?,
        )?;
        std::fs::write(old, r#"{"mean":{"point_estimate":123}}"#)?;
        let path = run_benchmarks(&config, None, true)?;
        let reused: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
        assert_eq!(reused["measurement_confidence"]["complete"], false);
        assert_eq!(reused["data"]["records"][0]["id"], "removed");
        run_benchmarks(&config, None, false)?;
        let fresh: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        assert_eq!(fresh["summary"]["command_status"], "passed");
        assert_eq!(fresh["measurement_confidence"]["complete"], false);
        assert_eq!(fresh["data"]["records"], json!([]));
        Ok(())
    }

    #[test]
    fn benchmark_comparisons_report_relative_regressions() -> anyhow::Result<()> {
        let current = [json!({"id": "parse", "estimate_nanoseconds": 120.0})];
        let baseline = [json!({"id": "parse", "estimate_nanoseconds": 100.0})];
        let rows = comparisons(&current, &baseline)?;
        assert_eq!(rows[0]["delta_percent"], 20.0);
        assert_eq!(rows[0]["direction"], "regressed");
        for (current, baseline) in [(&current[..], &[][..]), (&[][..], &baseline[..])] {
            assert_eq!(
                comparisons(current, baseline)?[0]["direction"],
                "incomparable"
            );
        }
        Ok(())
    }
}
