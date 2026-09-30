use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::LensConfig;
use crate::util::write_json;

pub(crate) fn ingest(config: &LensConfig, adapter: &str, input: &Path) -> Result<PathBuf> {
    let document: Value = serde_json::from_str(
        &fs::read_to_string(input)
            .with_context(|| format!("reading external evidence {}", input.display()))?,
    )
    .with_context(|| format!("parsing external evidence {}", input.display()))?;
    validate(adapter, &document)?;
    let output = config
        .output_dir
        .join(format!("external_{}.json", safe_name(adapter)));
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
    if adapter.trim().is_empty() {
        bail!("external evidence adapter must not be empty");
    }
    let tool_name = document["tool"]["name"].as_str().unwrap_or_default();
    if tool_name.is_empty() || document["tool"]["version"].as_str().is_none() {
        bail!("external evidence requires tool.name and tool.version");
    }
    if !matches!(
        document["status"].as_str(),
        Some("completed" | "failed" | "partial" | "unavailable")
    ) {
        bail!("external evidence status must be completed, failed, partial, or unavailable");
    }
    for field in ["findings", "metrics"] {
        if !document[field].is_array() {
            bail!("external evidence {field} must be an array");
        }
    }
    if !document["scope"].is_object() || !document["confidence"].is_object() {
        bail!("external evidence requires scope and confidence objects");
    }
    Ok(())
}

/// Imported reports enter policy/export only when their producer linked the current inputs.
pub(crate) fn current_findings(config: &LensConfig) -> (Vec<Value>, Vec<String>) {
    let mut findings = Vec::new();
    let mut reasons = Vec::new();
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
        reasons.push("No imported external reports are available".into());
    }
    for (rule, policy) in &config.policy.rules {
        if policy.level == crate::config::PolicyRuleLevel::Off {
            continue;
        }
        if let Some(adapter) = rule
            .strip_prefix("external.")
            .and_then(|rule| rule.split('.').next())
            && !config
                .output_dir
                .join(format!("external_{}.json", safe_name(adapter)))
                .is_file()
        {
            reasons.push(format!(
                "Required external adapter {adapter} has no imported report"
            ));
        }
    }
    let current = config.input_fingerprint();
    for path in paths {
        let result = (|| -> Result<Vec<Value>> {
            let envelope: Value = serde_json::from_slice(&fs::read(&path)?)?;
            let adapter = envelope["adapter"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("missing adapter"))?;
            let document = &envelope["data"];
            validate(adapter, document)?;
            anyhow::ensure!(
                document["status"] == "completed"
                    && document["confidence"]["complete"] == true
                    && crate::fingerprint::freshness(document, &current)
                        == crate::fingerprint::Freshness::Current,
                "external evidence is incomplete, stale or not source-linked"
            );
            let mut rows = Vec::new();
            for raw in document["findings"].as_array().into_iter().flatten() {
                let rule = raw["rule_id"]
                    .as_str()
                    .filter(|rule| !rule.is_empty())
                    .ok_or_else(|| anyhow::anyhow!("missing rule ID"))?;
                anyhow::ensure!(
                    raw["message"].is_string()
                        && matches!(
                            raw["severity"].as_str(),
                            Some("error" | "warning" | "advisory")
                        ),
                    "invalid external finding"
                );
                let mut finding = raw.clone();
                finding["rule_id"] = json!(format!("external.{}.{}", safe_name(adapter), rule));
                finding["producer"] = document["tool"].clone();
                rows.push(finding);
            }
            let mut rows = Value::Array(rows);
            crate::findings::normalize(config, &mut rows);
            Ok(rows.as_array().cloned().unwrap_or_default())
        })();
        match result {
            Ok(rows) => findings.extend(rows),
            Err(error) => reasons.push(format!("{}: {error}", path.display())),
        }
    }
    (findings, reasons)
}

fn safe_name(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::validate;
    use serde_json::json;

    #[test]
    fn imported_findings_require_producer_source_identity() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        std::fs::create_dir(root.path().join("src"))?;
        std::fs::write(root.path().join("src/lib.rs"), "pub fn a() {}")?;
        std::fs::write(root.path().join("rqlens.toml"), "source_roots = ['src']\n")?;
        let config = crate::config::LensConfig::load(Some(root.path().join("rqlens.toml")))?;
        let report = root.path().join("report.json");
        let mut evidence = json!({"tool":{"name":"other","version":"1"},"status":"completed","scope":{},
            "confidence":{"complete":true},"metrics":[],"findings":[{"rule_id":"rule","message":"problem","severity":"warning","path":"src/lib.rs","line":1}]});
        std::fs::write(&report, serde_json::to_vec(&evidence)?)?;
        super::ingest(&config, "other", &report)?;
        assert!(super::current_findings(&config).0.is_empty());
        evidence["input_fingerprint"] = config.input_fingerprint();
        std::fs::write(&report, serde_json::to_vec(&evidence)?)?;
        super::ingest(&config, "other", &report)?;
        let (findings, reasons) = super::current_findings(&config);
        assert!(reasons.is_empty(), "{reasons:?}");
        assert_eq!(findings[0]["rule_id"], "external.other.rule");
        std::fs::write(root.path().join("src/lib.rs"), "pub fn changed() {}")?;
        assert!(super::current_findings(&config).0.is_empty());
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
