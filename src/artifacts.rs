use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::config::LensConfig;
use crate::facts::FileFacts;
use crate::measurement::{module_for_path, source_confidence};
use crate::util::{iter_rust_files, normalize_slashes};

mod history;
mod indices;
mod topology;

pub(crate) use history::{GitHistoryIndex, git_history_facts};
pub(crate) use indices::CorrectnessIndex;
pub(crate) use topology::{cycle_modules, is_layer_violation, layer_violations};

pub(super) struct ArtifactRead {
    name: &'static str,
    path: PathBuf,
    required: bool,
    status: &'static str,
    reason: Option<String>,
    value: Option<Value>,
}

pub(crate) struct MapArtifacts {
    pub(crate) hotspots: Option<BTreeMap<String, f64>>,
    pub(crate) correctness: Option<CorrectnessIndex>,
    pub(crate) locality: Option<BTreeMap<String, f64>>,
    pub(crate) leverage: Option<BTreeMap<String, f64>>,
    reads: Vec<ArtifactRead>,
}

impl MapArtifacts {
    pub(crate) fn load(config: &LensConfig) -> Self {
        let hotspot_read = read_artifact(config, "hotspots.json", true);
        let correctness_read = read_artifact(config, "correctness_review.json", true);
        let locality_read = read_artifact(config, "locality_metrics.json", true);
        let leverage_read = read_artifact(config, "leverage_metrics.json", true);
        let hotspots = hotspot_read.value.as_ref().map(indices::hotspot_index);
        let correctness = correctness_read
            .value
            .as_ref()
            .map(CorrectnessIndex::from_review);
        let locality = locality_read
            .value
            .as_ref()
            .map(|value| indices::metric_index(value, &["locality_risk"]));
        let leverage = leverage_read
            .value
            .as_ref()
            .map(|value| indices::metric_index(value, &["pressure_score", "leverage_pressure"]));
        Self {
            hotspots,
            correctness,
            locality,
            leverage,
            reads: vec![hotspot_read, correctness_read, locality_read, leverage_read],
        }
    }

    pub(crate) fn status_json(&self, git_history: &GitHistoryIndex) -> Value {
        let mut status = serde_json::Map::new();
        for read in &self.reads {
            status.insert(read.name.to_string(), read.status_json());
        }
        status.insert("git_history".to_string(), git_history.status_json());
        Value::Object(status)
    }
}

impl ArtifactRead {
    fn status_json(&self) -> Value {
        json!({
            "status": self.status,
            "required": self.required,
            "path": normalize_slashes(&self.path),
            "reason": self.reason,
        })
    }
}

fn read_artifact(config: &LensConfig, file_name: &'static str, required: bool) -> ArtifactRead {
    let path = config.output_dir.join(file_name);
    if !path.exists() {
        return artifact_read(
            file_name,
            path,
            required,
            "missing",
            "artifact has not been produced",
        );
    }
    if artifact_is_stale(&path, &config.source_roots) {
        return artifact_read(
            file_name,
            path,
            required,
            "stale",
            "artifact is older than one or more Rust source files",
        );
    }
    match fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
    {
        Some(value) => ArtifactRead {
            name: file_name,
            path,
            required,
            status: "available",
            reason: None,
            value: Some(value),
        },
        None => artifact_read(
            file_name,
            path,
            required,
            "unreadable",
            "artifact is not valid JSON",
        ),
    }
}

fn artifact_read(
    name: &'static str,
    path: PathBuf,
    required: bool,
    status: &'static str,
    reason: &str,
) -> ArtifactRead {
    ArtifactRead {
        name,
        path,
        required,
        status,
        reason: Some(reason.to_string()),
        value: None,
    }
}

fn artifact_is_stale(path: &Path, source_roots: &[String]) -> bool {
    let artifact_modified = fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH);
    iter_rust_files(source_roots)
        .into_iter()
        .filter_map(|source| {
            fs::metadata(source)
                .and_then(|metadata| metadata.modified())
                .ok()
        })
        .any(|modified| artifact_modified < modified)
}

pub(crate) fn map_measurement_confidence(
    paths: &[String],
    facts: &[FileFacts],
    artifacts: &MapArtifacts,
    git_history: &GitHistoryIndex,
) -> Value {
    let mut confidence = source_confidence(paths, facts);
    let Some(object) = confidence.as_object_mut() else {
        return confidence;
    };
    let has_missing = push_artifact_inputs(
        object,
        "missing_input",
        artifacts
            .reads
            .iter()
            .filter(|read| read.status == "missing" && read.required)
            .map(|read| read.name),
    ) || push_git_missing(object, git_history);
    let has_stale = push_artifact_inputs(
        object,
        "stale_input",
        artifacts
            .reads
            .iter()
            .filter(|read| read.status == "stale")
            .map(|read| read.name),
    );
    let partial = has_missing
        || has_stale
        || object["unsupported_pattern"]
            .as_array()
            .is_some_and(|items| !items.is_empty());
    object.insert("complete".to_string(), Value::Bool(!partial));
    object.insert("partial".to_string(), Value::Bool(partial));
    confidence
}

fn push_artifact_inputs<'a>(
    object: &mut serde_json::Map<String, Value>,
    key: &str,
    names: impl Iterator<Item = &'a str>,
) -> bool {
    let Some(items) = object
        .entry(key)
        .or_insert_with(|| json!([]))
        .as_array_mut()
    else {
        return false;
    };
    let start_len = items.len();
    items.extend(names.map(|name| Value::String(name.to_string())));
    items.len() > start_len
}

fn push_git_missing(
    object: &mut serde_json::Map<String, Value>,
    git_history: &GitHistoryIndex,
) -> bool {
    if git_history.is_available() {
        return false;
    }
    push_artifact_inputs(object, "missing_input", ["git_history"].into_iter())
}

pub(super) fn module_from_record(row: &Value) -> Option<String> {
    row["module_key"]
        .as_str()
        .or_else(|| row["module"].as_str())
        .map(str::to_string)
        .or_else(|| {
            row["path"]
                .as_str()
                .or_else(|| row["name"].as_str())
                .map(module_for_path)
        })
}

pub(super) fn first_f64(row: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|key| row[*key].as_f64())
}

pub(crate) fn json_f64(row: &Value, key: &str) -> f64 {
    row[key].as_f64().unwrap_or(f64::NEG_INFINITY)
}
