use anyhow::{Context, Result, ensure};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

use crate::config::LensConfig;
use crate::util::write_json;

#[cfg(test)]
#[path = "sarif_reporting_tests.rs"]
mod reporting_tests;

pub(crate) fn write(config: &LensConfig, output: Option<PathBuf>) -> Result<PathBuf> {
    let mut rules = BTreeMap::<String, Value>::new();
    let mut results = Vec::new();
    add_reliability_results(config, &mut rules, &mut results)?;
    add_finding_document(
        config,
        "architecture_rules.json",
        "violations",
        &mut rules,
        &mut results,
    )?;
    add_finding_document(
        config,
        "test_quality.json",
        "findings",
        &mut rules,
        &mut results,
    )?;
    add_practice_results(config, &mut rules, &mut results)?;
    let (external, external_reasons) = crate::external::current_findings(config);
    add_source_findings(config, external.iter(), &mut rules, &mut results);
    let document = json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "rust-quality-lens",
                    "informationUri": "https://github.com/pmfleming/rust-quality-lens",
                    "rules": rules.into_values().collect::<Vec<_>>(),
                }
            },
            "results": results,
            "properties": {"external_evidence_reasons": external_reasons},
        }]
    });
    let output = output.unwrap_or_else(|| config.output_dir.join("rqlens.sarif"));
    write_json(&output, &document)?;
    Ok(output)
}

fn add_reliability_results(
    config: &LensConfig,
    rules: &mut BTreeMap<String, Value>,
    results: &mut Vec<Value>,
) -> Result<()> {
    let Some(document) = read_optional(config, "reliability_findings.json")? else {
        return Ok(());
    };
    add_source_findings(
        config,
        payload(&document).as_array().into_iter().flatten(),
        rules,
        results,
    );
    Ok(())
}

fn add_finding_document(
    config: &LensConfig,
    file_name: &str,
    payload_key: &str,
    rules: &mut BTreeMap<String, Value>,
    results: &mut Vec<Value>,
) -> Result<()> {
    let Some(document) = read_optional(config, file_name)? else {
        return Ok(());
    };
    add_source_findings(
        config,
        payload(&document)[payload_key]
            .as_array()
            .into_iter()
            .flatten(),
        rules,
        results,
    );
    Ok(())
}

fn add_source_findings<'a>(
    config: &LensConfig,
    findings: impl Iterator<Item = &'a Value>,
    rules: &mut BTreeMap<String, Value>,
    results: &mut Vec<Value>,
) {
    for finding in findings {
        let Some(rule_id) = finding["rule_id"].as_str() else {
            continue;
        };
        let message = finding["message"].as_str().unwrap_or(rule_id);
        rules.entry(rule_id.to_string()).or_insert_with(|| {
            json!({
                "id": rule_id,
                "name": rule_id.replace(['.', '-'], "_"),
                "shortDescription": {"text": message},
                "helpUri": finding["source"],
            })
        });
        let mut result = Map::new();
        result.insert("ruleId".to_string(), json!(rule_id));
        result.insert(
            "level".to_string(),
            json!(crate::findings::finding_level(finding)),
        );
        result.insert("message".to_string(), json!({"text": message}));
        if let Some(path) = finding["path"].as_str() {
            result.insert(
                "locations".to_string(),
                json!([{
                    "physicalLocation": {
                        "artifactLocation": {"uri": path.replace('\\', "/")},
                        "region": {"startLine": finding["line"].as_u64().unwrap_or(1).max(1)}
                    }
                }]),
            );
        }
        let mut result = Value::Object(result);
        annotate_result(config, finding, &mut result);
        results.push(result);
    }
}

fn add_practice_results(
    config: &LensConfig,
    rules: &mut BTreeMap<String, Value>,
    results: &mut Vec<Value>,
) -> Result<()> {
    let Some(document) = read_optional(config, "rust_practices.json")? else {
        return Ok(());
    };
    let mut seen_diagnostics = BTreeSet::new();
    for check in payload(&document)["checks"]
        .as_array()
        .into_iter()
        .flatten()
    {
        for diagnostic in check["diagnostics"].as_array().into_iter().flatten() {
            add_compiler_diagnostic(
                config,
                check,
                diagnostic,
                &mut seen_diagnostics,
                rules,
                results,
            );
        }
        add_failed_practice(config, check, rules, results);
    }
    Ok(())
}

