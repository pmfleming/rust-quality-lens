use anyhow::Result;
use serde_json::{Value, json};
use std::time::Duration;

use crate::command_runner::{CommandRequest, CommandStatus, run as run_command};
use crate::config::LensConfig;
use crate::facts::{FileFacts, RunContext};
use crate::measurement::{module_for_path, project_relative_path};
use crate::util::round2;

pub(super) fn produce(config: &LensConfig, context: &RunContext) -> Result<Value> {
    // cargo-llvm-cov holds Cargo's build lock while tests execute. Starting it
    // recursively from a test would deadlock waiting for that same build.
    if std::env::var_os("CARGO_LLVM_COV").is_some() {
        return Ok(unavailable(
            "nested cargo-llvm-cov execution is unsupported",
            "unsupported_pattern",
        ));
    }
    if let Err(reason) = require_coverage_tool(config) {
        return Ok(unavailable(&reason, "missing_input"));
    }

    let temp = tempfile::tempdir()?;
    let result = collect_coverage(config, context, &temp.path().join("coverage.json"));
    Ok(result.unwrap_or_else(|reason| unavailable(&reason, "unsupported_pattern")))
}

fn require_coverage_tool(config: &LensConfig) -> std::result::Result<(), String> {
    let arguments = ["llvm-cov", "--version"].map(str::to_string);
    let mut request = CommandRequest::new("cargo", &arguments, &config.project_root);
    request.timeout = Duration::from_secs(30);
    let outcome = run_command(request);
    if outcome.status == CommandStatus::Passed {
        Ok(())
    } else {
        Err(outcome
            .reason
            .unwrap_or_else(|| "cargo-llvm-cov is not installed".to_string()))
    }
}

fn collect_coverage(
    config: &LensConfig,
    context: &RunContext,
    output_path: &std::path::Path,
) -> std::result::Result<Value, String> {
    execute_coverage(config, output_path)?;
    let data = read_coverage_data(output_path)?;
    let files = coverage_files(config, context, &data)?;
    let totals = data
        .get("totals")
        .ok_or_else(|| "cargo llvm-cov output did not contain aggregate totals".to_string())?;
    let summary = coverage_summary(totals, files.len())?;
    Ok(complete_coverage(summary, files))
}

fn execute_coverage(
    config: &LensConfig,
    output_path: &std::path::Path,
) -> std::result::Result<(), String> {
    let mut arguments = vec![
        "llvm-cov".to_string(),
        "--json".to_string(),
        "--output-path".to_string(),
        output_path.to_string_lossy().to_string(),
    ];
    arguments.extend(config.verification.cargo_scope_arguments(true));
    let mut request = CommandRequest::new("cargo", &arguments, &config.project_root);
    request.timeout = Duration::from_secs(config.verification.timeout_seconds);
    let outcome = run_command(request);
    if outcome.status == CommandStatus::Passed {
        Ok(())
    } else {
        Err(format!(
            "cargo llvm-cov failed: {} {}",
            outcome.stdout_tail, outcome.stderr_tail
        ))
    }
}

fn read_coverage_data(output_path: &std::path::Path) -> std::result::Result<Value, String> {
    let bytes = std::fs::read(output_path)
        .map_err(|error| format!("cargo llvm-cov did not write valid output: {error}"))?;
    let export: Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("cargo llvm-cov output was not valid JSON: {error}"))?;
    export["data"]
        .as_array()
        .and_then(|data| data.first())
        .cloned()
        .ok_or_else(|| "cargo llvm-cov output did not contain a coverage data set".to_string())
}

fn coverage_files(
    config: &LensConfig,
    context: &RunContext,
    data: &Value,
) -> std::result::Result<Vec<Value>, String> {
    let export_files = data["files"]
        .as_array()
        .ok_or_else(|| "cargo llvm-cov output did not contain file coverage".to_string())?;
    let files = export_files
        .iter()
        .map(|file| coverage_file(config, file, &context.source_facts))
        .collect::<std::result::Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    if files.is_empty() && !context.source_facts.is_empty() {
        Err("cargo llvm-cov reported no Rust files inside the configured project".to_string())
    } else {
        Ok(files)
    }
}

