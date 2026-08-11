use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::MeasureTool;
use crate::config::LensConfig;
use crate::util::write_json;

#[derive(Clone, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum FailPolicy {
    Partial,
    TestFailure,
    PracticeFailure,
    ReliabilityFinding,
    Regression,
    Threshold,
}

pub(crate) fn run_check(
    config: &LensConfig,
    baseline: Option<PathBuf>,
    fail_on: &[FailPolicy],
    max_total_score: f64,
    max_regression: f64,
) -> Result<()> {
    let documents = measurement_documents(config)?;
    let stale_artifacts = stale_artifacts(config, &documents);
    let partial_artifacts = partial_artifacts(&documents, &stale_artifacts);
    let test_failures = test_failures(&documents);
    let practices = classify_findings(config, practice_findings(&documents));
    let reliability = classify_findings(config, reliability_findings(&documents));
    let policy_rule_evaluations = evaluate_policy_rules(
        &config.policy,
        practices.active.iter().chain(&reliability.active),
    );
    let policy_rule_violations = policy_rule_evaluations
        .iter()
        .filter(|evaluation| evaluation["status"] == "exceeded")
        .cloned()
        .collect::<Vec<_>>();
    let policy_rule_errors = policy_rule_violations
        .iter()
        .filter(|evaluation| evaluation["level"] == "error")
        .count();
    let mut waived_findings = practices.waived;
    waived_findings.extend(reliability.waived);
    let expired_waivers = config
        .policy
        .expired_waivers()
        .into_iter()
        .map(|waiver| serde_json::to_value(waiver).unwrap_or_else(|_| json!({})))
        .collect::<Vec<_>>();
    let current_map = documents.get("map.json").map(artifact_payload);
    let threshold_violations = current_map
        .map(|map| threshold_violations(map, max_total_score))
        .unwrap_or_default();
    let score_deltas = match baseline {
        Some(path) => {
            let current_document = documents
                .get("map.json")
                .context("map.json is required for baseline comparison")?;
            let baseline_map = read_baseline_map(&path, model_version(current_document))?;
            current_map
                .map(|current| score_deltas(current, &baseline_map))
                .unwrap_or_default()
        }
        None => Vec::new(),
    };
    let regressions = score_deltas
        .iter()
        .filter(|delta| {
            delta["delta"]
                .as_f64()
                .is_some_and(|delta| delta > max_regression)
        })
        .cloned()
        .collect::<Vec<_>>();

    let mut failed_policies = failed_cli_policies(
        fail_on,
        &partial_artifacts,
        &test_failures,
        &practices.errors,
        &reliability.errors,
        &regressions,
        &threshold_violations,
    );
    if policy_rule_errors > 0 {
        failed_policies.push("configured-rules".to_string());
    }
    let mut enabled_policies = fail_on
        .iter()
        .map(|policy| policy.as_str().to_string())
        .collect::<Vec<_>>();
    if !config.policy.rules.is_empty() {
        enabled_policies.push("configured-rules".to_string());
    }
    let report = json!({
        "version": 2,
        "generated_from": "rqlens",
        "passed": failed_policies.is_empty(),
        "enabled_policies": enabled_policies,
        "failed_policies": failed_policies,
        "partial_artifacts": partial_artifacts,
        "stale_artifacts": stale_artifacts,
        "test_failures": test_failures,
        "practice_failures": practices.errors,
        "practice_warnings": practices.warnings,
        "reliability_findings": reliability.errors,
        "reliability_warnings": reliability.warnings,
        "waived_findings": waived_findings,
        "expired_waivers": expired_waivers,
        "policy_rule_evaluations": policy_rule_evaluations,
        "policy_rule_violations": policy_rule_violations,
        "threshold_violations": threshold_violations,
        "regressions": regressions,
        "score_deltas": score_deltas,
        "limits": {"max_total_score": max_total_score, "max_regression": max_regression},
    });
    let output = config.output_dir.join("policy_report.json");
    write_json(&output, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if report["passed"] == false {
        bail!(
            "quality policy failed; report written to {}",
            output.display()
        );
    }
    Ok(())
}

fn failed_cli_policies(
    fail_on: &[FailPolicy],
    partial_artifacts: &[String],
    test_failures: &[Value],
    practice_failures: &[Value],
    reliability_findings: &[Value],
    regressions: &[Value],
    threshold_violations: &[Value],
) -> Vec<String> {
    fail_on
        .iter()
        .filter(|policy| match policy {
            FailPolicy::Partial => !partial_artifacts.is_empty(),
            FailPolicy::TestFailure => !test_failures.is_empty(),
            FailPolicy::PracticeFailure => !practice_failures.is_empty(),
            FailPolicy::ReliabilityFinding => !reliability_findings.is_empty(),
            FailPolicy::Regression => !regressions.is_empty(),
            FailPolicy::Threshold => !threshold_violations.is_empty(),
        })
        .map(|policy| policy.as_str().to_string())
        .collect()
}

impl FailPolicy {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Partial => "partial",
            Self::TestFailure => "test-failure",
            Self::PracticeFailure => "practice-failure",
            Self::ReliabilityFinding => "reliability-finding",
            Self::Regression => "regression",
            Self::Threshold => "threshold",
        }
    }
}

