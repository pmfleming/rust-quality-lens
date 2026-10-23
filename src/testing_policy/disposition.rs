//! Reviewed candidates, never automatic deletion or a numeric quality score.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;

use super::{
    BehaviorReview, finding,
    model::{Config, TestSelector, relative_path, strings, text},
};
use crate::config::LensConfig;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Action {
    Consolidate,
    LowValue,
    ReduceMatrix,
    RetireCompatibility,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SupportDecision {
    withdrawn: bool,
    document: String,
    effective: String,
    approved_by: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Disposition {
    pub(crate) id: String,
    pub(crate) behavior: String,
    action: Action,
    tests: Vec<TestSelector>,
    reason: String,
    reviewer: String,
    review_revision: String,
    preserved_risks: Vec<String>,
    estimated_effort_hours: f64,
    #[serde(default)]
    support_decision: Option<SupportDecision>,
}

impl Disposition {
    pub(super) fn validate(&self) -> Result<()> {
        for value in [
            &self.id,
            &self.behavior,
            &self.reason,
            &self.reviewer,
            &self.review_revision,
        ] {
            text(value)?;
        }
        strings(&self.preserved_risks)?;
        ensure!(
            self.estimated_effort_hours.is_finite()
                && self.estimated_effort_hours > 0.0
                && (self.tests.len() as f64 / self.estimated_effort_hours).is_finite(),
            "estimated effort and leverage must be positive and finite"
        );
        ensure!(
            !self.tests.is_empty(),
            "disposition must name candidate tests"
        );
        let mut tests = BTreeSet::new();
        for test in &self.tests {
            test.validate()?;
            ensure!(tests.insert(test), "duplicate disposition selector");
        }
        if let Some(decision) = &self.support_decision {
            relative_path(&decision.document)?;
            for value in [&decision.effective, &decision.approved_by] {
                text(value)?;
            }
        }
        Ok(())
    }
}

pub(super) fn candidates(
    config: &LensConfig,
    settings: &Config,
    dispositions: Vec<Disposition>,
    behaviors: &[BehaviorReview],
    tests: &[Value],
    findings: &mut Vec<Value>,
) -> Vec<Value> {
    let mut candidates = Vec::new();
    for disposition in dispositions {
        let Some(behavior) = behaviors
            .iter()
            .find(|row| row.declared.id == disposition.behavior)
        else {
            continue;
        };
        let status = candidate_status(config, settings, &disposition, behaviors, behavior, tests);
        let mut row = finding(
            &settings.manifest,
            &behavior.declared,
            "rust.testing.consolidation-review",
            "Review candidate value, distinct risks and prerequisites; never delete solely to improve counts or coverage metrics.",
        );
        row["disposition"] = json!("review");
        row["candidate_id"] = json!(disposition.id);
        row["candidate_status"] = json!(status);
        row["declared"] = json!(disposition);
        row["replacement_owner"] = if matches!(
            disposition.action,
            Action::LowValue | Action::RetireCompatibility
        ) {
            Value::Null
        } else {
            json!(behavior.declared.owner)
        };
        row["estimated_removable_registrations"] = json!(disposition.tests.len());
        row["estimated_registrations_per_hour"] =
            json!(disposition.tests.len() as f64 / disposition.estimated_effort_hours);
        row["prerequisites"] = json!([
            "Resolve critical protection gaps before pursuing numerical leverage.",
            "Review distinct important risks and materially important matrix combinations.",
            "Run retained owners and, where practical, demonstrate representative defect sensitivity.",
            "Review coverage deltas without silently suppressing existing gates; production removal requires separate authorization."
        ]);
        findings.push(row.clone());
        candidates.push(row);
    }
    // Estimates only, within the consolidation queue. Protection gaps precede it.
    candidates.sort_by(|a, b| {
        let leverage = |row: &Value| {
            row["estimated_registrations_per_hour"]
                .as_f64()
                .unwrap_or(0.0)
        };
        leverage(b)
            .total_cmp(&leverage(a))
            .then_with(|| a["candidate_id"].as_str().cmp(&b["candidate_id"].as_str()))
    });
    candidates
}

fn candidate_status(
    config: &LensConfig,
    settings: &Config,
    disposition: &Disposition,
    behaviors: &[BehaviorReview],
    behavior: &BehaviorReview,
    tests: &[Value],
) -> &'static str {
    if disposition
        .tests
        .iter()
        .any(|selector| super::ownership::resolve(selector, tests) != "resolved")
    {
        return "unresolved_candidates";
    }
    if protects_retained_risk(disposition, behaviors) {
        return "distinct_risk_review_required";
    }
    if disposition.action == Action::RetireCompatibility {
        return if disposition
            .support_decision
            .as_ref()
            .is_some_and(|decision| {
                decision.withdrawn
                    && settings.evidence_inputs.contains(&decision.document)
                    && config.project_root.join(&decision.document).is_file()
            }) {
            "retirement_review"
        } else {
            "support_review_required"
        };
    }
    if disposition.action != Action::LowValue && behavior.status != "observed_pass" {
        return "replacement_evidence_required";
    }
    "value_review_candidate"
}

fn protects_retained_risk(disposition: &Disposition, behaviors: &[BehaviorReview]) -> bool {
    behaviors
        .iter()
        .filter(|row| {
            disposition.action != Action::RetireCompatibility
                || row.declared.id != disposition.behavior
        })
        .any(|row| {
            disposition.tests.iter().any(|test| {
                row.declared.owner.tests.contains(test)
                    || row.declared.supporting.iter().any(|support| {
                        support.test == *test && !support.distinct_risk.trim().is_empty()
                    })
            })
        })
}