fn complete_coverage(summary: Value, files: Vec<Value>) -> Value {
    json!({
        "summary": summary,
        "measurement_confidence": {
            "complete": true,
            "partial": false,
            "confidence_scope": "dynamic_coverage",
            "required_inputs": ["cargo_llvm_cov", "successful_test_run"],
            "observed_inputs": {"cargo_llvm_cov": true, "covered_rust_files": files.len()},
            "missing_input": [],
            "stale_input": [],
            "unsupported_pattern": [],
        },
        "files": files,
    })
}

fn coverage_file(
    config: &LensConfig,
    file: &Value,
    facts: &[FileFacts],
) -> std::result::Result<Option<Value>, String> {
    let filename = file["filename"]
        .as_str()
        .ok_or_else(|| "cargo llvm-cov emitted a file without a filename".to_string())?;
    if !filename.ends_with(".rs") {
        return Ok(None);
    }
    let absolute = if std::path::Path::new(filename).is_absolute() {
        std::path::PathBuf::from(filename)
    } else {
        config.project_root.join(filename)
    };
    if !absolute.starts_with(&config.project_root) {
        return Ok(None);
    }
    let path = project_relative_path(&config.project_root, filename);
    let identity = facts
        .iter()
        .find(|fact| project_relative_path(&config.project_root, &fact.path) == path);
    let summary = &file["summary"];
    let line_hits = line_hits(&file["segments"])?;
    Ok(Some(json!({
        "path": path,
        "module_key": module_for_path(&path),
        "module_id": identity.map(|fact| fact.module_id.as_str()),
        "package_name": identity.map(|fact| fact.package_name.as_str()),
        "target_name": identity.map(|fact| fact.target_name.as_str()),
        "lines": metric(&summary["lines"], "lines")?,
        "regions": metric(&summary["regions"], "regions")?,
        "functions": metric(&summary["functions"], "functions")?,
        "branches": metric(&summary["branches"], "branches")?,
        "line_hits": line_hits,
    })))
}

fn line_hits(segments: &Value) -> std::result::Result<Value, String> {
    let segments = segments
        .as_array()
        .ok_or_else(|| "cargo llvm-cov omitted file coverage segments".to_string())?;
    let mut lines = std::collections::BTreeMap::<u64, bool>::new();
    for segment in segments {
        let Some(values) = segment.as_array() else {
            continue;
        };
        let (Some(line), Some(count), Some(has_count)) = (
            values.first().and_then(Value::as_u64),
            values.get(2).and_then(Value::as_u64),
            values.get(3).and_then(Value::as_bool),
        ) else {
            continue;
        };
        if has_count {
            lines
                .entry(line)
                .and_modify(|covered| *covered |= count > 0)
                .or_insert(count > 0);
        }
    }
    Ok(Value::Array(
        lines
            .into_iter()
            .map(|(line, covered)| json!({"line": line, "covered": covered}))
            .collect(),
    ))
}

fn metric(value: &Value, name: &str) -> std::result::Result<Value, String> {
    let count = value["count"]
        .as_u64()
        .ok_or_else(|| format!("cargo llvm-cov omitted the {name} count"))?;
    let covered = value["covered"]
        .as_u64()
        .ok_or_else(|| format!("cargo llvm-cov omitted the covered {name} count"))?;
    if covered > count {
        return Err(format!(
            "cargo llvm-cov reported {covered} covered {name} out of {count}"
        ));
    }
    let branch_data_available = name != "branches" || count > 0;
    let percent = if !branch_data_available {
        Value::Null
    } else if count == 0 {
        Value::from(100.0)
    } else {
        Value::from(round2(covered as f64 / count as f64 * 100.0))
    };
    Ok(json!({
        "count": count,
        "covered": covered,
        "percent": percent,
        "available": branch_data_available,
    }))
}