fn measurement_documents(config: &LensConfig) -> Result<BTreeMap<String, Value>> {
    let files = MeasureTool::schema_tools()
        .into_iter()
        .map(|tool| tool.output_file())
        .collect::<BTreeSet<_>>();
    let mut documents = BTreeMap::new();
    for file in files {
        let path = config.output_dir.join(file);
        if !path.exists() {
            continue;
        }
        let value = serde_json::from_str(&fs::read_to_string(&path)?)
            .with_context(|| format!("parsing {}", path.display()))?;
        documents.insert(file.to_string(), value);
    }
    Ok(documents)
}

fn partial_artifacts(
    documents: &BTreeMap<String, Value>,
    stale_artifacts: &[String],
) -> Vec<String> {
    let mut partial = documents
        .iter()
        .filter(|(_, document)| document["measurement_confidence"]["partial"] == true)
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    partial.extend(stale_artifacts.iter().cloned());
    for tool in MeasureTool::all_tools() {
        let name = tool.output_file();
        if !documents.contains_key(name) && !partial.iter().any(|item| item == name) {
            partial.push(name.to_string());
        }
    }
    partial.sort();
    partial.dedup();
    partial
}

fn stale_artifacts(config: &LensConfig, documents: &BTreeMap<String, Value>) -> Vec<String> {
    let current =
        crate::util::project_input_fingerprint(&config.project_root, &config.source_roots);
    let current_digest = current["digest"].as_str();
    documents
        .iter()
        .filter(|(_, document)| document["input_fingerprint"]["digest"].as_str() != current_digest)
        .map(|(name, _)| name.clone())
        .collect()
}

fn test_failures(documents: &BTreeMap<String, Value>) -> Vec<Value> {
    let Some(document) = documents.get("correctness_review.json") else {
        return vec![json!({"reason": "correctness_review.json is missing"})];
    };
    let data = artifact_payload(document);
    let failed = data["summary"]["failed"].as_u64().unwrap_or(0);
    let run_failed = data["summary"]["run_failed"].as_bool().unwrap_or(false);
    if failed > 0 || run_failed {
        vec![json!({
            "failed_tests": failed,
            "run_failed": run_failed,
            "compile_failed": data["summary"]["compile_failed"].as_bool().unwrap_or(false),
        })]
    } else {
        Vec::new()
    }
}

fn practice_findings(documents: &BTreeMap<String, Value>) -> Vec<Value> {
    let Some(document) = documents.get("rust_practices.json") else {
        return vec![json!({"reason": "rust_practices.json is missing"})];
    };
    let data = artifact_payload(document);
    data["checks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|check| {
            matches!(
                check["status"].as_str(),
                Some("failed" | "unavailable" | "timed-out")
            )
        })
        .cloned()
        .collect()
}

fn reliability_findings(documents: &BTreeMap<String, Value>) -> Vec<Value> {
    let Some(document) = documents.get("reliability_findings.json") else {
        return vec![json!({"reason": "reliability_findings.json is missing"})];
    };
    artifact_payload(document)
        .as_array()
        .into_iter()
        .flatten()
        .filter(|finding| finding["severity"] != "advisory" && finding["scope"] == "production")
        .cloned()
        .collect()
}

struct FindingClassification {
    active: Vec<Value>,
    errors: Vec<Value>,
    warnings: Vec<Value>,
    waived: Vec<Value>,
}

fn classify_findings(config: &LensConfig, findings: Vec<Value>) -> FindingClassification {
    let (active, waived) = apply_waivers(config, findings);
    let (errors, warnings) = active
        .iter()
        .cloned()
        .partition(|finding| finding["severity"] == "error");
    FindingClassification {
        active,
        errors,
        warnings,
        waived,
    }
}

fn apply_waivers(config: &LensConfig, findings: Vec<Value>) -> (Vec<Value>, Vec<Value>) {
    let mut active = Vec::new();
    let mut waived = Vec::new();
    for finding in findings {
        let waiver = finding["rule_id"].as_str().and_then(|rule_id| {
            config
                .policy
                .active_waiver(rule_id, finding["path"].as_str())
        });
        if let Some(waiver) = waiver {
            waived.push(json!({"finding": finding, "waiver": waiver}));
        } else {
            active.push(finding);
        }
    }
    (active, waived)
}

