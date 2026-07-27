use anyhow::{Context, Result};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use crate::config::LensConfig;
use crate::util::write_json;

pub(crate) fn write(config: &LensConfig, output: Option<PathBuf>) -> Result<PathBuf> {
    let mut rules = BTreeMap::<String, Value>::new();
    let mut results = Vec::new();
    add_reliability_results(config, &mut rules, &mut results)?;
    add_practice_results(config, &mut rules, &mut results)?;
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
    let Some(document) = read_optional(config.output_dir.join("reliability_findings.json"))? else {
        return Ok(());
    };
    for finding in payload(&document).as_array().into_iter().flatten() {
        if finding["severity"] == "advisory" {
            continue;
        }
        let Some(rule_id) = finding["rule_id"].as_str() else {
            continue;
        };
        if config
            .policy
            .active_waiver(rule_id, finding["path"].as_str())
            .is_some()
        {
            continue;
        }
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
            json!(sarif_level(&finding["severity"])),
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
        results.push(Value::Object(result));
    }
    Ok(())
}

fn add_practice_results(
    config: &LensConfig,
    rules: &mut BTreeMap<String, Value>,
    results: &mut Vec<Value>,
) -> Result<()> {
    let Some(document) = read_optional(config.output_dir.join("rust_practices.json"))? else {
        return Ok(());
    };
    for check in payload(&document)["checks"]
        .as_array()
        .into_iter()
        .flatten()
    {
        if !matches!(
            check["status"].as_str(),
            Some("failed" | "unavailable" | "timed-out")
        ) {
            continue;
        }
        let Some(rule_id) = check["rule_id"].as_str() else {
            continue;
        };
        if config.policy.active_waiver(rule_id, None).is_some() {
            continue;
        }
        let title = check["title"].as_str().unwrap_or(rule_id);
        rules.entry(rule_id.to_string()).or_insert_with(|| {
            json!({
                "id": rule_id,
                "name": rule_id.replace(['.', '-'], "_"),
                "shortDescription": {"text": title},
                "helpUri": check["source"],
            })
        });
        results.push(json!({
            "ruleId": rule_id,
            "level": sarif_level(&check["severity"]),
            "message": {"text": format!("{}: {}", title, check["status"].as_str().unwrap_or("failed"))},
        }));
    }
    Ok(())
}

fn read_optional(path: PathBuf) -> Result<Option<Value>> {
    if !path.is_file() {
        return Ok(None);
    }
    let contents = fs::read_to_string(&path)
        .with_context(|| format!("reading SARIF input {}", path.display()))?;
    serde_json::from_str(&contents)
        .with_context(|| format!("parsing SARIF input {}", path.display()))
        .map(Some)
}

fn payload(document: &Value) -> &Value {
    document
        .get("records")
        .or_else(|| document.get("data"))
        .unwrap_or(document)
}

fn sarif_level(severity: &Value) -> &'static str {
    match severity.as_str() {
        Some("error") => "error",
        Some("warning") => "warning",
        _ => "note",
    }
}

#[cfg(test)]
mod tests {
    use super::write;
    use crate::config::{LensConfig, SemanticIdentityMode};
    use serde_json::json;
    use std::path::PathBuf;

    #[test]
    fn emits_findings_and_failed_gates_as_sarif() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let analysis = root.path().join("analysis");
        std::fs::create_dir_all(&analysis)?;
        std::fs::write(
            analysis.join("reliability_findings.json"),
            serde_json::to_vec(&json!({"records": [{
                "rule_id": "rust.safety.undocumented-unsafe",
                "severity": "error",
                "message": "missing rationale",
                "source": "https://example.invalid/rule",
                "path": "src/lib.rs",
                "line": 4
            }]}))?,
        )?;
        std::fs::write(
            analysis.join("rust_practices.json"),
            serde_json::to_vec(&json!({"data": {"checks": [{
                "rule_id": "rust.official.clippy",
                "title": "Clippy",
                "severity": "error",
                "status": "failed",
                "source": "https://doc.rust-lang.org/clippy/"
            }]}}))?,
        )?;
        let config = LensConfig {
            project_name: "sarif-test".to_string(),
            project_root: root.path().to_path_buf(),
            source_roots: vec![],
            output_dir: analysis,
            helper_manifest: PathBuf::from("unused"),
            identity_resolution: SemanticIdentityMode::Disabled,
            rust_analyzer: PathBuf::from("rust-analyzer"),
            identity_timeout_seconds: 1,
            identity_offline: true,
            verification: Default::default(),
            policy: Default::default(),
        };
        let output = write(&config, None)?;
        let document: serde_json::Value = serde_json::from_slice(&std::fs::read(output)?)?;
        assert_eq!(document["version"], "2.1.0");
        assert_eq!(
            document["runs"][0]["results"].as_array().map(Vec::len),
            Some(2)
        );
        Ok(())
    }
}