fn coverage_summary(totals: &Value, file_count: usize) -> std::result::Result<Value, String> {
    Ok(json!({
        "file_count": file_count,
        "lines": metric(&totals["lines"], "lines")?,
        "regions": metric(&totals["regions"], "regions")?,
        "functions": metric(&totals["functions"], "functions")?,
        "branches": metric(&totals["branches"], "branches")?,
    }))
}

fn unavailable(reason: &str, field: &str) -> Value {
    let mut missing = Vec::new();
    let mut unsupported = Vec::new();
    if field == "missing_input" {
        missing.push(reason.to_string());
    } else {
        unsupported.push(reason.to_string());
    }
    json!({
        "summary": {"file_count": 0, "status": "unavailable"},
        "files": [],
        "measurement_confidence": {
            "complete": false,
            "partial": true,
            "confidence_scope": "dynamic_coverage",
            "required_inputs": ["cargo_llvm_cov", "successful_test_run"],
            "observed_inputs": {"cargo_llvm_cov": field != "missing_input", "covered_rust_files": 0},
            "missing_input": missing,
            "stale_input": [],
            "unsupported_pattern": unsupported,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{coverage_file, coverage_summary};
    use crate::config::{LensConfig, SemanticIdentityMode};
    use crate::facts::FileFacts;
    use serde_json::json;
    use std::path::PathBuf;

    #[test]
    fn parses_llvm_coverage_metrics() {
        let config = LensConfig {
            project_name: "demo".to_string(),
            project_root: PathBuf::from("/project"),
            source_roots: vec!["/project/src".to_string()],
            output_dir: PathBuf::from("/project/target/analysis"),
            helper_manifest: PathBuf::from("unused"),
            identity_resolution: SemanticIdentityMode::Disabled,
            rust_analyzer: PathBuf::from("rust-analyzer"),
            identity_timeout_seconds: 1,
            identity_offline: true,
            verification: Default::default(),
            policy: Default::default(),
        };
        let mut fact = FileFacts::test_fact("/project/src/domain.rs", "domain");
        fact.module_id = "demo::demo::domain".to_string();
        let Ok(file) = coverage_file(
            &config,
            &json!({
                "filename": "/project/src/domain.rs",
                "segments": [[1, 1, 1, true, true, false], [2, 1, 0, true, true, false]],
                "summary": {
                    "lines": {"count": 10, "covered": 8, "percent": 80.0},
                    "regions": {"count": 12, "covered": 9, "percent": 75.0},
                    "functions": {"count": 2, "covered": 2, "percent": 100.0},
                    "branches": {"count": 4, "covered": 2, "percent": 50.0}
                }
            }),
            &[fact],
        ) else {
            panic!("coverage row should be valid");
        };
        assert_eq!(
            file.as_ref().map(|row| &row["module_key"]),
            Some(&json!("domain"))
        );
        assert_eq!(
            file.as_ref().map(|row| &row["lines"]["percent"]),
            Some(&json!(80.0))
        );
        assert_eq!(
            file.as_ref().map(|row| &row["module_id"]),
            Some(&json!("demo::demo::domain"))
        );
        assert_eq!(
            file.as_ref().map(|row| &row["line_hits"]),
            Some(&json!([
                {"line": 1, "covered": true},
                {"line": 2, "covered": false}
            ]))
        );
        let Ok(summary) = coverage_summary(
            &json!({
                "lines": {"count": 10, "covered": 8},
                "regions": {"count": 12, "covered": 9},
                "functions": {"count": 2, "covered": 2},
                "branches": {"count": 4, "covered": 2}
            }),
            1,
        ) else {
            panic!("coverage summary should be valid");
        };
        assert_eq!(summary["file_count"], 1);
        let empty_branch_summary = coverage_summary(
            &json!({
                "lines": {"count": 10, "covered": 8},
                "regions": {"count": 12, "covered": 9},
                "functions": {"count": 2, "covered": 2},
                "branches": {"count": 0, "covered": 0}
            }),
            1,
        );
        assert_eq!(
            empty_branch_summary
                .ok()
                .map(|summary| summary["branches"].clone()),
            Some(json!({
                "available": false,
                "count": 0,
                "covered": 0,
                "percent": null
            }))
        );
    }
}
