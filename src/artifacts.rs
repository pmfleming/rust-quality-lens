use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

use crate::config::LensConfig;
use crate::facts::{FileFacts, ModuleGraph};
use crate::risk_model::{ChangeFacts, CorrectnessFacts, PerformanceFacts};
use crate::util::{iter_rust_files, normalize_slashes};
use crate::{classify_module, module_for_path, source_confidence};
struct ArtifactRead {
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
    pub(crate) slowspots: Option<PerformanceIndex>,
    reads: Vec<ArtifactRead>,
}

impl MapArtifacts {
    pub(crate) fn load(config: &LensConfig) -> Self {
        let hotspot_read = read_artifact(config, "hotspots.json", true);
        let correctness_read = read_artifact(config, "correctness_review.json", true);
        let locality_read = read_artifact(config, "locality_metrics.json", true);
        let leverage_read = read_artifact(config, "leverage_metrics.json", true);
        let slowspots_read = read_artifact(config, "slowspots.json", false);
        let hotspots = hotspot_read.value.as_ref().map(hotspot_index);
        let correctness = correctness_read
            .value
            .as_ref()
            .map(CorrectnessIndex::from_review);
        let locality = locality_read
            .value
            .as_ref()
            .map(|value| metric_index(value, &["locality_risk"]));
        let leverage = leverage_read
            .value
            .as_ref()
            .map(|value| metric_index(value, &["pressure_score", "leverage_pressure"]));
        let slowspots = slowspots_read.value.as_ref().map(performance_index);
        Self {
            hotspots,
            correctness,
            locality,
            leverage,
            slowspots,
            reads: vec![
                hotspot_read,
                correctness_read,
                locality_read,
                leverage_read,
                slowspots_read,
            ],
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
        return ArtifactRead {
            name: file_name,
            path,
            required,
            status: "missing",
            reason: Some("artifact has not been produced".to_string()),
            value: None,
        };
    }
    if artifact_is_stale(&path, &config.source_roots) {
        return ArtifactRead {
            name: file_name,
            path,
            required,
            status: "stale",
            reason: Some("artifact is older than one or more Rust source files".to_string()),
            value: None,
        };
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
        None => ArtifactRead {
            name: file_name,
            path,
            required,
            status: "unreadable",
            reason: Some("artifact is not valid JSON".to_string()),
            value: None,
        },
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

fn hotspot_index(value: &Value) -> BTreeMap<String, f64> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let module = module_from_record(row)?;
            let score = first_f64(row, &["score", "quality_score", "complexity_score"])?;
            Some((module, score))
        })
        .collect()
}

fn metric_index(value: &Value, score_keys: &[&str]) -> BTreeMap<String, f64> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let module = module_from_record(row)?;
            let score = first_f64(row, score_keys)?;
            Some((module, score))
        })
        .collect()
}

#[derive(Clone, Default)]
pub(crate) struct CorrectnessIndex {
    by_module: BTreeMap<String, CorrectnessFacts>,
}

impl CorrectnessIndex {
    fn from_review(value: &Value) -> Self {
        let mut by_module = BTreeMap::<String, CorrectnessFacts>::new();
        for test in value["tests"].as_array().into_iter().flatten() {
            let module = test["module"]
                .as_str()
                .map(str::to_string)
                .or_else(|| module_from_record(test));
            let Some(module) = module else {
                continue;
            };
            let facts = by_module.entry(module).or_default();
            facts.test_count += 1;
            match test["last_status"].as_str().unwrap_or("unknown") {
                "failed" => facts.failed_count += 1,
                "skipped" | "ignored" => facts.skipped_count += 1,
                "passed" => {}
                _ => facts.unknown_count += 1,
            }
        }
        Self { by_module }
    }

    pub(crate) fn for_module(&self, module: &str) -> CorrectnessFacts {
        self.by_module.get(module).cloned().unwrap_or_default()
    }
}

#[derive(Clone, Default)]
pub(crate) struct PerformanceIndex {
    pub(crate) by_module: BTreeMap<String, PerformanceFacts>,
    pub(crate) raw: BTreeMap<String, Value>,
}

fn performance_index(value: &Value) -> PerformanceIndex {
    let rows: Vec<&Value> = if let Some(array) = value.as_array() {
        array.iter().collect()
    } else if let Some(array) = value["slowspots"].as_array() {
        array.iter().collect()
    } else if let Some(array) = value["benchmarks"].as_array() {
        array.iter().collect()
    } else {
        Vec::new()
    };
    let mut by_module = BTreeMap::new();
    let mut raw = BTreeMap::new();
    for row in rows {
        let Some(module) = module_from_record(row) else {
            continue;
        };
        by_module.insert(
            module.clone(),
            PerformanceFacts {
                benchmark_score: first_f64(
                    row,
                    &[
                        "benchmark_score",
                        "performance_score",
                        "score",
                        "perf_score",
                    ],
                )
                .unwrap_or(0.0),
                mean_ms: first_f64(row, &["mean_ms", "mean_runtime_ms", "duration_ms", "mean"])
                    .unwrap_or(0.0),
                variance: first_f64(row, &["variance", "runtime_variance", "stddev_ratio"])
                    .unwrap_or(0.0),
            },
        );
        raw.insert(module, row.clone());
    }
    PerformanceIndex { by_module, raw }
}

