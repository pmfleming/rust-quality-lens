//! Explicit imported experiments; no arbitrary commands or mutations are executed.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs};

use super::{
    BehaviorReview,
    model::{TestSelector, relative_path, strings, text},
};
use crate::{config::LensConfig, contracts::MeasurementConfidence};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Evidence {
    version: u32,
    input_fingerprint: Value,
    producer: String,
    producer_version: String,
    records: Vec<Experiment>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Experiment {
    id: String,
    behavior: String,
    owner: TestSelector,
    fault: String,
    fault_path: String,
    expected_failure: String,
    baseline: Run,
    injected: Run,
    restored: Run,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Run {
    command: Vec<String>,
    status: RunStatus,
    output: String,
}

#[derive(Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum RunStatus {
    Passed,
    AssertionFailed,
    CompileFailed,
    TimedOut,
    NotRun,
    Inconclusive,
}

pub(super) fn review(
    config: &LensConfig,
    behaviors: &[BehaviorReview],
    confidence: &mut MeasurementConfidence,
) -> Value {
    let Some(path) = config
        .testing_policy
        .as_ref()
        .and_then(|settings| settings.fault_evidence.as_ref())
    else {
        return json!({"status": "not_configured", "records": [], "subject_fingerprint": null});
    };
    // Exclude the report itself from the subject it attests to. The ordinary
    // artifact fingerprint still includes the report, so later edits stale it.
    let mut subject = config.clone();
    if let Some(settings) = &mut subject.testing_policy {
        settings.fault_evidence = None;
    }
    let fingerprint = subject.input_fingerprint();
    match load(config, path, &fingerprint, behaviors) {
        Ok(evidence) => json!({
            "status": "current_import", "subject_fingerprint": fingerprint,
            "artifact": path, "producer": evidence.producer, "producer_version": evidence.producer_version,
            "authority": "imported_observations_not_independently_verified",
            "records": evidence.records.iter().map(|record| json!({
                "status": classification(record), "observation": record,
            })).collect::<Vec<_>>(),
            "limitation": "Scoped reported experiments, not proof of adequacy, all faults, or independent command execution."
        }),
        Err(error) => {
            let reason = format!("fault evidence: {error:#}");
            confidence.mark_partial(&reason);
            json!({"status": "unavailable", "reason": reason, "subject_fingerprint": fingerprint, "records": []})
        }
    }
}

fn load(
    config: &LensConfig,
    path: &str,
    current: &Value,
    behaviors: &[BehaviorReview],
) -> Result<Evidence> {
    let evidence: Evidence =
        serde_json::from_str(&fs::read_to_string(config.project_root.join(path))?)?;
    ensure!(evidence.version == 1, "unsupported fault evidence version");
    text(&evidence.producer)?;
    text(&evidence.producer_version)?;
    ensure!(
        crate::fingerprint::freshness(
            &json!({"input_fingerprint": evidence.input_fingerprint}),
            current
        ) == crate::fingerprint::Freshness::Current,
        "fault evidence subject is stale or incomplete"
    );
    let mut ids = BTreeSet::new();
    for record in &evidence.records {
        ensure!(ids.insert(&record.id), "duplicate fault experiment");
        validate_experiment(record, behaviors)?;
    }
    Ok(evidence)
}

fn validate_experiment(record: &Experiment, behaviors: &[BehaviorReview]) -> Result<()> {
    for value in [
        &record.id,
        &record.behavior,
        &record.fault,
        &record.expected_failure,
    ] {
        text(value)?;
    }
    relative_path(&record.fault_path)?;
    record.owner.validate()?;
    let behavior = behaviors
        .iter()
        .find(|row| row.declared.id == record.behavior)
        .context("unknown fault behavior")?;
    ensure!(
        behavior.declared.owner.tests.contains(&record.owner),
        "fault owner is not a declared primary owner"
    );
    for run in [&record.baseline, &record.injected, &record.restored] {
        strings(&run.command)?;
        text(&run.output)?;
    }
    Ok(())
}

pub(super) fn advice(path: &str, behaviors: &[BehaviorReview], evidence: &Value) -> Vec<Value> {
    behaviors.iter().filter(|row| row.declared.required).filter(|row| {
        !evidence["records"].as_array().into_iter().flatten().any(|record| {
            record["status"] == "reported_detected" && record["observation"]["behavior"] == row.declared.id
        })
    }).map(|row| super::finding(path, &row.declared, "rust.testing.sensitivity-review",
        "Where practical, demonstrate a representative fault in an isolated fixture: passing baseline, intended behavioral failure, passing restored run. No mutation-score target or automatic execution."))
    .collect()
}

pub(super) fn validate_output(evidence: &Value, behaviors: &[BehaviorReview]) -> Result<()> {
    if evidence.is_null() {
        return Ok(());
    } // Older optional extension.
    let records = evidence["records"]
        .as_array()
        .context("missing sensitivity records")?;
    match evidence["status"].as_str() {
        Some("not_configured" | "unavailable") => ensure!(
            records.is_empty(),
            "unavailable sensitivity contains observations"
        ),
        Some("current_import") => {
            ensure!(
                evidence["authority"] == "imported_observations_not_independently_verified",
                "invalid sensitivity authority"
            );
            for row in records {
                let record: Experiment = serde_json::from_value(row["observation"].clone())?;
                validate_experiment(&record, behaviors)?;
                ensure!(
                    row["status"] == classification(&record),
                    "sensitivity status disagrees with experiment"
                );
            }
        }
        _ => anyhow::bail!("invalid sensitivity status"),
    }
    Ok(())
}

fn classification(record: &Experiment) -> &'static str {
    if record.injected.status == RunStatus::NotRun {
        return "not_run";
    }
    if record.baseline.status != RunStatus::Passed || record.restored.status != RunStatus::Passed {
        return "inconclusive";
    }
    match record.injected.status {
        RunStatus::AssertionFailed => "reported_detected",
        RunStatus::Passed => "survived",
        _ => "inconclusive",
    }
}
