use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::LensConfig;
use crate::measurement::module_for_path;
use crate::util::{normalize_slashes, project_input_fingerprint, write_json};

pub(crate) fn collect(config: &LensConfig, labels: Option<PathBuf>) -> Result<PathBuf> {
    let output = Command::new("git")
        .args([
            "log",
            "--name-only",
            "--no-renames",
            "--format=commit:%H%x09%aI%x09%s",
            "--",
        ])
        .current_dir(&config.project_root)
        .output()
        .context("reading repository outcomes from git log")?;
    if !output.status.success() {
        bail!(
            "git log failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let mut records = parse_outcomes(&String::from_utf8_lossy(&output.stdout));
    if let Some(path) = labels {
        records.extend(read_labels(&path)?);
    }
    records.sort_by(|left, right| {
        left["commit"]
            .as_str()
            .cmp(&right["commit"].as_str())
            .then_with(|| left["kind"].as_str().cmp(&right["kind"].as_str()))
    });
    let inferred = records
        .iter()
        .filter(|record| record["evidence_class"] == "repository-inferred")
        .count();
    let labeled = records.len().saturating_sub(inferred);
    let document = json!({
        "schema_version": 1,
        "generated_from": "rqlens",
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "generator_version": env!("CARGO_PKG_VERSION"),
        "input_fingerprint": project_input_fingerprint(&config.project_root, &config.source_roots),
        "tool": "repository-outcomes",
        "measurement_confidence": {
            "complete": true,
            "partial": false,
            "confidence_scope": "repository_outcome_history",
            "required_inputs": ["git_history"],
            "observed_inputs": {"inferred_records": inferred, "reviewed_labels": labeled},
            "missing_input": [],
            "stale_input": [],
            "unsupported_pattern": [],
        },
        "summary": {
            "outcome_count": records.len(),
            "inferred_count": inferred,
            "reviewed_label_count": labeled,
            "defect_count": records.iter().filter(|record| record["kind"] == "defect").count(),
            "revert_count": records.iter().filter(|record| record["kind"] == "revert").count(),
            "security_count": records.iter().filter(|record| record["kind"] == "security").count(),
        },
        "data": {"records": records},
    });
    fs::create_dir_all(&config.output_dir)?;
    let path = config.output_dir.join("repository_outcomes.json");
    write_json(&path, &document)?;
    Ok(path)
}

fn parse_outcomes(text: &str) -> Vec<Value> {
    let mut records = Vec::new();
    let mut current = OutcomeCommit::default();
    for line in text.lines() {
        if line.starts_with("commit:") {
            current.flush(&mut records);
            current = OutcomeCommit::from_header(line);
        } else if !line.trim().is_empty() {
            current.paths.insert(normalize_slashes(line.trim()));
        }
    }
    current.flush(&mut records);
    records
}

#[derive(Default)]
struct OutcomeCommit {
    commit: String,
    timestamp: String,
    subject: String,
    paths: BTreeSet<String>,
}

impl OutcomeCommit {
    fn from_header(line: &str) -> Self {
        let values = line
            .trim_start_matches("commit:")
            .splitn(3, '\t')
            .collect::<Vec<_>>();
        Self {
            commit: values.first().copied().unwrap_or_default().to_string(),
            timestamp: values.get(1).copied().unwrap_or_default().to_string(),
            subject: values.get(2).copied().unwrap_or_default().to_string(),
            paths: BTreeSet::new(),
        }
    }

    fn flush(self, records: &mut Vec<Value>) {
        if self.commit.is_empty() {
            return;
        }
        let kinds = classify_subject(&self.subject);
        if kinds.is_empty() {
            return;
        }
        let modules = self
            .paths
            .iter()
            .filter(|path| path.ends_with(".rs"))
            .map(|path| module_for_path(path))
            .collect::<BTreeSet<_>>();
        for kind in kinds {
            records.push(json!({
                "commit": &self.commit,
                "timestamp": &self.timestamp,
                "kind": kind,
                "subject": &self.subject,
                "paths": &self.paths,
                "modules": &modules,
                "evidence_class": "repository-inferred",
                "review_required": true,
            }));
        }
    }
}

fn classify_subject(subject: &str) -> Vec<&'static str> {
    let lower = subject.to_ascii_lowercase();
    let words = lower
        .split(|character: char| !character.is_alphanumeric())
        .collect::<BTreeSet<_>>();
    let mut kinds = Vec::new();
    if words.iter().any(|word| {
        matches!(
            *word,
            "bug" | "bugfix" | "defect" | "fix" | "fixed" | "panic" | "crash" | "regression"
        )
    }) {
        kinds.push("defect");
    }
    if words.contains("revert") || words.contains("rollback") {
        kinds.push("revert");
    }
    if words.contains("security")
        || words.contains("vulnerability")
        || lower.contains("cve-")
        || lower.contains("rustsec-")
    {
        kinds.push("security");
    }
    kinds
}

fn read_labels(path: &Path) -> Result<Vec<Value>> {
    let value: Value = serde_json::from_str(
        &fs::read_to_string(path)
            .with_context(|| format!("reading outcome labels {}", path.display()))?,
    )?;
    let Some(labels) = value.as_array() else {
        bail!("outcome labels must be a JSON array");
    };
    labels
        .iter()
        .enumerate()
        .map(|(index, label)| normalize_label(label, index, path))
        .collect()
}

fn normalize_label(label: &Value, index: usize, path: &Path) -> Result<Value> {
    if !label.is_object() {
        bail!("outcome label at index {index} must be a JSON object");
    }
    let commit = required_label_string(label, "commit", index)?;
    let kind = required_label_string(label, "kind", index)?;
    let timestamp = optional_label_string(label, "timestamp", index)?;
    let subject = optional_label_string(label, "subject", index)?;
    let paths = label_string_array(label, "paths", index)?;
    let modules = label_string_array(label, "modules", index)?;
    let source = match optional_label_string(label, "source", index)? {
        Value::Null => json!(normalize_slashes(path)),
        source => source,
    };
    Ok(json!({
        "commit": commit,
        "timestamp": timestamp,
        "kind": kind,
        "subject": subject,
        "paths": paths,
        "modules": modules,
        "evidence_class": "reviewed-label",
        "review_required": false,
        "source": source,
    }))
}

fn required_label_string<'a>(label: &'a Value, field: &str, index: usize) -> Result<&'a str> {
    label[field]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("outcome label at index {index} requires non-empty {field}"))
}