#[derive(Default)]
struct ModuleGitFacts {
    churn: u64,
    commits: BTreeSet<String>,
    contributors: BTreeSet<String>,
    defect_commits: BTreeSet<String>,
    cochange_count: usize,
}

pub(crate) struct GitHistoryIndex {
    by_module: BTreeMap<String, ModuleGitFacts>,
    status: &'static str,
    reason: Option<String>,
}

impl GitHistoryIndex {
    pub(crate) fn for_module(
        &self,
        module: &str,
        correctness: Option<&CorrectnessFacts>,
    ) -> Option<ChangeFacts> {
        if self.status != "available" {
            return None;
        }
        let facts = self.by_module.get(module);
        Some(ChangeFacts {
            churn: facts.map(|facts| facts.churn).unwrap_or_default(),
            commit_count: facts.map(|facts| facts.commits.len()).unwrap_or_default(),
            contributor_count: facts
                .map(|facts| facts.contributors.len())
                .unwrap_or_default(),
            defect_commit_count: facts
                .map(|facts| facts.defect_commits.len())
                .unwrap_or_default(),
            has_test_evidence: correctness.is_some_and(|facts| facts.test_count > 0),
        })
    }

    pub(crate) fn raw_for_module(&self, module: &str) -> Value {
        let Some(facts) = self.by_module.get(module) else {
            return json!({
                "churn": 0,
                "commit_count": 0,
                "contributor_count": 0,
                "defect_commit_count": 0,
                "cochange_count": 0,
            });
        };
        json!({
            "churn": facts.churn,
            "commit_count": facts.commits.len(),
            "contributor_count": facts.contributors.len(),
            "defect_commit_count": facts.defect_commits.len(),
            "cochange_count": facts.cochange_count,
        })
    }

    pub(crate) fn status_json(&self) -> Value {
        json!({
            "status": self.status,
            "required": true,
            "path": ".git",
            "reason": self.reason,
        })
    }
}

