use crate::MeasureTool;
use crate::config::LensConfig;
use crate::contracts::{ReviewEntrypoint, ReviewMeasurement, ReviewOutput, ReviewScope};
use crate::facts::FileFacts;
use crate::facts::RunContext;
use crate::producers::produce_measurement;
use crate::util::{absolutize, normalize_slashes, write_json};
use anyhow::{Context, Result};
use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const REVIEW_TOOLS: &[MeasureTool] = &[
    MeasureTool::Hotspots,
    MeasureTool::Clones,
    MeasureTool::EscapeHatches,
    MeasureTool::Reliability,
    MeasureTool::ApiHealth,
    MeasureTool::TypeHealth,
];

pub(crate) fn run_review(
    config: LensConfig,
    changed_since: Option<String>,
    diff_file: Option<PathBuf>,
) -> Result<PathBuf> {
    fs::create_dir_all(&config.output_dir)?;
    let changed_files = changed_files(&config.project_root, changed_since.as_deref(), &diff_file)?;
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

    let output = ReviewOutput {
        version: 1,
        generated_from: "rqlens",
        scope: ReviewScope {
            changed_since,
            diff_file: diff_file.map(normalize_slashes),
            changed_files: relative_paths(&config.project_root, &changed_files),
            measured_rust_files: relative_paths(&config.project_root, &measured_files),
            entrypoints,
        },
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

fn changed_files(
    project_root: &Path,
    changed_since: Option<&str>,
    diff_file: &Option<PathBuf>,
) -> Result<Vec<PathBuf>> {
    if let Some(diff_file) = diff_file {
        let text = fs::read_to_string(diff_file)
            .with_context(|| format!("reading diff file {}", diff_file.display()))?;
        return Ok(diff_paths(project_root, &text));
    }
    git_changed_paths(project_root, changed_since.unwrap_or("HEAD"))
}

fn git_changed_paths(project_root: &Path, base: &str) -> Result<Vec<PathBuf>> {
    let output = Command::new("git")
        .arg("diff")
        .arg("--name-only")
        .arg(base)
        .arg("--")
        .current_dir(project_root)
        .output()
        .with_context(|| format!("running git diff from {}", project_root.display()))?;
    if !output.status.success() {
        anyhow::bail!(
            "git diff failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let mut paths = BTreeSet::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            paths.insert(absolutize(project_root.join(trimmed)));
        }
    }
    Ok(paths.into_iter().collect())
}

fn diff_paths(project_root: &Path, diff: &str) -> Vec<PathBuf> {
    let mut paths = BTreeSet::new();
    for line in diff.lines() {
        if let Some(path) = line.strip_prefix("+++ b/")
            && path != "/dev/null"
        {
            paths.insert(absolutize(project_root.join(path)));
        }
    }
    paths.into_iter().collect()
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
    let serde_json::Value::Array(rows) = payload else {
        return payload;
    };
    serde_json::Value::Array(
        rows.into_iter()
            .filter(|row| row_matches_changed_file(project_root, row, measured_files))
            .collect(),
    )
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
    normalize_slashes(canonical).to_lowercase()
}