fn evaluate_policy_rules<'a>(
    policy: &crate::config::PolicyConfig,
    findings: impl Iterator<Item = &'a Value> + Clone,
) -> Vec<Value> {
    policy
        .rules
        .iter()
        .map(|(rule_id, rule)| {
            let matching = findings
                .clone()
                .filter(|finding| finding["rule_id"].as_str() == Some(rule_id))
                .collect::<Vec<_>>();
            let observed = matching
                .iter()
                .filter(|finding| {
                    rule.includes(finding["path"].as_str(), finding["package_name"].as_str())
                })
                .count();
            let excluded = matching.len().saturating_sub(observed);
            let exceeded = observed > rule.max && rule.level != crate::config::PolicyRuleLevel::Off;
            json!({
                "rule_id": rule_id,
                "level": rule.level.as_str(),
                "maximum": rule.max,
                "observed": observed,
                "excess": observed.saturating_sub(rule.max),
                "excluded": excluded,
                "status": if rule.level == crate::config::PolicyRuleLevel::Off {
                    "disabled"
                } else if exceeded {
                    "exceeded"
                } else {
                    "passed"
                },
            })
        })
        .collect()
}

fn threshold_violations(map: &Value, maximum: f64) -> Vec<Value> {
    map["graph"]["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|node| {
            let data = &node["data"];
            let score = data["total_score"].as_f64()?;
            (score > maximum)
                .then(|| json!({"module": data["id"], "score": score, "maximum": maximum}))
        })
        .collect()
}

fn read_baseline_map(path: &Path, current_model_version: Option<u64>) -> Result<Value> {
    let path = if path.is_dir() {
        path.join("map.json")
    } else {
        path.to_path_buf()
    };
    let value: Value = serde_json::from_str(
        &fs::read_to_string(&path)
            .with_context(|| format!("reading baseline {}", path.display()))?,
    )?;
    let baseline_model_version = model_version(&value);
    if baseline_model_version != current_model_version {
        bail!(
            "baseline risk model version {:?} does not match current version {:?}",
            baseline_model_version,
            current_model_version
        );
    }
    Ok(artifact_payload(&value).clone())
}

fn model_version(value: &Value) -> Option<u64> {
    value["risk_model_version"]
        .as_u64()
        .or_else(|| artifact_payload(value)["meta"]["risk_model_version"].as_u64())
}

fn score_deltas(current: &Value, baseline: &Value) -> Vec<Value> {
    let old_scores = node_scores(baseline);
    node_scores(current)
        .into_iter()
        .map(|(module, current)| {
            let baseline = old_scores.get(&module).copied().unwrap_or(0.0);
            let delta = current - baseline;
            json!({
                "module": module,
                "baseline": baseline,
                "current": current,
                "delta": crate::util::round2(delta),
                "direction": if !old_scores.contains_key(&module) {
                    "new"
                } else if delta > 0.0 {
                    "regressed"
                } else if delta < 0.0 {
                    "improved"
                } else {
                    "unchanged"
                },
                "new_module": !old_scores.contains_key(&module),
            })
        })
        .collect()
}

fn node_scores(map: &Value) -> BTreeMap<String, f64> {
    map["graph"]["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|node| {
            Some((
                node["data"]["id"].as_str()?.to_string(),
                node["data"]["total_score"].as_f64()?,
            ))
        })
        .collect()
}

fn artifact_payload(value: &Value) -> &Value {
    value
        .get("data")
        .or_else(|| value.get("records"))
        .unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::evaluate_policy_rules;
    use crate::config::{PolicyConfig, PolicyRule, PolicyRuleLevel};
    use serde_json::json;
    use std::collections::BTreeMap;

    #[test]
    fn configured_rules_enforce_limits_after_exclusions() {
        let policy = PolicyConfig {
            rules: BTreeMap::from([(
                "rust.reliability.expect".to_string(),
                PolicyRule {
                    level: PolicyRuleLevel::Error,
                    max: 1,
                    exclude_paths: vec!["src/generated/**".to_string()],
                    exclude_packages: Vec::new(),
                },
            )]),
            waivers: Vec::new(),
        };
        let findings = [
            json!({"rule_id": "rust.reliability.expect", "path": "src/lib.rs", "package_name": "app"}),
            json!({"rule_id": "rust.reliability.expect", "path": "src/main.rs", "package_name": "app"}),
            json!({"rule_id": "rust.reliability.expect", "path": "src/generated/model.rs", "package_name": "app"}),
        ];
        let evaluations = evaluate_policy_rules(&policy, findings.iter());
        assert_eq!(evaluations[0]["observed"], 2);
        assert_eq!(evaluations[0]["excluded"], 1);
        assert_eq!(evaluations[0]["status"], "exceeded");
    }
}