pub(crate) fn git_history_facts(config: &LensConfig, graph: &ModuleGraph) -> GitHistoryIndex {
    let git_check = Command::new("git")
        .arg("rev-parse")
        .arg("--is-inside-work-tree")
        .current_dir(&config.project_root)
        .output();
    if !git_check
        .as_ref()
        .is_ok_and(|output| output.status.success())
    {
        return GitHistoryIndex {
            by_module: BTreeMap::new(),
            status: "unavailable",
            reason: Some("project root is not inside a git work tree".to_string()),
        };
    }
    let mut command = Command::new("git");
    command
        .arg("log")
        .arg("--numstat")
        .arg("--format=commit:%H%x09%an%x09%s")
        .arg("--");
    for root in module_source_roots(graph) {
        command.arg(root);
    }
    let output = match command.current_dir(&config.project_root).output() {
        Ok(output) if output.status.success() => output,
        _ => {
            return GitHistoryIndex {
                by_module: BTreeMap::new(),
                status: "unavailable",
                reason: Some("git log could not be read".to_string()),
            };
        }
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut by_module = BTreeMap::<String, ModuleGitFacts>::new();
    let mut current_hash = String::new();
    let mut current_author = String::new();
    let mut current_subject = String::new();
    let mut current_modules = BTreeSet::<String>::new();
    for line in text.lines().chain(std::iter::once("")) {
        if line.starts_with("commit:") || line.is_empty() {
            flush_git_commit(
                &mut by_module,
                &current_hash,
                &current_author,
                &current_subject,
                &current_modules,
            );
            current_modules.clear();
            if let Some(rest) = line.strip_prefix("commit:") {
                let parts: Vec<&str> = rest.splitn(3, '\t').collect();
                current_hash = parts.first().copied().unwrap_or_default().to_string();
                current_author = parts.get(1).copied().unwrap_or_default().to_string();
                current_subject = parts.get(2).copied().unwrap_or_default().to_string();
            }
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 3 {
            continue;
        }
        let Some(module) = git_path_to_module(parts[2], graph) else {
            continue;
        };
        let added = parts[0].parse::<u64>().unwrap_or(0);
        let deleted = parts[1].parse::<u64>().unwrap_or(0);
        by_module.entry(module.clone()).or_default().churn += added + deleted;
        current_modules.insert(module);
    }
    GitHistoryIndex {
        by_module,
        status: "available",
        reason: None,
    }
}

fn flush_git_commit(
    by_module: &mut BTreeMap<String, ModuleGitFacts>,
    hash: &str,
    author: &str,
    subject: &str,
    modules: &BTreeSet<String>,
) {
    if hash.is_empty() {
        return;
    }
    let defect = is_defect_subject(subject);
    for module in modules {
        let facts = by_module.entry(module.clone()).or_default();
        facts.commits.insert(hash.to_string());
        facts.contributors.insert(author.to_string());
        if defect {
            facts.defect_commits.insert(hash.to_string());
        }
        facts.cochange_count += modules.len().saturating_sub(1);
    }
}

fn is_defect_subject(subject: &str) -> bool {
    let lower = subject.to_lowercase();
    [
        "bug",
        "fix",
        "fixed",
        "defect",
        "regression",
        "panic",
        "crash",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn module_source_roots(graph: &ModuleGraph) -> Vec<String> {
    let mut roots = BTreeSet::new();
    for module in graph.modules.values() {
        if let Some(first) = normalize_slashes(&module.path).split('/').next() {
            roots.insert(first.to_string());
        }
    }
    if roots.is_empty() {
        roots.insert("src".to_string());
    }
    roots.into_iter().collect()
}

fn git_path_to_module(path: &str, graph: &ModuleGraph) -> Option<String> {
    let normalized = normalize_slashes(Path::new(path));
    graph
        .modules
        .values()
        .find(|module| normalize_slashes(&module.path) == normalized)
        .map(|module| module.key.clone())
        .or_else(|| {
            let module = module_for_path(&normalized);
            graph.modules.contains_key(&module).then_some(module)
        })
}

pub(crate) fn cycle_modules(dependencies: &HashMap<String, HashSet<String>>) -> BTreeSet<String> {
    let mut result = BTreeSet::new();
    for module in dependencies.keys() {
        if dependencies
            .get(module)
            .into_iter()
            .flatten()
            .any(|target| path_exists(target, module, dependencies, &mut HashSet::new()))
        {
            result.insert(module.clone());
        }
    }
    result
}

fn path_exists(
    current: &str,
    target: &str,
    dependencies: &HashMap<String, HashSet<String>>,
    seen: &mut HashSet<String>,
) -> bool {
    if current == target {
        return true;
    }
    if !seen.insert(current.to_string()) {
        return false;
    }
    dependencies
        .get(current)
        .into_iter()
        .flatten()
        .any(|next| path_exists(next, target, dependencies, seen))
}

pub(crate) fn layer_violations(graph: &ModuleGraph) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for (source, targets) in &graph.dependencies {
        let count = targets
            .iter()
            .filter(|target| is_layer_violation(source, target))
            .count();
        if count > 0 {
            counts.insert(source.clone(), count);
        }
    }
    counts
}

pub(crate) fn is_layer_violation(source: &str, target: &str) -> bool {
    let Some(source_rank) = layer_rank(classify_module(source)) else {
        return false;
    };
    let Some(target_rank) = layer_rank(classify_module(target)) else {
        return false;
    };
    source_rank < target_rank
}

fn layer_rank(layer: &str) -> Option<u8> {
    match layer {
        "Tests" => None,
        "Interface" => Some(4),
        "Application" | "Infrastructure" => Some(3),
        "Domain" => Some(2),
        _ => None,
    }
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
    let has_missing = {
        let missing = object
            .entry("missing_input")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .expect("missing_input should be an array");
        for read in &artifacts.reads {
            if read.status == "missing" && (read.required || read.name == "slowspots.json") {
                missing.push(Value::String(read.name.to_string()));
            }
        }
        if git_history.status != "available" {
            missing.push(Value::String("git_history".to_string()));
        }
        !missing.is_empty()
    };
    let has_stale = {
        let stale = object
            .entry("stale_input")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .expect("stale_input should be an array");
        for read in &artifacts.reads {
            if read.status == "stale" {
                stale.push(Value::String(read.name.to_string()));
            }
        }
        !stale.is_empty()
    };
    let partial = has_missing
        || has_stale
        || object["unsupported_pattern"]
            .as_array()
            .is_some_and(|items| !items.is_empty());
    object.insert("complete".to_string(), Value::Bool(!partial));
    object.insert("partial".to_string(), Value::Bool(partial));
    confidence
}

fn module_from_record(row: &Value) -> Option<String> {
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

fn first_f64(row: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|key| row[*key].as_f64())
}

pub(crate) fn json_f64(row: &Value, key: &str) -> f64 {
    row[key].as_f64().unwrap_or(f64::NEG_INFINITY)
}