fn optional_label_string(label: &Value, field: &str, index: usize) -> Result<Value> {
    match label.get(field) {
        None | Some(Value::Null) => Ok(Value::Null),
        Some(Value::String(value)) => Ok(Value::String(value.clone())),
        Some(_) => bail!("outcome label at index {index} field {field} must be a string"),
    }
}

fn label_string_array(label: &Value, field: &str, index: usize) -> Result<Vec<String>> {
    let Some(value) = label.get(field) else {
        return Ok(Vec::new());
    };
    let Some(items) = value.as_array() else {
        bail!("outcome label at index {index} field {field} must be an array of strings");
    };
    items
        .iter()
        .enumerate()
        .map(|(item_index, item)| {
            item.as_str()
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "outcome label at index {index} field {field}[{item_index}] must be a non-empty string"
                    )
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{classify_subject, normalize_label, parse_outcomes};
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn repository_outcomes_separate_reverts_defects_and_security() {
        assert_eq!(classify_subject("fix parser crash"), ["defect"]);
        assert_eq!(
            classify_subject("revert security regression"),
            ["defect", "revert", "security"]
        );
        assert!(classify_subject("refresh fixtures").is_empty());
        let records =
            parse_outcomes("commit:abc\t2026-01-01T00:00:00Z\tfix parser\n\nsrc/parser.rs\n");
        assert_eq!(records[0]["modules"][0], "parser");
    }

    #[test]
    fn reviewed_labels_are_validated_without_silent_drops() {
        let valid = normalize_label(
            &json!({
                "commit": "abc",
                "kind": "defect",
                "modules": ["app::lib::parser"]
            }),
            0,
            Path::new("labels.json"),
        );
        assert!(valid.is_ok());

        let missing_commit = normalize_label(
            &json!({"kind": "defect", "modules": ["parser"]}),
            2,
            Path::new("labels.json"),
        );
        assert!(missing_commit.is_err_and(|error| {
            error
                .to_string()
                .contains("index 2 requires non-empty commit")
        }));

        let malformed_modules = normalize_label(
            &json!({"commit": "abc", "kind": "defect", "modules": "parser"}),
            3,
            Path::new("labels.json"),
        );
        assert!(malformed_modules.is_err());
    }
}
