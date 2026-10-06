use super::{
    confidence,
    config::{ResourceKind, ResourceProfile},
};
use crate::{
    command_runner::{CommandOutcome, CommandRequest, CommandStatus, run},
    config::LensConfig,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    time::Duration,
};

pub(super) fn measure(config: &LensConfig, profile: &ResourceProfile) -> Value {
    match execute(config, profile) {
        Ok(row) => row,
        Err(error) => {
            json!({"id": profile.id, "profile": profile, "status": "unavailable", "observations": [],
            "measurement_confidence": confidence("resource_profile", &[error.to_string()])})
        }
    }
}

fn execute(config: &LensConfig, profile: &ResourceProfile) -> Result<Value> {
    let directory = tempfile::tempdir()?;
    let target = directory.path().join("target");
    let mut reasons = Vec::new();
    let profiler = if profile.peak_rss {
        profiler(config)
            .map_err(|error| reasons.push(error.to_string()))
            .ok()
    } else {
        None
    };
    let phases = if profile.kind == ResourceKind::Build {
        vec!["cold-build", "warm-noop-build"]
    } else {
        vec!["workload"; profile.runs]
    };
    let mut observations = Vec::new();
    let mut failed = false;
    for (iteration, phase) in phases.iter().enumerate() {
        let report_path = directory
            .path()
            .join(format!("instrumentation-{iteration}.json"));
        let time_path = directory.path().join(format!("time-{iteration}.txt"));
        let command = command(config, profile);
        let outcome = invoke(
            config,
            &command,
            profiler.as_deref(),
            &time_path,
            &report_path,
            (profile.kind == ResourceKind::Build).then_some(target.as_path()),
        );
        failed |= outcome.status == CommandStatus::Failed;
        let mut metrics = BTreeMap::new();
        metrics.insert(
            "elapsed_ms".to_string(),
            u64::try_from(outcome.duration_ms).unwrap_or(u64::MAX),
        );
        let normal = matches!(
            outcome.status,
            CommandStatus::Passed | CommandStatus::Failed
        ) && outcome.reason.is_none();
        if !normal {
            reasons.push(format!(
                "{phase} iteration {} did not complete normally",
                iteration + 1
            ));
        }
        if profiler.is_some() {
            match fs::read_to_string(&time_path)
                .context("fresh GNU time output missing")
                .and_then(|text| parse_peak_rss(&text))
            {
                Ok(rss) => {
                    metrics.insert("peak_rss_bytes".into(), rss);
                }
                Err(error) => reasons.push(error.to_string()),
            }
        }
        if !profile.instrumented_metrics.is_empty() {
            match fs::read(&report_path)
                .context("fresh instrumentation report missing")
                .and_then(|bytes| Ok(serde_json::from_slice::<Value>(&bytes)?))
                .and_then(|value| instrumented(&value, &profile.instrumented_metrics))
            {
                Ok(values) => metrics.extend(values),
                Err(error) => reasons.push(error.to_string()),
            }
        }
        let artifacts =
            if profile.kind == ResourceKind::Build && outcome.status == CommandStatus::Passed {
                match artifacts(config, &outcome.stdout, &target) {
                    Ok(artifacts) => {
                        let total: u64 = artifacts
                            .iter()
                            .filter_map(|row| row["bytes"].as_u64())
                            .sum();
                        metrics.insert("artifact_bytes".into(), total);
                        artifacts
                    }
                    Err(error) => {
                        reasons.push(error.to_string());
                        Vec::new()
                    }
                }
            } else {
                Vec::new()
            };
        let budgets = budget_results(&metrics, &profile.budgets);
        failed |= budgets.iter().any(|row| row["status"] == "breached");
        if budgets.iter().any(|row| row["status"] == "unknown") {
            reasons.push("a budget has no measurement".into());
        }
        observations.push(
            json!({"phase": phase, "iteration": iteration + 1, "metrics": metrics,
            "command": outcome, "artifacts": artifacts, "budgets": budgets}),
        );
        // A failed cold build cannot provide a comparable warm build.
        if profile.kind == ResourceKind::Build && outcome.status != CommandStatus::Passed {
            reasons.push(
                "successful artifact sizes and the cold/warm build pair are unavailable".into(),
            );
            break;
        }
    }
    let status = if failed {
        "failed"
    } else if !reasons.is_empty() {
        "unavailable"
    } else {
        "passed"
    };
    Ok(
        json!({"id": profile.id, "profile": profile, "status": status, "observations": observations,
        "profiler": profiler, "scope": "configured commands on this runner; no cross-machine normalization",
        "measurement_confidence": confidence("resource_profile", &reasons)}),
    )
}

