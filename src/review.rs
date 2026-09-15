use crate::config::LensConfig;
use crate::contracts::{ReviewEntrypoint, ReviewMeasurement, ReviewOutput, ReviewScope};
use crate::facts::FileFacts;
use crate::facts::RunContext;
use crate::producers::produce_measurement;
use crate::tool::MeasureTool;
use crate::util::{absolutize, normalize_slashes, write_json};
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

mod diff;
#[cfg(test)]
mod tests;

const REVIEW_TOOLS: &[MeasureTool] = &[
    MeasureTool::Hotspots,
    MeasureTool::FunctionRisk,
    MeasureTool::Clones,
    MeasureTool::EscapeHatches,
    MeasureTool::Reliability,
    MeasureTool::ArchitectureRules,
    MeasureTool::TestQuality,
    MeasureTool::ApiHealth,
    MeasureTool::TypeHealth,
];

pub(crate) fn run_review(
    config: LensConfig,
    changed_since: Option<String>,
    diff_file: Option<PathBuf>,
) -> Result<PathBuf> {
    fs::create_dir_all(&config.output_dir)?;
    let diff = review_diff(&config.project_root, changed_since.as_deref(), &diff_file)?;
    let scope = diff::parse(&diff)?;
    let changed_files = scope
        .paths
        .iter()
        .map(|path| absolutize(config.project_root.join(path)))
        .collect::<Vec<_>>();
    let changed_lines = scope.additions;
    let measured_files = changed_files
        .iter()
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .filter(|path| path.exists())
        .cloned()
        .collect::<Vec<_>>();

    let (measurements, entrypoints) = if measured_files.is_empty() {
        (Vec::new(), Vec::new())
    } else {
        let measured_file_keys: HashSet<String> = measured_files.iter().map(path_key).collect();
        let tools = REVIEW_TOOLS.to_vec();
        let context = RunContext::new(&config, &tools)?;
        let measurements = tools
            .iter()
            .map(|tool| {
                produce_measurement(tool, &config, &context).map(|payload| ReviewMeasurement {
                    tool: tool.name().to_string(),
                    output_file: tool.output_file().to_string(),
                    payload: filter_payload(&config.project_root, payload, &measured_file_keys),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        (
            measurements,
            review_entrypoints(
                &config.project_root,
                &context.source_facts,
                &context.correctness_facts,
                &measured_file_keys,
            ),
        )
    };

    let change_evidence = changed_line_coverage(&config, &changed_lines);
    let output = ReviewOutput {
        version: 2,
        generated_from: "rqlens",
        scope: ReviewScope {
            changed_since,
            diff_file: diff_file.map(normalize_slashes),
            changed_files: relative_paths(&config.project_root, &changed_files),
            changed_lines: serde_json::to_value(&changed_lines)?,
            measured_rust_files: relative_paths(&config.project_root, &measured_files),
            entrypoints,
        },
        change_evidence,
        measurements,
    };
    let output_path = config.output_dir.join("review.json");
    write_json(&output_path, &serde_json::to_value(output)?)?;
    Ok(output_path)
}

fn review_entrypoints(
    project_root: &Path,
    source_facts: &[FileFacts],
    correctness_facts: &[FileFacts],
    measured_files: &HashSet<String>,
) -> Vec<ReviewEntrypoint> {
    let mut seen = HashSet::new();
    source_facts
        .iter()
        .chain(correctness_facts)
        .filter(|fact| fact.is_entrypoint)
        .filter(|fact| measured_files.contains(&payload_path_key(project_root, &fact.path)))
        .filter(|fact| seen.insert(payload_path_key(project_root, &fact.path)))
        .filter_map(|fact| {
            Some(ReviewEntrypoint {
                path: relative_path(project_root, &fact.path),
                module_key: fact.module_key.clone(),
                target_kind: fact.target_kind.clone(),
                entrypoint_kind: fact.entrypoint_kind.clone()?,
            })
        })
        .collect()
}

fn review_diff(
    project_root: &Path,
    changed_since: Option<&str>,
    diff_file: &Option<PathBuf>,
) -> Result<String> {
    if let Some(diff_file) = diff_file {
        return fs::read_to_string(diff_file)
            .with_context(|| format!("reading diff file {}", diff_file.display()));
    }
    git_diff(project_root, changed_since.unwrap_or("HEAD"))
}

fn git_diff(project_root: &Path, base: &str) -> Result<String> {
    let output = crate::command_runner::checked_output(
        Command::new("git")
            .args([
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--relative",
                "--src-prefix=a/",
                "--dst-prefix=b/",
                "--unified=0",
                base,
                "--",
            ])
            .current_dir(project_root),
        std::time::Duration::from_secs(30),
    )
    .with_context(|| format!("running git diff from {}", project_root.display()))?;
    Ok(output.stdout)
}

fn changed_line_coverage(
    config: &LensConfig,
    ranges: &std::collections::BTreeMap<String, Vec<[u64; 2]>>,
) -> serde_json::Value {
    let path = config.output_dir.join("coverage.json");
    let Ok(text) = fs::read_to_string(&path) else {
        return serde_json::json!({"status": "unavailable", "reason": "coverage.json is missing"});
    };
    let Ok(document) = serde_json::from_str::<serde_json::Value>(&text) else {
        return serde_json::json!({"status": "unavailable", "reason": "coverage.json is invalid"});
    };
    if let Err(error) = crate::contracts::validate_artifact("coverage.json", &document) {
        return serde_json::json!({"status": "unavailable", "reason": error.to_string()});
    }
    let current = config.input_fingerprint();
    if crate::fingerprint::freshness(&document, &current) != crate::fingerprint::Freshness::Current
    {
        return serde_json::json!({"status": "stale", "reason": "coverage inputs do not match the current source"});
    }
    if document["measurement_confidence"]["complete"] != true {
        return serde_json::json!({"status": "partial", "reason": "coverage measurement is incomplete"});
    }
    let data = &document["data"];
    let mut executable = 0u64;
    let mut covered = 0u64;
    for file in data["files"].as_array().into_iter().flatten() {
        let Some(file_ranges) = file["path"].as_str().and_then(|path| ranges.get(path)) else {
            continue;
        };
        for hit in file["line_hits"].as_array().into_iter().flatten() {
            let Some(line) = hit["line"].as_u64() else {
                continue;
            };
            if file_ranges
                .iter()
                .any(|range| line >= range[0] && line <= range[1])
            {
                executable += 1;
                covered += u64::from(hit["covered"] == true);
            }
        }
    }
    serde_json::json!({
        "status": "observed",
        "changed_executable_lines": executable,
        "changed_covered_lines": covered,
        "changed_line_coverage_percent": if executable == 0 { serde_json::Value::Null } else { serde_json::Value::from(crate::util::round2(covered as f64 / executable as f64 * 100.0)) },
        "source": "cargo_llvm_cov_changed_lines",
    })
}

fn relative_paths(project_root: &Path, paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|path| relative_path(project_root, path))
        .collect()
}

fn relative_path(project_root: &Path, path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    path.strip_prefix(project_root)
        .map(normalize_slashes)
        .unwrap_or_else(|_| normalize_slashes(path))
}

fn filter_payload(
    project_root: &Path,
    payload: serde_json::Value,
    measured_files: &HashSet<String>,
) -> serde_json::Value {
    match payload {
        serde_json::Value::Array(rows) => serde_json::Value::Array(
            rows.into_iter()
                .filter(|row| row_matches_changed_file(project_root, row, measured_files))
                .collect(),
        ),
        serde_json::Value::Object(mut object)
            if object.contains_key("violations") || object.contains_key("functions") =>
        {
            let key = if object.contains_key("violations") {
                "violations"
            } else {
                "functions"
            };
            if let Some(serde_json::Value::Array(rows)) = object.remove(key) {
                let rows = rows
                    .into_iter()
                    .filter(|row| row_matches_changed_file(project_root, row, measured_files))
                    .collect::<Vec<_>>();
                object.insert(key.to_string(), serde_json::Value::Array(rows));
            }
            serde_json::Value::Object(object)
        }
        payload => payload,
    }
}

fn row_matches_changed_file(
    project_root: &Path,
    row: &serde_json::Value,
    measured_files: &HashSet<String>,
) -> bool {
    for key in ["path", "name", "file_path"] {
        if row
            .get(key)
            .and_then(serde_json::Value::as_str)
            .is_some_and(|path| measured_files.contains(&payload_path_key(project_root, path)))
        {
            return true;
        }
    }
    row.get("instances")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|instances| {
            instances.iter().any(|instance| {
                instance
                    .get("file_path")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|path| {
                        measured_files.contains(&payload_path_key(project_root, path))
                    })
            })
        })
}

fn payload_path_key(project_root: &Path, path: &str) -> String {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path_key(path)
    } else {
        path_key(project_root.join(path))
    }
}

fn path_key(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    let absolute = absolutize(path);
    let canonical = fs::canonicalize(&absolute).unwrap_or(absolute);
    let key = normalize_slashes(canonical);
    if cfg!(windows) {
        key.to_lowercase()
    } else {
        key
    }
}
