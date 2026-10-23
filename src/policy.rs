use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::config::LensConfig;
use crate::contracts::InputStatus;
use crate::tool::MeasureTool;
use crate::util::write_json;

#[cfg(test)]
#[path = "policy/tests.rs"]
mod input_tests;
mod inputs;

#[derive(Clone, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum FailPolicy {
    Partial,
    TestFailure,
    PracticeFailure,
    ReliabilityFinding,
    OperationalFailure,
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
    let inputs = inputs::Inputs::load(config);
    let mut input_failures = inputs.failures(config, fail_on, baseline.as_deref());
    let stale_artifacts = inputs
        .status
        .iter()
        .filter(|(_, state)| state.status == InputStatus::Stale)
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    let documents = &inputs.documents;
    let partial_artifacts = partial_artifacts(documents, &stale_artifacts);
    let test_failures = test_failures(documents);
    let practice_projection =
        crate::findings::practices::project(config, documents.get("rust_practices.json"));
    let practices = classify_findings(config, practice_projection.findings);
    let source_findings = |file| {
        classify_findings(
            config,
            crate::findings::source(config, file, documents.get(file)),
        )
    };
    let reliability = source_findings("reliability_findings.json");
    let architecture = source_findings("architecture_rules.json");
    let test_quality = source_findings("test_quality.json");
    let operational_failures = operational_failures(documents);
    let external_evidence = crate::external::current_findings(config);
    let external = classify_findings(config, external_evidence.findings);
    input_failures.extend(external_input_failures(config, &external_evidence.reasons));
    let classified = [
        &practices,
        &reliability,
        &architecture,
        &test_quality,
        &external,
    ];
    let policy_rule_evaluations = evaluate_policy_rules(
        &config.policy,
        classified.iter().flat_map(|group| group.active.iter()),
        |rule| inputs.rule_unavailability(config, rule),
    );
    let policy_rule_violations = policy_rule_evaluations
        .iter()
        .filter(|evaluation| evaluation["status"] == "exceeded")
        .cloned()
        .collect::<Vec<_>>();
    let policy_rule_errors = policy_rule_evaluations
        .iter()
        .filter(|evaluation| {
            ["exceeded", "incomplete"].contains(&evaluation["status"].as_str().unwrap_or_default())
        })
        .filter(|evaluation| evaluation["level"] == "error")
        .count();
    let waived_findings = classified
        .iter()
        .flat_map(|group| &group.waived)
        .collect::<Vec<_>>();
    let expired_waivers = config.policy.expired_waivers();
    let current_map = documents.get("map.json").map(artifact_payload);
    let threshold_violations = current_map
        .map(|map| threshold_violations(map, max_total_score))
        .unwrap_or_default();
    let regression = checked_regression(
        config,
        &inputs,
        baseline.as_deref(),
        max_regression,
        &mut input_failures,
    );

    if fail_on.contains(&FailPolicy::Regression) {
        input_failures.extend(regression.evidence_deltas.iter()
            .filter(|delta| delta["status"] == "incomparable")
            .map(|delta| json!({"policy": "regression", "artifact": "performance.json", "status": "incomparable", "reason": delta["reason"], "benchmark": delta["benchmark"]})));
    }

    let policy_evidence = PolicyEvidence {
        partial_artifacts: &partial_artifacts,
        test_failures: &test_failures,
        practice_failures: &practices.errors,
        reliability_findings: &reliability.errors,
        operational_failures: &operational_failures,
        regressions: &regression.regressions,
        threshold_violations: &threshold_violations,
    };
    let mut failed_policies = failed_cli_policies(fail_on, &policy_evidence);
    failed_policies.extend(
        input_failures
            .iter()
            .filter_map(|failure| failure["policy"].as_str())
            .map(str::to_string),
    );
    if policy_rule_errors > 0 {
        failed_policies.push("configured-rules".to_string());
    }
    if !architecture.errors.is_empty() {
        failed_policies.push("architecture-rules".to_string());
    }
    let enabled_policies = enabled_policies(config, fail_on);
    failed_policies.sort();
    failed_policies.dedup();
    let report = json!({
        "version": 2,
        "generated_from": "rqlens",
        "passed": failed_policies.is_empty(),
        "enabled_policies": enabled_policies,
        "failed_policies": failed_policies,
        "partial_artifacts": partial_artifacts,
        "stale_artifacts": stale_artifacts,
        "artifact_status": inputs.status,
        "input_failures": input_failures,
        "test_failures": test_failures,
        "practice_evidence": practice_projection.evidence,
        "practice_failures": practices.errors,
        "practice_warnings": practices.warnings,
        "reliability_findings": reliability.errors,
        "reliability_warnings": reliability.warnings,
        "architecture_violations": architecture.errors,
        "architecture_warnings": architecture.warnings,
        "test_quality_findings": test_quality.errors,
        "test_quality_warnings": test_quality.warnings,
        "reliability_out_of_scope": reliability.out_of_scope,
        "source_evidence": crate::findings::SOURCE_FILES.iter().map(|(file, _)| (*file, documents.get(*file))).collect::<BTreeMap<_, _>>(),
        "operational_failures": operational_failures,
        "external_findings": external.active,
        "external_evidence_reasons": external_evidence.reasons,
        "external_evidence": external_evidence.documents,
        "waived_findings": waived_findings,
        "expired_waivers": expired_waivers,
        "policy_rule_evaluations": policy_rule_evaluations,
        "policy_rule_violations": policy_rule_violations,
        "threshold_violations": threshold_violations,
        "regressions": regression.regressions,
        "score_deltas": regression.score_deltas,
        "evidence_deltas": regression.evidence_deltas,
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

fn checked_regression(
    config: &LensConfig,
    inputs: &inputs::Inputs,
    baseline: Option<&Path>,
    maximum: f64,
    failures: &mut Vec<Value>,
) -> RegressionEvidence {
    if inputs.status["map.json"].status != InputStatus::Available {
        return RegressionEvidence::default();
    }
    let documents = &inputs.documents;
    match regression_evidence(
        config,
        documents,
        documents.get("map.json").map(artifact_payload),
        baseline,
        maximum,
    ) {
        Ok(evidence) => evidence,
        Err(error) => {
            failures.push(json!({"policy": "regression", "artifact": "baseline", "status": "incompatible", "reason": format!("{error:#}")}));
            RegressionEvidence::default()
        }
    }
}

fn external_input_failures(config: &LensConfig, reasons: &[String]) -> Vec<Value> {
    if config.policy.external_adapters().next().is_none() {
        return Vec::new();
    }
    reasons.iter().map(|reason| json!({"policy": "external-evidence", "status": "incomplete", "reason": reason})).collect()
}

fn enabled_policies(config: &LensConfig, fail_on: &[FailPolicy]) -> Vec<String> {
    let mut policies = fail_on
        .iter()
        .map(|policy| policy.as_str().to_string())
        .collect::<Vec<_>>();
    if !config.policy.rules.is_empty() {
        policies.push("configured-rules".to_string());
    }
    if !config.architecture.rules.is_empty() {
        policies.push("architecture-rules".to_string());
    }
    policies
}

#[derive(Default)]
struct RegressionEvidence {
    score_deltas: Vec<Value>,
    evidence_deltas: Vec<Value>,
    regressions: Vec<Value>,
}

fn regression_evidence(
    config: &LensConfig,
    documents: &BTreeMap<String, Value>,
    current_map: Option<&Value>,
    baseline: Option<&Path>,
    max_regression: f64,
) -> Result<RegressionEvidence> {
    let Some(path) = baseline else {
        return Ok(RegressionEvidence::default());
    };
    let current_document = documents
        .get("map.json")
        .context("map.json is required for baseline comparison")?;
    let baseline_map = read_baseline_map(path, model_version(current_document))?;
    let score_deltas = current_map
        .map(|current| score_deltas(current, &baseline_map))
        .unwrap_or_default();
    let mut evidence_deltas = coverage_regression(config, documents, path, max_regression)?
        .into_iter()
        .collect::<Vec<_>>();
    evidence_deltas.extend(performance_regressions(documents, path, max_regression)?);
    let mut regressions = score_deltas
        .iter()
        .filter(|delta| {
            delta["delta"]
                .as_f64()
                .is_some_and(|delta| delta > max_regression)
        })
        .cloned()
        .collect::<Vec<_>>();
    regressions.extend(
        evidence_deltas
            .iter()
            .filter(|delta| delta["status"] == "regressed")
            .cloned(),
    );
    Ok(RegressionEvidence {
        score_deltas,
        evidence_deltas,
        regressions,
    })
}

struct PolicyEvidence<'a> {
    partial_artifacts: &'a [String],
    test_failures: &'a [Value],
    practice_failures: &'a [Value],
    reliability_findings: &'a [Value],
    operational_failures: &'a [Value],
    regressions: &'a [Value],
    threshold_violations: &'a [Value],
}

fn failed_cli_policies(fail_on: &[FailPolicy], evidence: &PolicyEvidence<'_>) -> Vec<String> {
    fail_on
        .iter()
        .filter(|policy| match policy {
            FailPolicy::Partial => !evidence.partial_artifacts.is_empty(),
            FailPolicy::TestFailure => !evidence.test_failures.is_empty(),
            FailPolicy::PracticeFailure => !evidence.practice_failures.is_empty(),
            FailPolicy::ReliabilityFinding => !evidence.reliability_findings.is_empty(),
            FailPolicy::OperationalFailure => !evidence.operational_failures.is_empty(),
            FailPolicy::Regression => !evidence.regressions.is_empty(),
            FailPolicy::Threshold => !evidence.threshold_violations.is_empty(),
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
            Self::OperationalFailure => "operational-failure",
            Self::Regression => "regression",
            Self::Threshold => "threshold",
        }
    }
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

fn test_failures(documents: &BTreeMap<String, Value>) -> Vec<Value> {
    let Some(document) = documents.get("correctness_review.json") else {
        return vec![json!({"reason": "correctness_review.json is missing"})];
    };
    let data = artifact_payload(document);
    let failed = data["summary"]["failed"].as_u64().unwrap_or(0);
    let run_failed = data["summary"]["run_failed"].as_bool().unwrap_or(false);
    if failed > 0 || run_failed || data["summary"]["compile_failed"] == true {
        vec![json!({
            "failed_tests": failed,
            "run_failed": run_failed,
            "compile_failed": data["summary"]["compile_failed"].as_bool().unwrap_or(false),
        })]
    } else {
        Vec::new()
    }
}

fn operational_failures(documents: &BTreeMap<String, Value>) -> Vec<Value> {
    documents
        .get("operational_evidence.json")
        .map(artifact_payload)
        .and_then(|data| data["records"].as_array())
        .into_iter()
        .flatten()
        .filter(|record| record["status"] == "breached")
        .cloned()
        .collect()
}

struct FindingClassification {
    active: Vec<Value>,
    errors: Vec<Value>,
    warnings: Vec<Value>,
    waived: Vec<Value>,
    out_of_scope: Vec<Value>,
}

fn classify_findings(config: &LensConfig, findings: Vec<Value>) -> FindingClassification {
    let (active, waived) = apply_waivers(config, findings);
    let (active, out_of_scope): (Vec<_>, Vec<_>) = active
        .into_iter()
        .partition(|finding| finding["policy_selected"] != false);
    let (errors, warnings) = active
        .iter()
        .cloned()
        .partition(|finding| crate::findings::finding_level(finding) == "error");
    FindingClassification {
        active,
        errors,
        warnings,
        waived,
        out_of_scope,
    }
}

fn apply_waivers(config: &LensConfig, findings: Vec<Value>) -> (Vec<Value>, Vec<Value>) {
    let mut active = Vec::new();
    let mut waived = Vec::new();
    for finding in findings {
        if let Some(waiver) = crate::findings::active_waiver(config, &finding) {
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
    unavailability: impl Fn(&str) -> Option<String>,
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
            let disabled = rule.level == crate::config::PolicyRuleLevel::Off;
            let reason = if disabled {
                None
            } else {
                unavailability(rule_id)
            };
            let complete = !disabled && reason.is_none();
            json!({
                "rule_id": rule_id,
                "level": rule.level.as_str(),
                "maximum": rule.max,
                "observed": complete.then_some(observed),
                "observed_findings": observed,
                "excess": complete.then_some(observed.saturating_sub(rule.max)),
                "excluded": excluded,
                "evaluation_complete": complete,
                "reason": reason,
                "status": if disabled {
                    "disabled"
                } else if !complete {
                    "incomplete"
                } else if observed > rule.max {
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

fn baseline_artifact(baseline: &Path, file: &str) -> PathBuf {
    let directory = if baseline.is_dir() {
        baseline
    } else {
        baseline.parent().unwrap_or_else(|| Path::new("."))
    };
    directory.join(file)
}

fn coverage_regression(
    config: &LensConfig,
    documents: &BTreeMap<String, Value>,
    baseline: &Path,
    maximum: f64,
) -> Result<Option<Value>> {
    let Some(current) = documents.get("coverage.json") else {
        return Ok(None);
    };
    let path = baseline_artifact(baseline, "coverage.json");
    let old = crate::contracts::read_baseline(&path, "coverage.json")?;
    crate::contracts::validate_comparison_evidence("coverage.json", current)?;
    let current_percent = crate::contracts::coverage_percent(current)?;
    let baseline_percent = crate::contracts::coverage_percent(&old)?;
    let decrease = crate::util::round2(baseline_percent - current_percent);
    Ok(Some(json!({
        "metric": "line_coverage_percent",
        "baseline": baseline_percent,
        "current": current_percent,
        "decrease": decrease,
        "maximum_decrease": maximum,
        "status": if decrease > maximum { "regressed" } else if decrease < 0.0 { "improved" } else { "unchanged" },
        "project": config.project_name,
    })))
}

fn performance_regressions(
    documents: &BTreeMap<String, Value>,
    baseline: &Path,
    maximum: f64,
) -> Result<Vec<Value>> {
    let Some(current) = documents.get("performance.json") else {
        return Ok(Vec::new());
    };
    let path = baseline_artifact(baseline, "performance.json");
    let old = crate::contracts::read_baseline(&path, "performance.json")?;
    crate::contracts::validate_comparison_evidence("performance.json", current)?;
    let current_records = crate::contracts::benchmark_records(&current["data"]["records"])?;
    let old_records = crate::contracts::benchmark_records(&old["data"]["records"])?;
    Ok(
        crate::performance::comparisons(&current_records, &old_records)?
            .into_iter()
            .map(|comparison| {
                let increase = comparison["delta_percent"].as_f64();
                json!({
                    "metric": "benchmark_estimate_percent",
                    "benchmark": comparison["id"],
                    "baseline": comparison["baseline_nanoseconds"],
                    "current": comparison["current_nanoseconds"],
                    "increase": increase,
                    "maximum_increase": maximum,
                    "reason": comparison["reason"],
                    "status": match comparison["direction"].as_str() {
                        Some("regressed") if increase.is_some_and(|value| value > maximum) => "regressed",
                        Some("improved") => "improved",
                        Some("regressed" | "within_noise" | "unchanged") => "unchanged",
                        _ => "incomparable",
                    },
                })
            })
            .collect(),
    )
}

fn read_baseline_map(path: &Path, current_model_version: Option<u64>) -> Result<Value> {
    let value = crate::contracts::read_baseline(path, "map.json")?;
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
        let evaluations = evaluate_policy_rules(&policy, findings.iter(), |_| None);
        assert_eq!(evaluations[0]["observed"], 2);
        assert_eq!(evaluations[0]["excluded"], 1);
        assert_eq!(evaluations[0]["status"], "exceeded");
    }
}
