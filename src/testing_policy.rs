//! Declared risk ownership is separate from observed execution and inferred value.
mod disposition;
mod model;
mod ownership;
mod schema;

use anyhow::{Context, Result};
pub(crate) use model::Config;
pub(crate) use schema::{config_schema, report_schema};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs;

use crate::{config::LensConfig, contracts::MeasurementConfidence};
use model::{Behavior, Manifest, TestSelector};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Observation {
    test: TestSelector,
    resolution: String,
    execution: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BehaviorReview {
    declared: Behavior,
    observed: Vec<Observation>,
    status: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Review {
    version: u32,
    status: String,
    manifest: Option<String>,
    execution_evidence: String,
    behaviors: Vec<BehaviorReview>,
    #[serde(default)]
    candidates: Vec<Value>,
    #[serde(default)]
    contextual_evidence: Vec<Value>,
    limitations: Vec<String>,
}

pub(crate) fn review(
    config: &LensConfig,
    fingerprint: &Value,
    tests: &[Value],
    confidence: &mut MeasurementConfidence,
    findings: &mut Vec<Value>,
) -> Result<Value> {
    let mut review = Review {
        version: 1,
        status: "not_configured".into(),
        manifest: None,
        execution_evidence: "not_requested".into(),
        behaviors: Vec::new(),
        candidates: Vec::new(),
        contextual_evidence: Vec::new(),
        limitations: vec![
            "Declarations are reviewed claims, not proof of behavior or boundary coverage.".into(),
            "Aggregate coverage and syntax cannot establish test-specific protection or redundancy.".into(),
            "Only exact discovered Rust tests are supported; external harnesses remain unresolved.".into(),
        ],
    };
    if let Some(settings) = &config.testing_policy {
        review.manifest = Some(settings.manifest.clone());
        match load(config, settings) {
            Ok(manifest) => {
                let execution = ownership::Execution::load(config, fingerprint);
                review.execution_evidence = execution.status.clone();
                review.status = "reviewed".into();
                for behavior in manifest.behaviors {
                    review.behaviors.push(ownership::review_behavior(
                        behavior,
                        tests,
                        &execution,
                        &settings.manifest,
                        findings,
                    ));
                }
                review.candidates = disposition::candidates(
                    config,
                    settings,
                    manifest.dispositions,
                    &review.behaviors,
                    tests,
                    findings,
                );
                review.contextual_evidence = contextual_evidence(config, fingerprint);
            }
            Err(error) => {
                review.status = "unavailable".into();
                let reason = format!("testing policy manifest: {error:#}");
                confidence.mark_partial(&reason);
                review.limitations.push(reason);
            }
        }
    }
    Ok(serde_json::to_value(review)?)
}

fn load(config: &LensConfig, settings: &Config) -> Result<Manifest> {
    let path = config.project_root.join(&settings.manifest);
    let manifest: Manifest = toml::from_str(
        &fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?,
    )?;
    manifest.validate()?;
    Ok(manifest)
}

pub(super) fn finding(manifest: &str, behavior: &Behavior, rule: &str, message: &str) -> Value {
    json!({
        "rule_id": rule, "category": "heuristic", "kind": "testing-policy",
        "severity": "advisory", "path": manifest, "line": 1,
        "source": "rqlens_testing_policy", "behavior_id": behavior.id,
        "message": format!("{}: {message}", behavior.id),
        "review_required": true, "evidence_basis": "declared_ownership_and_bounded_observations",
        "risks": behavior.risks, "requirement": behavior.requirement,
    })
}

fn contextual_evidence(config: &LensConfig, fingerprint: &Value) -> Vec<Value> {
    [
        (
            "clones.json",
            "Text/AST similarity is not equivalent behavioral protection.",
        ),
        (
            "coverage.json",
            "Aggregate execution reach does not identify owners or assertion sensitivity.",
        ),
    ]
    .into_iter()
    .map(|(file, limitation)| {
        let status = match crate::contracts::read_current(
            &config.output_dir.join(file),
            file,
            fingerprint,
        ) {
            Ok(artifact) if artifact.current && artifact.complete => "current",
            Ok(artifact) if !artifact.current => "stale",
            Ok(_) => "partial",
            Err(_) => "unavailable",
        };
        json!({"artifact": file, "status": status, "limitation": limitation})
    })
    .collect()
}

pub(crate) fn validate_report(value: &Value) -> Result<()> {
    // Old artifacts without this optional extension remain valid, not evaluated.
    if let Some(value) = value.get("testing_policy") {
        let report: Review = serde_json::from_value(value.clone())?;
        anyhow::ensure!(report.version == 1, "unsupported testing policy report");
        anyhow::ensure!(
            ["not_configured", "unavailable", "reviewed"].contains(&report.status.as_str()),
            "invalid testing policy status"
        );
        let manifest = Manifest {
            version: 1,
            behaviors: report
                .behaviors
                .into_iter()
                .map(|row| row.declared)
                .collect(),
            dispositions: Vec::new(),
        };
        manifest.validate()?;
    }
    Ok(())
}
