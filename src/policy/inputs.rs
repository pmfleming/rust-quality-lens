use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;

use super::FailPolicy;
use crate::config::{LensConfig, PolicyRuleLevel};
use crate::contracts::validate_artifact;
use crate::tool::MeasureTool;

#[derive(Default)]
pub(super) struct Inputs {
    pub(super) documents: BTreeMap<String, Value>,
    pub(super) status: BTreeMap<String, Value>,
}

impl Inputs {
    pub(super) fn load(config: &LensConfig) -> Self {
        let mut inputs = Self::default();
        let current = config.input_fingerprint();
        for file in MeasureTool::schema_tools()
            .into_iter()
            .map(|tool| tool.output_file())
            .chain([
                "performance.json",
                "repository_outcomes.json",
                "operational_evidence.json",
            ])
        {
            let text = match fs::read_to_string(config.output_dir.join(file)) {
                Ok(text) => text,
                Err(error) => {
                    let status = if error.kind() == std::io::ErrorKind::NotFound {
                        "missing"
                    } else {
                        "unreadable"
                    };
                    inputs.status.insert(
                        file.to_string(),
                        json!({"status": status, "reason": error.to_string()}),
                    );
                    continue;
                }
            };
            let document: Value = match serde_json::from_str(&text) {
                Ok(document) => document,
                Err(error) => {
                    inputs.status.insert(
                        file.to_string(),
                        json!({"status": "unreadable", "reason": error.to_string()}),
                    );
                    continue;
                }
            };
            if let Err(error) = validate_artifact(file, &document) {
                inputs.status.insert(
                    file.to_string(),
                    json!({"status": "incompatible", "reason": error.to_string()}),
                );
                continue;
            }
            if file == "operational_evidence.json"
                && let Err(error) =
                    crate::telemetry::validate_freshness(&document, chrono::Utc::now())
            {
                inputs.status.insert(
                    file.to_string(),
                    json!({"status": "stale", "reason": error.to_string()}),
                );
                inputs.documents.insert(file.to_string(), document);
                continue;
            }
            let status = match crate::fingerprint::freshness(&document, &current) {
                crate::fingerprint::Freshness::Stale => "stale",
                crate::fingerprint::Freshness::Incomplete => "partial",
                crate::fingerprint::Freshness::Current
                    if document["measurement_confidence"]["complete"] != true =>
                {
                    "partial"
                }
                crate::fingerprint::Freshness::Current => "available",
            };
            inputs
                .status
                .insert(file.to_string(), json!({"status": status}));
            inputs.documents.insert(file.to_string(), document);
        }
        inputs
    }

    pub(super) fn failures(
        &self,
        config: &LensConfig,
        policies: &[FailPolicy],
        baseline: Option<&std::path::Path>,
    ) -> Vec<Value> {
        let mut failures = Vec::new();
        for policy in policies {
            let files: Vec<&str> = match policy {
                FailPolicy::Partial => MeasureTool::all_tools()
                    .iter()
                    .map(MeasureTool::output_file)
                    .collect(),
                FailPolicy::TestFailure => vec!["correctness_review.json"],
                FailPolicy::PracticeFailure => vec!["rust_practices.json"],
                FailPolicy::ReliabilityFinding => vec!["reliability_findings.json"],
                FailPolicy::OperationalFailure => vec!["operational_evidence.json"],
                FailPolicy::Threshold | FailPolicy::Regression => vec!["map.json"],
            };
            for file in files {
                self.require(policy.as_str(), file, &mut failures);
            }
            if *policy == FailPolicy::TestFailure
                && let Some(document) = self.documents.get("correctness_review.json")
                && (document["data"]["summary"]["last_run"]["status"] != "passed"
                    || document["data"]["summary"]["unknown"].as_u64() != Some(0))
            {
                failures.push(json!({"policy": policy.as_str(), "artifact": "correctness_review.json", "status": "partial", "reason": "test execution must be observed and all test statuses known"}));
            }
            if *policy == FailPolicy::Regression {
                for file in ["coverage.json", "performance.json"] {
                    let baseline_file = baseline.map(|path| {
                        if path.is_dir() {
                            path.join(file)
                        } else {
                            path.parent()
                                .unwrap_or_else(|| std::path::Path::new("."))
                                .join(file)
                        }
                    });
                    if self.status[file]["status"] != "missing"
                        || baseline_file.is_some_and(|path| path.exists())
                    {
                        self.require(policy.as_str(), file, &mut failures);
                    }
                }
            }
            if *policy == FailPolicy::Regression && baseline.is_none() {
                failures.push(json!({"policy": policy.as_str(), "artifact": "baseline", "status": "missing", "reason": "regression policy requires --baseline"}));
            }
            if matches!(policy, FailPolicy::Threshold | FailPolicy::Regression)
                && let Some(map) = self.documents.get("map.json")
                && map["data"]["graph"]["nodes"]
                    .as_array()
                    .is_some_and(|nodes| {
                        nodes
                            .iter()
                            .any(|node| !node["data"]["total_score"].is_number())
                    })
            {
                failures.push(json!({"policy": policy.as_str(), "artifact": "map.json", "status": "partial", "reason": "one or more modules have unknown scores"}));
            }
        }
        if config
            .architecture
            .rules
            .iter()
            .any(|rule| rule.level == PolicyRuleLevel::Error)
        {
            self.require(
                "architecture-rules",
                "architecture_rules.json",
                &mut failures,
            );
        }
        // Rule IDs are configurable, including custom architecture rules. Require
        // all finding sources rather than guess a producer from a rule's prefix.
        if config
            .policy
            .rules
            .values()
            .any(|rule| rule.level == PolicyRuleLevel::Error)
        {
            for file in [
                "rust_practices.json",
                "reliability_findings.json",
                "architecture_rules.json",
                "test_quality.json",
            ] {
                self.require("configured-rules", file, &mut failures);
            }
        }
        failures
    }

    fn require(&self, policy: &str, file: &str, failures: &mut Vec<Value>) {
        let status = &self.status[file];
        if status["status"] != "available" {
            failures.push(json!({"policy": policy, "artifact": file, "status": status["status"], "reason": status["reason"]}));
        }
    }
}
