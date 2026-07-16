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
    let partial_artifacts = partial_artifacts(&documents);
    let test_failures = test_failures(&documents);
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

    let failed_policies = fail_on
        .iter()
        .filter(|policy| match policy {
            FailPolicy::Partial => !partial_artifacts.is_empty(),
            FailPolicy::TestFailure => !test_failures.is_empty(),
            FailPolicy::Regression => !regressions.is_empty(),
            FailPolicy::Threshold => !threshold_violations.is_empty(),
        })
        .map(FailPolicy::as_str)
        .collect::<Vec<_>>();
    let report = json!({
        "version": 1,
        "generated_from": "rqlens",
        "passed": failed_policies.is_empty(),
        "enabled_policies": fail_on.iter().map(FailPolicy::as_str).collect::<Vec<_>>(),
        "failed_policies": failed_policies,
        "partial_artifacts": partial_artifacts,
        "test_failures": test_failures,
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

impl FailPolicy {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Partial => "partial",
            Self::TestFailure => "test-failure",
            Self::Regression => "regression",
            Self::Threshold => "threshold",
        }
    }
}

fn measurement_documents(config: &LensConfig) -> Result<BTreeMap<String, Value>> {
    let files = MeasureTool::all_tools()
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

fn partial_artifacts(documents: &BTreeMap<String, Value>) -> Vec<String> {
    let mut partial = documents
        .iter()
        .filter(|(_, document)| document["measurement_confidence"]["partial"] == true)
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
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
