use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::{collections::BTreeMap, fs};

use crate::config::LensConfig;
use crate::util::write_json;

pub(crate) fn ingest(config: &LensConfig, adapter: &str, input: &Path) -> Result<PathBuf> {
    let document: Value = serde_json::from_str(
        &fs::read_to_string(input)
            .with_context(|| format!("reading external evidence {}", input.display()))?,
    )
    .with_context(|| format!("parsing external evidence {}", input.display()))?;
    validate(adapter, &document)?;
    let output = config.output_dir.join(format!("external_{adapter}.json"));
    let envelope = json!({
        "schema_version": 1,
        "generated_from": "rqlens-external-ingest",
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "adapter": adapter,
        "source_file": input,
        "data": document,
    });
    write_json(&output, &envelope)?;
    Ok(output)
}

fn validate(adapter: &str, document: &Value) -> Result<()> {
    ensure!(
        !adapter.is_empty()
            && adapter
                .chars()
                .all(|character| character.is_ascii_alphanumeric()
                    || ['-', '_'].contains(&character)),
        "external adapter must contain only ASCII letters, digits, '-' or '_'"
    );
    for field in ["name", "version"] {
        ensure!(
            document["tool"][field]
                .as_str()
                .is_some_and(|value| !value.trim().is_empty()),
            "external evidence requires nonempty tool.{field}"
        );
    }
    ensure!(
        ["completed", "failed", "partial", "unavailable"]
            .contains(&document["status"].as_str().unwrap_or_default()),
        "external evidence status must be completed, failed, partial, or unavailable"
    );
    for field in ["findings", "metrics"] {
        ensure!(
            document[field].is_array(),
            "external evidence {field} must be an array"
        );
    }
    ensure!(
        ["scope", "confidence"]
            .iter()
            .all(|field| document[*field].is_object()),
        "external evidence requires scope and confidence objects"
    );
    crate::contracts::validate_diagnostics(&document["findings"])
}

#[derive(Default)]
pub(crate) struct ExternalEvidence {
    pub(crate) findings: Vec<Value>,
    pub(crate) reasons: Vec<String>,
    pub(crate) documents: BTreeMap<String, Value>,
}

/// Imported reports enter policy/export only when their producer linked the current inputs.
pub(crate) fn current_findings(config: &LensConfig) -> ExternalEvidence {
    let mut evidence = ExternalEvidence::default();
    let mut paths = fs::read_dir(&config.output_dir)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("external_") && name.ends_with(".json"))
        })
        .collect::<Vec<_>>();
    paths.sort();
    if paths.is_empty() {
        evidence
            .reasons
            .push("No imported external reports are available".into());
    }
    for (rule, policy) in &config.policy.rules {
        if policy.level != crate::config::PolicyRuleLevel::Off
            && let Some(adapter) = rule
                .strip_prefix("external.")
                .and_then(|rule| rule.split('.').next())
            && !config
                .output_dir
                .join(format!("external_{adapter}.json"))
                .is_file()
        {
            evidence.reasons.push(format!(
                "Required external adapter {adapter} has no imported report"
            ));
        }
    }
    let current = config.input_fingerprint();
    for path in paths {
        match current_report(config, &path, &current) {
            Ok((rows, document)) => {
                evidence.findings.extend(rows);
                evidence.documents.insert(
                    document["adapter"].as_str().unwrap_or_default().to_string(),
                    document,
                );
            }
            Err(error) => evidence
                .reasons
                .push(format!("{}: {error}", path.display())),
        }
    }
    evidence
}

fn current_report(
    config: &LensConfig,
    path: &Path,
    current: &Value,
) -> Result<(Vec<Value>, Value)> {
    let envelope: Value = serde_json::from_slice(&fs::read(path)?)?;
    let adapter = envelope["adapter"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing adapter"))?;
    let document = &envelope["data"];
    validate(adapter, document)?;
    ensure!(
        envelope["schema_version"] == 1
            && path.file_name().and_then(|name| name.to_str())
                == Some(format!("external_{adapter}.json").as_str()),
        "external schema version or adapter/filename mismatch"
    );
    ensure!(
        document["status"] == "completed"
            && document["confidence"]["complete"] == true
            && crate::fingerprint::freshness(document, current)
                == crate::fingerprint::Freshness::Current,
        "external evidence is incomplete, stale or not source-linked"
    );
    let mut rows = document["findings"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|raw| external_finding(adapter, &document["tool"], raw))
        .collect::<Vec<_>>();
    crate::findings::normalize_records(config, &mut rows);
    crate::findings::policy_context(config, &mut rows);
    for (index, row) in rows.iter_mut().enumerate() {
        row["origin"] = json!({"adapter": adapter, "record_index": index});
    }
    Ok((rows, envelope))
}

fn external_finding(adapter: &str, producer: &Value, raw: &Value) -> Value {
    let rule = raw["rule_id"].as_str().unwrap_or_default();
    let mut finding = raw.clone();
    finding["rule_id"] = json!(format!("external.{adapter}.{rule}"));
    finding["producer"] = producer.clone();
    finding
}

#[cfg(test)]
mod tests {
    use super::validate;
    use serde_json::json;

    #[test]
    fn imported_findings_require_producer_source_identity() -> anyhow::Result<()> {
        let (root, config) = crate::test_support::source_project(Some("pub fn a() {}"))?;
        let report = root.path().join("report.json");
        let mut evidence = json!({"tool":{"name":"other","version":"1"},"status":"completed","scope":{},
            "confidence":{"complete":true},"metrics":[],"findings":[{"rule_id":"rule","message":"problem","severity":"warning","path":"src/lib.rs","line":1}]});
        std::fs::write(&report, serde_json::to_vec(&evidence)?)?;
        super::ingest(&config, "other", &report)?;
        assert!(super::current_findings(&config).findings.is_empty());
        evidence["input_fingerprint"] = config.input_fingerprint();
        std::fs::write(&report, serde_json::to_vec(&evidence)?)?;
        super::ingest(&config, "other", &report)?;
        let current = super::current_findings(&config);
        assert!(current.reasons.is_empty(), "{:?}", current.reasons);
        assert_eq!(current.findings[0]["rule_id"], "external.other.rule");
        std::fs::write(root.path().join("src/lib.rs"), "pub fn changed() {}")?;
        assert!(super::current_findings(&config).findings.is_empty());
        Ok(())
    }

    #[test]
    fn external_contract_requires_explicit_tool_status_and_confidence() {
        let valid = json!({
            "tool": {"name": "cargo-crap", "version": "1.0.0"},
            "status": "completed",
            "scope": {},
            "findings": [],
            "metrics": [],
            "confidence": {"complete": true}
        });
        assert!(validate("cargo-crap", &valid).is_ok());
        assert!(validate("cargo-crap", &json!({})).is_err());
    }
}