fn command(config: &LensConfig, profile: &ResourceProfile) -> Vec<String> {
    if profile.kind == ResourceKind::Command {
        return profile.command.clone();
    }
    let mut args = vec!["cargo".into()];
    // Build production targets by default, not benches/tests/examples.
    args.extend(config.verification.cargo_arguments("build", false, false));
    if profile.release {
        args.push("--release".into());
    }
    if let Some(target) = &profile.target {
        args.extend(["--target".into(), target.clone()]);
    }
    args.push("--message-format=json".into());
    args
}

fn profiler(config: &LensConfig) -> Result<String> {
    ensure!(
        std::env::consts::OS == "linux",
        "peak RSS collection currently requires GNU time on Linux"
    );
    let args = vec!["--version".into()];
    let mut request = CommandRequest::new("time", &args, &config.project_root);
    request.timeout = Duration::from_secs(5);
    let outcome = run(request);
    ensure!(
        outcome.status == CommandStatus::Passed
            && format!("{}{}", outcome.stdout, outcome.stderr).contains("GNU"),
        "GNU time executable is unavailable on PATH"
    );
    Ok(format!(
        "{}; maximum resident set of command/children, not simultaneous process-tree sum",
        outcome.stdout.lines().next().unwrap_or("GNU time")
    ))
}

fn invoke(
    config: &LensConfig,
    command: &[String],
    profiler: Option<&str>,
    time_path: &Path,
    report_path: &Path,
    target: Option<&Path>,
) -> CommandOutcome {
    let args = if profiler.is_some() {
        [
            vec![
                "-f".into(),
                "RQLENS_RSS_KIB=%M".into(),
                "-o".into(),
                time_path.to_string_lossy().into_owned(),
                "--".into(),
            ],
            command.to_vec(),
        ]
        .concat()
    } else {
        command[1..].to_vec()
    };
    let program = if profiler.is_some() {
        "time"
    } else {
        &command[0]
    };
    let mut request = CommandRequest::new(program, &args, &config.project_root);
    request.timeout = Duration::from_secs(config.verification.timeout_seconds);
    request.environment.insert(
        "RQLENS_RESOURCE_REPORT".into(),
        report_path.to_string_lossy().into_owned(),
    );
    request.environment.insert("LC_ALL".into(), "C".into());
    if let Some(target) = target {
        request.environment.insert(
            "CARGO_TARGET_DIR".into(),
            target.to_string_lossy().into_owned(),
        );
        request
            .environment
            .insert("CARGO_INCREMENTAL".into(), "1".into());
    }
    run(request)
}

fn parse_peak_rss(text: &str) -> Result<u64> {
    let values: Vec<_> = text
        .lines()
        .filter_map(|line| line.strip_prefix("RQLENS_RSS_KIB="))
        .collect();
    ensure!(values.len() == 1, "missing or ambiguous GNU time peak RSS");
    values[0]
        .parse::<u64>()?
        .checked_mul(1024)
        .context("peak RSS overflow")
}

fn instrumented(value: &Value, requested: &[String]) -> Result<BTreeMap<String, u64>> {
    ensure!(
        value["schema_version"] == 1 && value["metrics"].is_object(),
        "unsupported resource instrumentation schema"
    );
    requested
        .iter()
        .map(|name| {
            Ok((
                name.clone(),
                value["metrics"][name].as_u64().with_context(|| {
                    format!("missing/non-integer/negative instrumented metric {name}")
                })?,
            ))
        })
        .collect()
}

