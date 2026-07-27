use anyhow::Result;
use serde_json::{Value, json};
use std::time::Duration;

use crate::command_runner::{CommandRequest, CommandStatus, run as run_command};
use crate::config::LensConfig;
use crate::facts::{FileFacts, RunContext};
use crate::measurement::{module_for_path, project_relative_path};
use crate::util::round2;

pub(super) fn produce(config: &LensConfig, context: &RunContext) -> Result<Value> {
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
    let arguments = vec![
        "llvm-cov".to_string(),
        "--json".to_string(),
        "--output-path".to_string(),
        output_path.to_string_lossy().to_string(),
    ];
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

    let export: Value = serde_json::from_slice(&std::fs::read(output_path)?)?;
    let data = export["data"].as_array().and_then(|data| data.first());
    let files = data
        .and_then(|data| data["files"].as_array())
        .into_iter()
        .flatten()
        .filter_map(|file| coverage_file(config, file, &context.source_facts))
        .collect::<Vec<_>>();
    let totals = data
        .and_then(|data| data.get("totals"))
        .cloned()
        .unwrap_or_else(|| json!({}));
    Ok(json!({
        "summary": coverage_summary(&totals, files.len()),
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

fn coverage_file(config: &LensConfig, file: &Value, facts: &[FileFacts]) -> Option<Value> {
    let filename = file["filename"].as_str()?;
    if !filename.ends_with(".rs") {
        return None;
    }
    let path = project_relative_path(&config.project_root, filename);
    let identity = facts
        .iter()
        .find(|fact| project_relative_path(&config.project_root, &fact.path) == path);
    let summary = &file["summary"];
    Some(json!({
        "path": path,
        "module_key": module_for_path(&path),
        "module_id": identity.map(|fact| fact.module_id.as_str()),
        "package_name": identity.map(|fact| fact.package_name.as_str()),
        "target_name": identity.map(|fact| fact.target_name.as_str()),
        "lines": metric(&summary["lines"]),
        "regions": metric(&summary["regions"]),
        "functions": metric(&summary["functions"]),
        "branches": metric(&summary["branches"]),
    }))
}

fn metric(value: &Value) -> Value {
    let count = value["count"].as_u64().unwrap_or(0);
    let covered = value["covered"].as_u64().unwrap_or(0);
    let percent = value["percent"].as_f64().unwrap_or_else(|| {
        if count == 0 {
            100.0
        } else {
            covered as f64 / count as f64 * 100.0
        }
    });
    json!({"count": count, "covered": covered, "percent": round2(percent)})
}

fn coverage_summary(totals: &Value, file_count: usize) -> Value {
    json!({
        "file_count": file_count,
        "lines": metric(&totals["lines"]),
        "regions": metric(&totals["regions"]),
        "functions": metric(&totals["functions"]),
        "branches": metric(&totals["branches"]),
    })
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
        };
        let mut fact = FileFacts::test_fact("/project/src/domain.rs", "domain");
        fact.module_id = "demo::demo::domain".to_string();
        let file = coverage_file(
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
        );
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
        let summary = coverage_summary(
            &json!({"lines": {"count": 10, "covered": 8, "percent": 80.0}}),
            1,
        );
        assert_eq!(summary["file_count"], 1);
    }
}
