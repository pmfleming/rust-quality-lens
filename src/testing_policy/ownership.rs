use serde_json::Value;

use super::{
    BehaviorReview, Observation, finding,
    model::{Behavior, TestSelector},
};
use crate::{config::LensConfig, contracts::read_current};

pub(super) struct Execution {
    pub(super) status: String,
    tests: Vec<Value>,
}

impl Execution {
    pub(super) fn load(config: &LensConfig, fingerprint: &Value) -> Self {
        let artifact = read_current(
            &config.output_dir.join("correctness_review.json"),
            "correctness_review.json",
            fingerprint,
        );
        match artifact {
            Ok(artifact) if artifact.current && artifact.complete => {
                let data = &artifact.document["data"];
                Self {
                    status: data["summary"]["last_run"]["status"]
                        .as_str()
                        .unwrap_or("not_run")
                        .into(),
                    tests: data["tests"].as_array().cloned().unwrap_or_default(),
                }
            }
            Ok(artifact) => Self {
                status: if artifact.current { "partial" } else { "stale" }.into(),
                tests: vec![],
            },
            Err(_) => Self {
                status: "unavailable".into(),
                tests: vec![],
            },
        }
    }

    fn status_for(&self, selector: &TestSelector) -> String {
        // The legacy runner may fall back to a qualified name. Do not promote
        // duplicate names across packages/targets into owner-specific evidence.
        let names = self
            .tests
            .iter()
            .filter(|test| test["qualified_name"] == selector.name)
            .count();
        if names != 1 {
            return "unknown".into();
        }
        self.tests
            .iter()
            .find(|test| {
                test["defining_package"] == selector.package
                    && test["defining_target"] == selector.target
                    && test["path"] == selector.path
                    && test["qualified_name"] == selector.name
            })
            .and_then(|test| test["last_status"].as_str())
            .unwrap_or("unknown")
            .into()
    }
}

pub(super) fn review_behavior(
    behavior: Behavior,
    tests: &[Value],
    execution: &Execution,
    path: &str,
    findings: &mut Vec<Value>,
) -> BehaviorReview {
    let observed = behavior
        .owner
        .tests
        .iter()
        .map(|selector| {
            let resolution = resolve(selector, tests);
            Observation {
                test: selector.clone(),
                resolution: resolution.into(),
                execution: if resolution == "resolved" {
                    execution.status_for(selector)
                } else {
                    "unknown".into()
                },
            }
        })
        .collect::<Vec<_>>();
    let status = owner_status(&observed, &execution.status);
    if status != "observed_pass" {
        findings.push(finding(path, &behavior, "rust.testing.owner-review", match status {
            "unresolved" => "Resolve a primary owner at the highest practical level; missing discovery is not proof that no protection exists.",
            "observed_failure" => "Repair the retained owner; do not add duplicate tests by default.",
            _ => "Run the retained owner in its required scope; execution is unknown, skipped, stale or incomplete.",
        }));
    }
    if !behavior.substituted_boundaries.is_empty() {
        findings.push(finding(path, &behavior, "rust.testing.boundary-review",
            "Limit the claim to real exercised implementations; doubles do not establish coverage of replaced boundaries."));
    }
    for support in &behavior.supporting {
        if support.distinct_risk.trim().is_empty() {
            let mut row = finding(
                path,
                &behavior,
                "rust.testing.overlap-review",
                "Review this supporting test for a distinct important risk before retaining or removing it.",
            );
            row["candidate"] = serde_json::json!(support.test);
            findings.push(row);
        }
    }
    BehaviorReview {
        declared: behavior,
        observed,
        status: status.into(),
    }
}

pub(super) fn resolve(selector: &TestSelector, tests: &[Value]) -> &'static str {
    match tests
        .iter()
        .filter(|test| {
            test["package_name"] == selector.package
                && test["target_name"] == selector.target
                && test["path"] == selector.path
                && test["qualified_name"] == selector.name
        })
        .count()
    {
        0 => "unresolved",
        1 => "resolved",
        _ => "ambiguous",
    }
}

pub(super) fn owner_status(observed: &[Observation], run: &str) -> &'static str {
    if observed.is_empty() || observed.iter().any(|row| row.resolution != "resolved") {
        "unresolved"
    } else if observed.iter().any(|row| row.execution == "failed") {
        "observed_failure"
    } else if run == "passed" && observed.iter().all(|row| row.execution == "passed") {
        "observed_pass"
    } else {
        "unknown"
    }
}
