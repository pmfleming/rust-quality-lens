use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use walkdir::WalkDir;

use crate::command_runner::{CommandOutcome, CommandRequest, CommandStatus, run};
use crate::config::LensConfig;
use crate::contracts::{BenchmarkRecord, benchmark_index, benchmark_records};
use crate::util::{normalize_slashes, round2, write_json};

pub(crate) fn run_benchmarks(
    config: &LensConfig,
    baseline: Option<PathBuf>,
    no_run: bool,
) -> Result<PathBuf> {
    // Validate the requested baseline before starting an expensive command.
    let baseline_records = baseline.as_deref().map(read_baseline).transpose()?;
    let target = cargo_target_directory(config)?;
    let snapshot = crate::measurement::InputSnapshot::capture(config);
    let (criterion_root, command) = execute_benchmarks(config, &target, no_run)?;
    let (mut records, read_errors) = criterion_records(&criterion_root);
    let environment = json!({"os": std::env::consts::OS, "arch": std::env::consts::ARCH,
        "host": hostname::get().ok().map(|host| host.to_string_lossy().into_owned()),
        "toolchain": crate::toolchain::as_json(&snapshot.toolchain)});
    for record in &mut records {
        record
            .details
            .insert("environment".into(), environment.clone());
    }
    let comparisons = baseline_records
        .as_deref()
        .map(|baseline| comparisons(&records, baseline))
        .transpose()?
        .unwrap_or_default();
    let confidence = benchmark_confidence(
        no_run,
        command.as_ref(),
        records.len(),
        &comparisons,
        read_errors,
    );
    let document = json!({
        "schema_version": 1,
        "generated_from": "rqlens",
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "generator_version": env!("CARGO_PKG_VERSION"),
        "input_fingerprint": snapshot.fingerprint,
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
    let output = config.output_dir.join("performance.json");
    snapshot.ensure_unchanged(config)?;
    write_json(&output, &document)?;
    Ok(output)
}

fn benchmark_confidence(
    no_run: bool,
    command: Option<&CommandOutcome>,
    count: usize,
    comparisons: &[Value],
    mut errors: Vec<String>,
) -> Value {
    let command_passed = command.is_some_and(|outcome| outcome.status == CommandStatus::Passed);
    if !no_run && !command_passed {
        errors.push("cargo bench did not complete successfully".into());
    }
    let mut missing = comparisons
        .iter()
        .filter(|row| row["direction"] == "incomparable")
        .map(|row| {
            row["reason"]
                .as_str()
                .unwrap_or("comparison evidence is incompatible")
        })
        .collect::<Vec<_>>();
    if count == 0 {
        missing.push("no Criterion estimates were found");
    }
    let complete = !no_run && command_passed && missing.is_empty() && errors.is_empty();
    json!({
        "complete": complete, "partial": !complete, "confidence_scope": "criterion_benchmark_execution",
        "required_inputs": ["cargo_bench", "criterion_estimates"],
        "observed_inputs": {"benchmark_count": count, "command_executed": !no_run},
        "missing_input": missing,
        "stale_input": if no_run { vec!["benchmark execution time is unknown because --no-run reused existing estimates"] } else { Vec::<&str>::new() },
        "unsupported_pattern": errors,
    })
}

fn execute_benchmarks(
    config: &LensConfig,
    target: &Path,
    no_run: bool,
) -> Result<(PathBuf, Option<CommandOutcome>)> {
    if no_run {
        return Ok((target.join("criterion"), None));
    }
    // Criterion honors CRITERION_HOME. A fresh directory prevents removed or
    // skipped benchmarks from being certified by an unrelated successful run.
    let runs = target.join("rqlens-bench-runs");
    fs::create_dir_all(&runs)?;
    let criterion_root = tempfile::tempdir_in(runs)?.keep();
    let arguments = config.verification.cargo_arguments("bench", false, false);
    let mut request = CommandRequest::new("cargo", &arguments, &config.project_root);
    request.timeout = Duration::from_secs(config.verification.timeout_seconds);
    request.environment.insert(
        "CRITERION_HOME".to_string(),
        criterion_root.to_string_lossy().into_owned(),
    );
    Ok((criterion_root, Some(run(request))))
}

fn cargo_target_directory(config: &LensConfig) -> Result<PathBuf> {
    config
        .cargo_metadata
        .as_ref()
        .and_then(|metadata| metadata["target_directory"].as_str())
        .map(PathBuf::from)
        .context("Cargo metadata is missing target_directory")
}

fn criterion_records(root: &Path) -> (Vec<BenchmarkRecord>, Vec<String>) {
    let mut rows = Vec::new();
    let mut errors = Vec::new();
    for entry in WalkDir::new(root) {
        match entry {
            Ok(entry) if normalize_slashes(entry.path()).ends_with("/new/estimates.json") => {
                match criterion_record(root, entry.path()) {
                    Ok(record) => rows.push(record),
                    Err(error) => errors.push(format!("{}: {error:#}", entry.path().display())),
                }
            }
            Ok(_) => {}
            Err(error) => errors.push(error.to_string()),
        }
    }
    rows.sort_by(|left, right| left.id.cmp(&right.id));
    (rows, errors)
}

fn criterion_record(root: &Path, path: &Path) -> Result<BenchmarkRecord> {
    let estimates: Value = serde_json::from_str(&fs::read_to_string(path)?)?;
    let estimate_data = if estimates["slope"]["point_estimate"].is_number() {
        &estimates["slope"]
    } else {
        &estimates["mean"]
    };
    let estimate = estimate_data["point_estimate"]
        .as_f64()
        .context("missing Criterion point estimate")?;
    let sample = path
        .parent()
        .map(|parent| parent.join("sample.json"))
        .and_then(|path| fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
    let samples = sample
        .as_ref()
        .and_then(|sample| sample["times"].as_array())
        .map(Vec::len);
    let benchmark_dir = path
        .parent()
        .and_then(Path::parent)
        .context("invalid Criterion path")?;
    let record = BenchmarkRecord {
        id: normalize_slashes(benchmark_dir.strip_prefix(root)?),
        estimate_nanoseconds: estimate,
        details: serde_json::Map::from_iter([
            ("source".to_string(), json!(normalize_slashes(path))),
            ("engine".to_string(), json!("criterion")),
            (
                "confidence_interval".to_string(),
                estimate_data["confidence_interval"].clone(),
            ),
            ("samples".to_string(), json!(samples)),
        ]),
    };
    benchmark_index(std::slice::from_ref(&record))?;
    Ok(record)
}

fn read_baseline(path: &Path) -> Result<Vec<BenchmarkRecord>> {
    let document = crate::contracts::read_baseline(path, "performance.json")?;
    benchmark_records(&document["data"]["records"])
}

pub(crate) fn comparisons(
    current: &[BenchmarkRecord],
    baseline: &[BenchmarkRecord],
) -> Result<Vec<Value>> {
    let current_details = current
        .iter()
        .map(|record| (&record.id, record))
        .collect::<std::collections::BTreeMap<_, _>>();
    let baseline_details = baseline
        .iter()
        .map(|record| (&record.id, record))
        .collect::<std::collections::BTreeMap<_, _>>();
    let current = benchmark_index(current)?;
    let old = benchmark_index(baseline)?;
    current.keys().chain(old.keys()).collect::<BTreeSet<_>>().into_iter().map(|id| {
        let (Some(current), Some(baseline)) = (current.get(id), old.get(id)) else {
            return Ok(json!({"id": id, "direction": "incomparable", "reason": if current.contains_key(id) { "benchmark is missing from baseline" } else { "benchmark is missing from current run" }}));
        };
        let delta_percent = (current - baseline) / baseline * 100.0;
        anyhow::ensure!(round2(delta_percent).is_finite(), "benchmark {id} percentage change overflows");
        let current_record = current_details[id];
        let baseline_record = baseline_details[id];
        let current_interval = interval(current_record);
        let baseline_interval = interval(baseline_record);
        let environment = current_record.details.get("environment");
        let matched = environment.is_some_and(|value| value.as_object().is_some_and(|object| !object.is_empty()))
            && environment == baseline_record.details.get("environment");
        let direction = match (matched, current_interval, baseline_interval) {
            (true, Some((low, high, level)), Some((base_low, base_high, base_level))) if level == base_level => {
                if low > base_high { "regressed" } else if high < base_low { "improved" } else { "within_noise" }
            }
            _ => "incomparable",
        };
        Ok(json!({
            "id": id,
            "baseline_nanoseconds": baseline,
            "current_nanoseconds": current,
            "delta_percent": round2(delta_percent),
            "direction": direction,
            "reason": if direction == "incomparable" { Some("Matching environment, sample counts and confidence intervals are required") } else { None },
            "comparison_method": "nonoverlapping-confidence-intervals-v1",
        }))
    }).collect()
}

#[derive(serde::Deserialize)]
struct ConfidenceInterval {
    lower_bound: f64,
    upper_bound: f64,
    confidence_level: f64,
}

fn interval(record: &BenchmarkRecord) -> Option<(f64, f64, f64)> {
    if record.details.get("samples")?.as_u64()? < 3 {
        return None;
    }
    let ConfidenceInterval {
        lower_bound: low,
        upper_bound: high,
        confidence_level: level,
    } = ConfidenceInterval::deserialize(record.details.get("confidence_interval")?).ok()?;
    (low.is_finite()
        && high.is_finite()
        && low > 0.0
        && (low..=high).contains(&record.estimate_nanoseconds)
        && (0.0..1.0).contains(&level)
        && level > 0.0)
        .then_some((low, high, level))
}

#[cfg(test)]
mod tests {
    use super::{benchmark_confidence, comparisons, run_benchmarks};
    use serde_json::json;

    fn comparison_confidence_keeps_the_actual_incompatibility_reason() {
        let confidence = benchmark_confidence(
            false,
            None,
            1,
            &[json!({"direction":"incomparable","reason":"environment differs"})],
            Vec::new(),
        );
        assert_eq!(confidence["complete"], false);
        assert_eq!(confidence["missing_input"], json!(["environment differs"]));
        assert_eq!(
            benchmark_confidence(true, None, 1, &[], Vec::new())["complete"],
            false
        );
        assert_eq!(
            benchmark_confidence(false, None, 0, &[], Vec::new())["complete"],
            false
        );
    }

    fn benchmark_project(
        manifest: &str,
        extra_files: &[(&str, &str)],
    ) -> anyhow::Result<(tempfile::TempDir, crate::config::LensConfig)> {
        let root = tempfile::tempdir()?;
        for (name, contents) in [
            ("src/lib.rs", "pub fn value() {}"),
            ("Cargo.toml", manifest),
            ("rqlens.toml", "source_roots = [\"src\"]\n"),
        ]
        .into_iter()
        .chain(extra_files.iter().copied())
        {
            let path = root.path().join(name);
            std::fs::create_dir_all(
                path.parent()
                    .ok_or_else(|| anyhow::anyhow!("missing fixture parent"))?,
            )?;
            std::fs::write(path, contents)?;
        }
        let config = crate::config::LensConfig::load(Some(root.path().join("rqlens.toml")))?;
        Ok((root, config))
    }

    #[test]
    fn benchmark_runs_ignore_old_estimates_and_honor_cargo_target_directory() -> anyhow::Result<()>
    {
        let (root, config) = benchmark_project(
            "[package]\nname = \"bench-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            &[(
                ".cargo/config.toml",
                "[build]\ntarget-dir = \"custom-target\"\n",
            )],
        )?;
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
    fn criterion_parse_failures_are_retained_without_dropping_valid_records() -> anyhow::Result<()>
    {
        let root = tempfile::tempdir()?;
        for (name, text) in [
            ("good", r#"{"mean":{"point_estimate":0.001}}"#),
            ("bad", "{broken"),
            ("zero", r#"{"mean":{"point_estimate":0}}"#),
        ] {
            let directory = root.path().join(name).join("new");
            std::fs::create_dir_all(&directory)?;
            std::fs::write(directory.join("estimates.json"), text)?;
        }
        let (records, errors) = super::criterion_records(root.path());
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].estimate_nanoseconds, 0.001);
        assert_eq!(errors.len(), 2);
        Ok(())
    }

    #[test]
    fn benchmark_execution_writes_into_its_isolated_directory() -> anyhow::Result<()> {
        let (_root, config) = benchmark_project(
            "[package]\nname = \"criterion-protocol-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[[bench]]\nname = \"probe\"\npath = \"probe.rs\"\nharness = false\n",
            &[(
                "probe.rs",
                r#"
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let home = std::path::PathBuf::from(std::env::var_os("CRITERION_HOME").ok_or("missing CRITERION_HOME")?);
    let directory = home.join("probe/new");
    std::fs::create_dir_all(&directory)?;
    std::fs::write(directory.join("estimates.json"), "{\"mean\":{\"point_estimate\":42.0}}")?;
    Ok(())
}
"#,
            )],
        )?;
        let output = run_benchmarks(&config, None, false)?;
        let document: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(output)?)?;
        assert_eq!(document["measurement_confidence"]["complete"], true);
        assert_eq!(document["data"]["records"][0]["id"], "probe");
        let source = document["data"]["records"][0]["source"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing estimate source"))?;
        assert!(std::path::Path::new(source).is_file());
        assert!(source.contains("rqlens-bench-runs"));
        Ok(())
    }

    #[test]
    fn benchmark_comparison_requires_matching_environment_and_nonoverlapping_intervals()
    -> anyhow::Result<()> {
        comparison_confidence_keeps_the_actual_incompatibility_reason();
        benchmark_comparisons_report_relative_regressions()?;
        let record = |estimate, low, high, host| {
            json!([{ "id": "parse", "estimate_nanoseconds": estimate,
            "samples": 10, "environment": {"host": host},
            "confidence_interval": {"lower_bound": low, "upper_bound": high, "confidence_level": 0.95}}])
        };
        let base = super::benchmark_records(&record(100.0, 95.0, 105.0, "same"))?;
        for (estimate, low, high, host, expected) in [
            (120.0, 115.0, 125.0, "same", "regressed"),
            (102.0, 97.0, 107.0, "same", "within_noise"),
            (120.0, 115.0, 125.0, "other", "incomparable"),
            (80.0, 75.0, 85.0, "same", "improved"),
            (100.0, 100.0, 100.0, "same", "within_noise"),
            (100.0, 0.0, 105.0, "same", "incomparable"),
            (100.0, 110.0, 90.0, "same", "incomparable"),
        ] {
            let current = super::benchmark_records(&record(estimate, low, high, host))?;
            assert_eq!(comparisons(&current, &base)?[0]["direction"], expected);
        }
        for level in [json!(0), json!(1), json!(null), json!("0.95")] {
            let mut value = record(100.0, 95.0, 105.0, "same");
            value[0]["confidence_interval"]["confidence_level"] = level;
            let current = super::benchmark_records(&value)?;
            assert!(super::interval(&current[0]).is_none());
        }
        Ok(())
    }

    fn benchmark_comparisons_report_relative_regressions() -> anyhow::Result<()> {
        let current =
            super::benchmark_records(&json!([{"id": "parse", "estimate_nanoseconds": 120.0}]))?;
        let baseline =
            super::benchmark_records(&json!([{"id": "parse", "estimate_nanoseconds": 100.0}]))?;
        let rows = comparisons(&current, &baseline)?;
        assert_eq!(rows[0]["delta_percent"], 20.0);
        assert_eq!(
            rows[0]["direction"], "incomparable",
            "point estimates alone do not establish a regression"
        );
        for (current, baseline) in [(&current[..], &[][..]), (&[][..], &baseline[..])] {
            assert_eq!(
                comparisons(current, baseline)?[0]["direction"],
                "incomparable"
            );
        }
        Ok(())
    }
}
