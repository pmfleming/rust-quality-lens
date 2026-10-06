use super::{
    confidence,
    config::{ResourceKind, ResourceProfile},
};
use crate::{
    command_runner::{
        CommandOutcome, CommandRequest, CommandStatus, cargo_artifacts, observed_result, run,
    },
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
    let profiler = profile
        .peak_rss
        .then(|| observed_result(profiler(config), &mut reasons))
        .flatten();
    let phases = match profile.kind {
        ResourceKind::Build => vec!["cold-build", "warm-noop-build"],
        ResourceKind::Command => vec!["workload"; profile.runs],
    };
    let mut observations = Vec::new();
    for (iteration, phase) in phases.iter().enumerate() {
        let report = directory
            .path()
            .join(format!("instrumentation-{iteration}.json"));
        let timing = directory.path().join(format!("time-{iteration}.txt"));
        let outcome = invoke(
            config,
            &command(config, profile),
            profiler.as_deref(),
            &timing,
            &report,
            (profile.kind == ResourceKind::Build).then_some(target.as_path()),
        );
        let mut metrics = BTreeMap::new();
        if outcome.status == CommandStatus::Passed && outcome.completed() {
            metrics.insert("elapsed_ms".into(), u64::try_from(outcome.duration_ms)?);
            if profiler.is_some() {
                metrics.extend(
                    observed_result(
                        fs::read_to_string(&timing)
                            .context("fresh GNU time output missing")
                            .and_then(|text| parse_peak_rss(&text)),
                        &mut reasons,
                    )
                    .map(|rss| ("peak_rss_bytes".into(), rss)),
                );
            }
            metrics.extend(
                observed_result(read_instrumentation(&report, profile), &mut reasons)
                    .into_iter()
                    .flatten(),
            );
        } else {
            reasons.push(format!(
                "{phase} iteration {} did not complete successfully",
                iteration + 1
            ));
        }
        let artifacts = build_artifacts(
            config,
            profile,
            &outcome,
            &target,
            &mut metrics,
            &mut reasons,
        );
        let budgets = budget_results(&metrics, &profile.budgets);
        if budgets.iter().any(|row| row["status"] == "unknown") {
            reasons.push("a budget has no measurement".into());
        }
        observations.push(
            json!({"phase": phase, "iteration": iteration + 1, "metrics": metrics,
            "command": outcome, "artifacts": artifacts, "budgets": budgets}),
        );
        if profile.kind == ResourceKind::Build && outcome.status != CommandStatus::Passed {
            break;
        }
    }
    let failed = observations.iter().any(|row| {
        row["command"]["status"] == "failed"
            || row["budgets"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|budget| budget["status"] == "breached")
    });
    let status = match (failed, reasons.is_empty()) {
        (true, _) => "failed",
        (_, false) => "unavailable",
        _ => "passed",
    };
    Ok(
        json!({"id": profile.id, "profile": profile, "status": status, "observations": observations,
        "profiler": profiler, "scope": "configured commands on this runner; no cross-machine normalization",
        "measurement_confidence": confidence("resource_profile", &reasons)}),
    )
}

fn read_instrumentation(path: &Path, profile: &ResourceProfile) -> Result<BTreeMap<String, u64>> {
    if profile.instrumented_metrics.is_empty() {
        return Ok(BTreeMap::new());
    }
    let value =
        serde_json::from_slice(&fs::read(path).context("fresh instrumentation report missing")?)?;
    instrumented(&value, &profile.instrumented_metrics)
}

fn build_artifacts(
    config: &LensConfig,
    profile: &ResourceProfile,
    outcome: &CommandOutcome,
    target: &Path,
    metrics: &mut BTreeMap<String, u64>,
    reasons: &mut Vec<String>,
) -> Vec<Value> {
    if profile.kind != ResourceKind::Build
        || outcome.status != CommandStatus::Passed
        || !outcome.completed()
    {
        return Vec::new();
    }
    let records = observed_result(artifacts(config, &outcome.stdout, target), reasons);
    if let Some(records) = &records {
        metrics.insert(
            "artifact_bytes".into(),
            records.iter().filter_map(|row| row["bytes"].as_u64()).sum(),
        );
    }
    records.unwrap_or_default()
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
    for artifact in cargo_artifacts(stdout)?.into_iter().filter(|artifact| {
        members.contains(artifact.package_id.as_str())
            && !artifact
                .target
                .kind
                .iter()
                .any(|kind| kind == "custom-build")
    }) {
        for path in &artifact.filenames {
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
            records.push(json!({"package_id": artifact.package_id, "target": artifact.target.name, "kind": artifact.target.kind,
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
    use super::{budget_results, instrumented, measure, parse_peak_rss};
    use crate::config::LensConfig;
    use crate::profiles::config::ResourceProfile;
    use anyhow::Result;
    use serde_json::json;
    use std::collections::BTreeMap;
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
        let mut profile = profile;
        profile.command = vec!["cargo".into(), "rqlens-missing-subcommand".into()];
        profile.instrumented_metrics.clear();
        profile.budgets.insert("elapsed_ms".into(), u64::MAX);
        let failed = measure(&config, &profile);
        assert_eq!(failed["observations"][0]["budgets"][0]["status"], "unknown");
        assert!(failed["observations"][0]["metrics"]["elapsed_ms"].is_null());
        Ok(())
    }
}
