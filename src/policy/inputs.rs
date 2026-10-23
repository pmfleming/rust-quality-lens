use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use super::FailPolicy;
use crate::config::{LensConfig, PolicyRuleLevel};
use crate::contracts::{ArtifactHeader, InputState, InputStatus, validate_artifact};
use crate::tool::MeasureTool;

#[derive(Default)]
pub(super) struct Inputs {
    pub(super) documents: BTreeMap<String, Value>,
    pub(super) status: BTreeMap<String, InputState>,
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
            let state = match read_document(config, file) {
                Ok((document, header)) => {
                    let state = document_state(file, &document, &header, &current);
                    inputs.documents.insert(file.to_string(), document);
                    state
                }
                Err(state) => state,
            };
            inputs.status.insert(file.to_string(), state);
        }
        inputs
    }

    pub(super) fn failures(
        &self,
        config: &LensConfig,
        policies: &[FailPolicy],
        baseline: Option<&Path>,
    ) -> Vec<Value> {
        let mut failures = Vec::new();
        for policy in policies {
            for file in required_files(policy) {
                self.require(policy.as_str(), file, &mut failures);
            }
            if *policy == FailPolicy::Regression {
                self.require_regression_inputs(baseline, &mut failures);
            }
            failures.extend(self.semantic_failure(policy));
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
        // Custom rule IDs need every finding source, not a guessed producer prefix.
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

    pub(super) fn rule_unavailability(&self, config: &LensConfig, rule: &str) -> Option<String> {
        let Ok(contracts) =
            serde_json::from_str::<Value>(include_str!("../../contracts/blocking-rules.json"))
        else {
            return Some("The blocking-rule contract inventory is malformed".into());
        };
        let architecture = config
            .architecture
            .rules
            .iter()
            .find(|entry| entry.id == rule);
        if architecture.is_some_and(|entry| {
            entry.level == PolicyRuleLevel::Off || contracts["rules"].get(rule).is_some()
        }) {
            return Some(
                "The architecture check is disabled or its ID collides with a native rule".into(),
            );
        }
        let family = contracts["rules"][rule]
            .as_str()
            .or_else(|| architecture.map(|_| "architecture"));
        let Some(file) = family.and_then(|name| contracts["families"][name]["artifact"].as_str())
        else {
            return Some("No supported per-rule evaluation coverage; finding absence is not a negative result".into());
        };
        if self.status[file].status != InputStatus::Available {
            return Some(format!("{file}: {:?}", self.status[file].status));
        }
        if rule == crate::testing_policy::REQUIRED_OWNER_RULE {
            return crate::testing_policy::required_owner_unavailability(
                &self.documents[file]["data"],
            );
        }
        if file == "rust_practices.json" {
            let matching = self.documents[file]["data"]["checks"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|check| check["rule_id"] == rule)
                .collect::<Vec<_>>();
            if matching.is_empty()
                || matching
                    .iter()
                    .any(|check| !matches!(check["status"].as_str(), Some("passed" | "failed")))
            {
                return Some("The practice check is absent, disabled, unavailable or has an unknown execution status".into());
            }
        }
        None
    }

    fn require_regression_inputs(&self, baseline: Option<&Path>, failures: &mut Vec<Value>) {
        for file in ["coverage.json", "performance.json"] {
            if self.status[file].status != InputStatus::Missing
                || baseline.is_some_and(|path| super::baseline_artifact(path, file).exists())
            {
                self.require("regression", file, failures);
            }
        }
        if baseline.is_none() {
            failures.push(json!({"policy": "regression", "artifact": "baseline", "status": "missing", "reason": "regression policy requires --baseline"}));
        }
    }

    fn semantic_failure(&self, policy: &FailPolicy) -> Option<Value> {
        let (file, reason) = match policy {
            FailPolicy::TestFailure
                if self
                    .documents
                    .get("correctness_review.json")
                    .is_some_and(|document| {
                        document["data"]["summary"]["last_run"]["status"] != "passed"
                            || document["data"]["summary"]["unknown"].as_u64() != Some(0)
                    }) =>
            {
                (
                    "correctness_review.json",
                    "test execution must be observed and all test statuses known",
                )
            }
            FailPolicy::Threshold | FailPolicy::Regression
                if self.documents.get("map.json").is_some_and(unknown_scores) =>
            {
                ("map.json", "one or more modules have unknown scores")
            }
            _ => return None,
        };
        Some(
            json!({"policy": policy.as_str(), "artifact": file, "status": "partial", "reason": reason}),
        )
    }

    fn require(&self, policy: &str, file: &str, failures: &mut Vec<Value>) {
        let status = &self.status[file];
        if status.status != InputStatus::Available {
            failures.push(json!({"policy": policy, "artifact": file, "status": status.status, "reason": status.reason}));
        }
    }
}

fn required_files(policy: &FailPolicy) -> Vec<&'static str> {
    match policy {
        FailPolicy::Partial => MeasureTool::all_tools()
            .iter()
            .map(MeasureTool::output_file)
            .collect(),
        FailPolicy::TestFailure => vec!["correctness_review.json"],
        FailPolicy::PracticeFailure => vec!["rust_practices.json"],
        FailPolicy::ReliabilityFinding => vec!["reliability_findings.json"],
        FailPolicy::OperationalFailure => vec!["operational_evidence.json"],
        FailPolicy::Threshold | FailPolicy::Regression => vec!["map.json"],
    }
}

fn unknown_scores(map: &Value) -> bool {
    map["data"]["graph"]["nodes"]
        .as_array()
        .is_some_and(|nodes| {
            nodes
                .iter()
                .any(|node| !node["data"]["total_score"].is_number())
        })
}

// Failed reads/validation have no usable document; stale but valid documents
// remain available for diagnostics, classified separately by document_state.
fn read_document(config: &LensConfig, file: &str) -> Result<(Value, ArtifactHeader), InputState> {
    let text = fs::read_to_string(config.output_dir.join(file)).map_err(|error| {
        let status = if error.kind() == std::io::ErrorKind::NotFound {
            InputStatus::Missing
        } else {
            InputStatus::Unreadable
        };
        InputState::new(status, Some(error.to_string()))
    })?;
    let document = serde_json::from_str(&text)
        .map_err(|error| InputState::new(InputStatus::Unreadable, Some(error.to_string())))?;
    let header = validate_artifact(file, &document)
        .map_err(|error| InputState::new(InputStatus::Incompatible, Some(error.to_string())))?;
    if file == "architecture_rules.json" {
        let compiler = config.compiler_architecture_for(&MeasureTool::ArchitectureRules);
        if compiler != (document["data"]["backend"] == "rustc-hir")
            || (compiler && document["data"]["rules"] != json!(config.architecture))
        {
            return Err(InputState::new(
                InputStatus::Incompatible,
                Some("architecture backend/rules differ from active configuration".into()),
            ));
        }
    }
    Ok((document, header))
}

fn document_state(
    file: &str,
    document: &Value,
    header: &ArtifactHeader,
    current: &Value,
) -> InputState {
    if file == "operational_evidence.json"
        && let Err(error) = crate::telemetry::validate_freshness(document, chrono::Utc::now())
    {
        return InputState::new(InputStatus::Stale, Some(error.to_string()));
    }
    use crate::fingerprint::Freshness;
    let status = match crate::fingerprint::freshness(document, current) {
        Freshness::Stale => InputStatus::Stale,
        Freshness::Incomplete => InputStatus::Partial,
        Freshness::Current if !header.measurement_confidence.is_complete() => InputStatus::Partial,
        Freshness::Current => InputStatus::Available,
    };
    InputState::new(status, None)
}
