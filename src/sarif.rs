use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::BTreeMap;
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
    for (file, key) in [
        ("reliability_findings.json", None),
        ("architecture_rules.json", Some("violations")),
        ("test_quality.json", Some("findings")),
    ] {
        add_finding_document(config, file, key, &mut rules, &mut results)?;
    }
    let practice_evidence = add_practice_results(config, &mut rules, &mut results)?;
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
            "properties": {"external_evidence_reasons": external_reasons, "practice_evidence": practice_evidence},
        }]
    });
    let output = output.unwrap_or_else(|| config.output_dir.join("rqlens.sarif"));
    write_json(&output, &document)?;
    Ok(output)
}

fn add_finding_document(
    config: &LensConfig,
    file_name: &str,
    payload_key: Option<&str>,
    rules: &mut BTreeMap<String, Value>,
    results: &mut Vec<Value>,
) -> Result<()> {
    let Some(document) = read_optional(config, file_name)? else {
        return Ok(());
    };
    let payload = payload(&document);
    let findings = payload_key.map_or(payload, |key| &payload[key]);
    add_source_findings(
        config,
        findings.as_array().into_iter().flatten(),
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
            let mut rule = json!({
                "id": rule_id,
                "name": rule_id.replace(['.', '-'], "_"),
                "shortDescription": {"text": message},
            });
            if let Some(source) = finding["source"].as_str() {
                rule["helpUri"] = json!(source);
            }
            rule
        });
        let mut result = json!({"ruleId": rule_id, "level": crate::findings::finding_level(finding), "message": {"text": message}});
        if let Some(path) = finding["path"].as_str().filter(|path| !path.is_empty()) {
            result["locations"] = json!([{"physicalLocation": physical_location(path, finding)}]);
        }
        annotate_result(config, finding, &mut result);
        results.push(result);
    }
}

fn physical_location(path: &str, finding: &Value) -> Value {
    let mut location = json!({"artifactLocation": {"uri": path.replace('\\', "/")}});
    if let Some(line) = finding["line"].as_u64().filter(|line| *line > 0) {
        location["region"] = json!({"startLine": line});
        if let Some(column) = finding["column"].as_u64().filter(|column| *column > 0) {
            location["region"]["startColumn"] = json!(column);
        }
    }
    location
}

fn add_practice_results(
    config: &LensConfig,
    rules: &mut BTreeMap<String, Value>,
    results: &mut Vec<Value>,
) -> Result<Value> {
    let document = read_optional(config, "rust_practices.json")?;
    let projection = crate::findings::practices::project(config, document.as_ref());
    add_source_findings(config, projection.findings.iter(), rules, results);
    Ok(projection.evidence)
}

fn annotate_result(config: &LensConfig, finding: &Value, result: &mut Value) {
    result["properties"]["rqlens_finding"] = finding.clone();
    if let Some(id) = finding["id"].as_str() {
        let version = finding["identity_version"]
            .as_str()
            .unwrap_or("source-anchor-v1");
        result["partialFingerprints"] = json!({format!("rqlens/{version}"): id});
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
    if file != "rust_practices.json" {
        crate::findings::normalize(config, &mut document);
    }
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
            (
                diagnostic["locations"][0]["physicalLocation"]["region"].clone(),
                diagnostic["properties"]["rqlens_finding"]["observations"]
                    .as_array()
                    .map(Vec::len)
            ),
            (json!({"startLine": 8, "startColumn": 4}), Some(2))
        );
        std::fs::write(source, "pub fn changed() {}\n")?;
        assert!(
            write(&config, None).is_err(),
            "stale evidence must not be exported"
        );
        Ok(())
    }
}