fn add_compiler_diagnostic(
    config: &LensConfig,
    check: &Value,
    diagnostic: &Value,
    seen: &mut BTreeSet<(String, String, u64, u64, String)>,
    rules: &mut BTreeMap<String, Value>,
    results: &mut Vec<Value>,
) {
    let Some(rule_id) = diagnostic["rule_id"].as_str() else {
        return;
    };
    let path = diagnostic["path"].as_str();
    let key = (
        rule_id.to_string(),
        path.unwrap_or_default().to_string(),
        diagnostic["line"].as_u64().unwrap_or_default(),
        diagnostic["column"].as_u64().unwrap_or_default(),
        diagnostic["message"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
    );
    if !seen.insert(key) {
        return;
    }
    rules.entry(rule_id.to_string()).or_insert_with(|| {
        json!({
            "id": rule_id,
            "name": rule_id.replace(['.', '-'], "_"),
            "shortDescription": {"text": diagnostic["message"]},
            "helpUri": check["source"],
        })
    });
    let mut result = json!({
        "ruleId": rule_id,
        "level": crate::findings::finding_level(diagnostic),
        "message": {"text": diagnostic["message"]},
    });
    if let (Some(path), Some(line)) = (path, diagnostic["line"].as_u64())
        && let Some(object) = result.as_object_mut()
    {
        object.insert(
            "locations".to_string(),
            json!([{
                "physicalLocation": {
                    "artifactLocation": {"uri": path},
                    "region": {
                        "startLine": line,
                        "startColumn": diagnostic["column"].as_u64().unwrap_or(1),
                    }
                }
            }]),
        );
    }
    annotate_result(config, diagnostic, &mut result);
    results.push(result);
}

fn add_failed_practice(
    config: &LensConfig,
    check: &Value,
    rules: &mut BTreeMap<String, Value>,
    results: &mut Vec<Value>,
) {
    if !matches!(
        check["status"].as_str(),
        Some("failed" | "unavailable" | "timed-out")
    ) {
        return;
    }
    let Some(rule_id) = check["rule_id"].as_str() else {
        return;
    };
    let title = check["title"].as_str().unwrap_or(rule_id);
    rules.entry(rule_id.to_string()).or_insert_with(|| {
        json!({
            "id": rule_id,
            "name": rule_id.replace(['.', '-'], "_"),
            "shortDescription": {"text": title},
            "helpUri": check["source"],
        })
    });
    let mut result = json!({
        "ruleId": rule_id,
        "level": crate::findings::finding_level(check),
        "message": {"text": format!("{}: {}", title, check["status"].as_str().unwrap_or("failed"))},
    });
    annotate_result(config, check, &mut result);
    results.push(result);
}

fn annotate_result(config: &LensConfig, finding: &Value, result: &mut Value) {
    if let Some(id) = finding["id"].as_str() {
        result["partialFingerprints"] = json!({"rqlens/source-anchor-v1": id});
    }
    let rule = finding["rule_id"].as_str().unwrap_or_default();
    if crate::findings::is_waivable(finding)
        && let Some(waiver) = config.policy.active_waiver(rule, finding["path"].as_str())
    {
        result["suppressions"] =
            json!([{"kind": "external", "status": "accepted", "justification": waiver.reason}]);
    }
    if let Some(locations) = finding.get("related_locations") {
        result["properties"]["related_locations"] = locations.clone();
    }
}

fn read_optional(config: &LensConfig, file: &str) -> Result<Option<Value>> {
    let path = config.output_dir.join(file);
    if !path.is_file() {
        return Ok(None);
    }
    let contents = fs::read_to_string(&path)
        .with_context(|| format!("reading SARIF input {}", path.display()))?;
    let mut document: Value = serde_json::from_str(&contents)
        .with_context(|| format!("parsing SARIF input {}", path.display()))?;
    crate::contracts::validate_artifact(file, &document)
        .with_context(|| format!("invalid SARIF input {file}"))?;
    ensure!(
        crate::fingerprint::freshness(&document, &config.input_fingerprint())
            == crate::fingerprint::Freshness::Current,
        "SARIF input {file} is stale or has incomplete provenance; rerun measurement"
    );
    crate::findings::normalize(config, &mut document);
    Ok(Some(document))
}

fn payload(document: &Value) -> &Value {
    document
        .get("records")
        .or_else(|| document.get("data"))
        .unwrap_or(document)
}

#[cfg(test)]
mod tests {
    use super::write;
    use anyhow::Context;
    use serde_json::{Map, Value, json};

    #[test]
    fn emits_findings_and_failed_gates_as_sarif() -> anyhow::Result<()> {
        let (root, config) = crate::test_support::source_project(Some("pub fn example() {}\n"))?;
        let source = root.path().join("src/lib.rs");
        let inputs: Map<String, Value> =
            serde_json::from_str(include_str!("../tests/fixtures/sarif-inputs.json"))?;
        for (file, payload) in inputs {
            let mut document = json!({"schema_version": 3, "risk_model_version": crate::measurement::MODEL_VERSION,
                "input_fingerprint": config.input_fingerprint(), "measurement_confidence": {"complete": true, "partial": false}});
            let Value::Object(fields) = payload else {
                anyhow::bail!("fixture must be an object");
            };
            document
                .as_object_mut()
                .context("header object")?
                .extend(fields);
            crate::util::write_json(&config.output_dir.join(file), &document)?;
        }
        let output = write(&config, None)?;
        let document: serde_json::Value = serde_json::from_slice(&std::fs::read(output)?)?;
        assert_eq!(document["version"], "2.1.0");
        assert_eq!(
            document["runs"][0]["results"].as_array().map(Vec::len),
            Some(3)
        );
        let Some(diagnostic) = document["runs"][0]["results"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|result| result["ruleId"] == "rust.compiler.dangling-pointers-from-locals")
        else {
            anyhow::bail!("compiler diagnostic result was not emitted");
        };
        assert_eq!(
            diagnostic["locations"][0]["physicalLocation"]["region"]["startLine"],
            8
        );
        std::fs::write(source, "pub fn changed() {}\n")?;
        assert!(
            write(&config, None).is_err(),
            "stale evidence must not be exported"
        );
        Ok(())
    }
}
