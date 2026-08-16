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
