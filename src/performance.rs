use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use walkdir::WalkDir;

use crate::command_runner::{CommandRequest, CommandStatus, run};
use crate::config::LensConfig;
use crate::util::{normalize_slashes, project_input_fingerprint, round2, write_json};

pub(crate) fn run_benchmarks(
    config: &LensConfig,
    baseline: Option<PathBuf>,
    no_run: bool,
) -> Result<PathBuf> {
    let command = if no_run {
        None
    } else {
        let arguments = config.verification.cargo_arguments("bench", false, false);
        let mut request = CommandRequest::new("cargo", &arguments, &config.project_root);
        request.timeout = Duration::from_secs(config.verification.timeout_seconds);
        Some(run(request))
    };
    let records = criterion_records(&config.project_root.join("target/criterion"));
    let baseline_records = baseline
        .as_deref()
        .map(read_baseline)
        .transpose()?
        .unwrap_or_default();
    let comparisons = comparisons(&records, &baseline_records);
    let command_passed = command
        .as_ref()
        .is_none_or(|outcome| outcome.status == CommandStatus::Passed);
    let confidence = json!({
        "complete": !no_run && command_passed && !records.is_empty(),
        "partial": no_run || !command_passed || records.is_empty(),
        "confidence_scope": "criterion_benchmark_execution",
        "required_inputs": ["cargo_bench", "criterion_estimates"],
        "observed_inputs": {"benchmark_count": records.len(), "command_executed": !no_run},
        "missing_input": if records.is_empty() { vec!["no Criterion estimates were found"] } else { Vec::<&str>::new() },
        "stale_input": if no_run { vec!["benchmark execution time is unknown because --no-run reused existing estimates"] } else { Vec::<&str>::new() },
        "unsupported_pattern": if command_passed { Vec::<String>::new() } else { vec!["cargo bench did not complete successfully".to_string()] },
    });
    let document = json!({
        "schema_version": 1,
        "generated_from": "rqlens",
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "generator_version": env!("CARGO_PKG_VERSION"),
        "input_fingerprint": project_input_fingerprint(&config.project_root, &config.source_roots),
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
        "estimate_nanoseconds": round2(estimate),
        "source": normalize_slashes(path),
        "engine": "criterion",
    }))
}

fn read_baseline(path: &Path) -> Result<Vec<Value>> {
    let path = if path.is_dir() {
        path.join("performance.json")
    } else {
        path.to_path_buf()
    };
    let document: Value = serde_json::from_str(
        &fs::read_to_string(&path)
            .with_context(|| format!("reading performance baseline {}", path.display()))?,
    )?;
    Ok(document["data"]["records"]
        .as_array()
        .cloned()
        .unwrap_or_default())
}

fn comparisons(current: &[Value], baseline: &[Value]) -> Vec<Value> {
    let old = baseline
        .iter()
        .filter_map(|row| {
            Some((
                row["id"].as_str()?.to_string(),
                row["estimate_nanoseconds"].as_f64()?,
            ))
        })
        .collect::<BTreeMap<_, _>>();
    current
        .iter()
        .filter_map(|row| {
            let id = row["id"].as_str()?;
            let current = row["estimate_nanoseconds"].as_f64()?;
            let baseline = *old.get(id)?;
            let delta_percent = if baseline == 0.0 { 0.0 } else { (current - baseline) / baseline * 100.0 };
            Some(json!({
                "id": id,
                "baseline_nanoseconds": baseline,
                "current_nanoseconds": current,
                "delta_percent": round2(delta_percent),
                "direction": if delta_percent > 0.0 { "regressed" } else if delta_percent < 0.0 { "improved" } else { "unchanged" },
            }))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::comparisons;
    use serde_json::json;

    #[test]
    fn benchmark_comparisons_report_relative_regressions() {
        let current = [json!({"id": "parse", "estimate_nanoseconds": 120.0})];
        let baseline = [json!({"id": "parse", "estimate_nanoseconds": 100.0})];
        let rows = comparisons(&current, &baseline);
        assert_eq!(rows[0]["delta_percent"], 20.0);
        assert_eq!(rows[0]["direction"], "regressed");
    }
}
