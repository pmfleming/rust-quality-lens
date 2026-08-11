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

    let version_args = ["llvm-cov", "--version"].map(str::to_string);
    let mut version_request = CommandRequest::new("cargo", &version_args, &config.project_root);
    version_request.timeout = Duration::from_secs(30);
    let version = run_command(version_request);
    if version.status != CommandStatus::Passed {
        return Ok(unavailable(
            version
                .reason
                .as_deref()
                .unwrap_or("cargo-llvm-cov is not installed"),
            "missing_input",
        ));
    }

    let temp = tempfile::tempdir()?;
    let output_path = temp.path().join("coverage.json");
    let mut arguments = vec![
        "llvm-cov".to_string(),
        "--json".to_string(),
        "--output-path".to_string(),
        output_path.to_string_lossy().to_string(),
    ];
    arguments.extend(config.verification.cargo_scope_arguments(true));
    let mut request = CommandRequest::new("cargo", &arguments, &config.project_root);
    request.timeout = Duration::from_secs(config.verification.timeout_seconds);
    let output = run_command(request);
    if output.status != CommandStatus::Passed {
        let detail = format!(
            "cargo llvm-cov failed: {} {}",
            output.stdout_tail, output.stderr_tail
        );
        return Ok(unavailable(&detail, "unsupported_pattern"));
    }

    let bytes = match std::fs::read(&output_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return Ok(unavailable(
                &format!("cargo llvm-cov did not write valid output: {error}"),
                "unsupported_pattern",
            ));
        }
    };
    let export: Value = match serde_json::from_slice(&bytes) {
        Ok(export) => export,
        Err(error) => {
            return Ok(unavailable(
                &format!("cargo llvm-cov output was not valid JSON: {error}"),
                "unsupported_pattern",
            ));
        }
    };
    let Some(data) = export["data"].as_array().and_then(|data| data.first()) else {
        return Ok(unavailable(
            "cargo llvm-cov output did not contain a coverage data set",
            "unsupported_pattern",
        ));
    };
    let Some(export_files) = data["files"].as_array() else {
        return Ok(unavailable(
            "cargo llvm-cov output did not contain file coverage",
            "unsupported_pattern",
        ));
    };
    let mut files = Vec::new();
    for file in export_files {
        match coverage_file(config, file, &context.source_facts) {
            Ok(Some(row)) => files.push(row),
            Ok(None) => {}
            Err(reason) => return Ok(unavailable(&reason, "unsupported_pattern")),
        }
    }
    if files.is_empty() && !context.source_facts.is_empty() {
        return Ok(unavailable(
            "cargo llvm-cov reported no Rust files inside the configured project",
            "unsupported_pattern",
        ));
    }
    let Some(totals) = data.get("totals") else {
        return Ok(unavailable(
            "cargo llvm-cov output did not contain aggregate totals",
            "unsupported_pattern",
        ));
    };
    let summary = match coverage_summary(totals, files.len()) {
        Ok(summary) => summary,
        Err(reason) => return Ok(unavailable(&reason, "unsupported_pattern")),
    };
    Ok(json!({
        "summary": summary,
        "files": files,
        "measurement_confidence": {
            "complete": true,
            "partial": false,
            "confidence_scope": "dynamic_coverage",
            "required_inputs": ["cargo_llvm_cov", "successful_test_run"],
            "observed_inputs": {"cargo_llvm_cov": true, "covered_rust_files": files.len()},
            "missing_input": [],
            "stale_input": [],
            "unsupported_pattern": [],
        }
    }))
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
    })))
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