fn budget_results(metrics: &BTreeMap<String, u64>, budgets: &BTreeMap<String, u64>) -> Vec<Value> {
    budgets.iter().map(|(metric, maximum)| {
        let measured = metrics.get(metric);
        json!({"metric": metric, "maximum": maximum, "observed": measured,
            "status": match measured { Some(value) if value > maximum => "breached", Some(_) => "healthy", None => "unknown" }})
    }).collect()
}

fn artifacts(config: &LensConfig, stdout: &str, target: &Path) -> Result<Vec<Value>> {
    let members: BTreeSet<_> = config
        .cargo_metadata
        .as_ref()
        .and_then(|metadata| metadata["workspace_members"].as_array())
        .context("workspace metadata unavailable for artifact attribution")?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let mut paths = BTreeSet::new();
    let mut records = Vec::new();
    let canonical_target = target.canonicalize()?;
    for line in stdout.lines() {
        let message: Value = serde_json::from_str(line).context("invalid Cargo artifact JSON")?;
        if message["reason"] != "compiler-artifact"
            || !message["package_id"]
                .as_str()
                .is_some_and(|id| members.contains(id))
            || message["target"]["kind"]
                .as_array()
                .is_some_and(|kinds| kinds.iter().any(|kind| kind == "custom-build"))
        {
            continue;
        }
        for path in message["filenames"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            let path = Path::new(path);
            if path
                .extension()
                .is_some_and(|ext| matches!(ext.to_str(), Some("rmeta" | "d" | "pdb")))
                || !path.is_file()
            {
                continue;
            }
            let canonical = path.canonicalize()?;
            ensure!(
                canonical.starts_with(&canonical_target),
                "build artifact escaped isolated target directory"
            );
            if !paths.insert(canonical.clone()) {
                continue;
            }
            records.push(json!({"package_id": message["package_id"], "target": message["target"]["name"], "kind": message["target"]["kind"],
                "path": canonical.strip_prefix(&canonical_target)?.to_string_lossy(), "bytes": fs::metadata(&canonical)?.len()}));
        }
    }
    ensure!(
        !records.is_empty(),
        "no current workspace output artifacts were measured"
    );
    records.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rss_instrumentation_and_budgets_are_strict() -> Result<()> {
        assert_eq!(
            parse_peak_rss("Command exited with non-zero status 1\nRQLENS_RSS_KIB=12\n")?,
            12288
        );
        assert!(parse_peak_rss("RQLENS_RSS_KIB=1\nRQLENS_RSS_KIB=2").is_err());
        assert!(parse_peak_rss("RQLENS_RSS_KIB=-1").is_err());
        let names = vec!["allocations".into()];
        assert_eq!(
            instrumented(
                &json!({"schema_version": 1, "metrics": {"allocations": 7}}),
                &names
            )?["allocations"],
            7
        );
        for value in [
            json!({}),
            json!({"schema_version": 1, "metrics": {"allocations": -1}}),
            json!({"schema_version": 1, "metrics": {"allocations": 1.5}}),
        ] {
            assert!(instrumented(&value, &names).is_err());
        }
        let budgets = BTreeMap::from([("allocations".into(), 5), ("peak_rss_bytes".into(), 100)]);
        let results = budget_results(&BTreeMap::from([("allocations".into(), 6)]), &budgets);
        assert_eq!(results[0]["status"], "breached");
        assert_eq!(results[1]["status"], "unknown");
        Ok(())
    }
    #[test]
    fn fresh_reports_cannot_be_reused_between_iterations() -> Result<()> {
        let root = tempfile::tempdir()?;
        let config = LensConfig::test_config("resource", root.path());
        let profile: ResourceProfile = toml::from_str(
            "id='tool'\nkind='command'\ncommand=['cargo','--version']\nruns=2\ninstrumented_metrics=['allocations']",
        )?;
        let row = measure(&config, &profile);
        assert_eq!(row["status"], "unavailable");
        assert_eq!(row["observations"].as_array().map(Vec::len), Some(2));
        assert_eq!(row["measurement_confidence"]["complete"], false);
        assert!(row["observations"][0]["metrics"]["allocations"].is_null());
        Ok(())
    }
}
