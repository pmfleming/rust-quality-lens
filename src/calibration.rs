use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::artifacts::artifact_payload;
use crate::config::{LensConfig, SemanticIdentityMode};
use crate::contracts::artifact_document;
use crate::facts::RunContext;
use crate::measurement::{MODEL_ID, MODEL_VERSION};
use crate::producers;
use crate::tool::MeasureTool;
use crate::util::{bundled_helper_manifest, round2, write_json};

const CALIBRATION_TOOLS: [MeasureTool; 6] = [
    MeasureTool::Hotspots,
    MeasureTool::Correctness,
    MeasureTool::Locality,
    MeasureTool::Leverage,
    MeasureTool::ModuleCohesion,
    MeasureTool::Map,
];

pub(crate) fn run(project_specs: &[String], output_dir: PathBuf) -> Result<PathBuf> {
    if project_specs.is_empty() {
        bail!("calibration requires at least one --project NAME=PATH");
    }
    fs::create_dir_all(&output_dir)?;
    let helper_manifest = bundled_helper_manifest()?;
    let mut pool = CalibrationPool::default();
    for spec in project_specs {
        pool.add(calibrate_project(spec, &output_dir, &helper_manifest)?);
    }
    let report_path = output_dir.join("calibration_report.json");
    write_json(&report_path, &pool.report())?;
    Ok(report_path)
}

struct ProjectCalibration {
    report: Value,
    function_scores: Vec<f64>,
    module_scores: Vec<f64>,
    total_scores: Vec<f64>,
    cyclomatic_complexities: Vec<f64>,
    cognitive_complexities: Vec<f64>,
    semantic: crate::semantic::IdentityResolutionSummary,
}

#[derive(Default)]
struct CalibrationPool {
    projects: Vec<Value>,
    function_scores: Vec<f64>,
    module_scores: Vec<f64>,
    total_scores: Vec<f64>,
    cyclomatic_complexities: Vec<f64>,
    cognitive_complexities: Vec<f64>,
    references: usize,
    resolved: usize,
    local: usize,
    unresolved: usize,
}

impl CalibrationPool {
    fn add(&mut self, project: ProjectCalibration) {
        self.function_scores.extend(project.function_scores);
        self.module_scores.extend(project.module_scores);
        self.total_scores.extend(project.total_scores);
        self.cyclomatic_complexities
            .extend(project.cyclomatic_complexities);
        self.cognitive_complexities
            .extend(project.cognitive_complexities);
        self.references += project.semantic.reference_count;
        self.resolved += project.semantic.resolved_count;
        self.local += project.semantic.local_definition_count;
        self.unresolved += project.semantic.unresolved_count;
        self.projects.push(project.report);
    }

    fn report(&self) -> Value {
        json!({
            "schema_version": 1,
            "generated_from": "rqlens calibrate",
            "generated_at": chrono::Utc::now().to_rfc3339(),
            "risk_model_id": MODEL_ID,
            "risk_model_version": MODEL_VERSION,
            "project_count": self.projects.len(),
            "projects": self.projects,
            "pooled_distributions": {
                "function_hotspot_score": distribution(&self.function_scores),
                "module_hotspot_score": distribution(&self.module_scores),
                "total_score": distribution(&self.total_scores),
                "cyclomatic_complexity": distribution(&self.cyclomatic_complexities),
                "cognitive_complexity": distribution(&self.cognitive_complexities),
            },
            "semantic_identity": {
                "reference_count": self.references,
                "resolved_count": self.resolved,
                "local_definition_count": self.local,
                "unresolved_count": self.unresolved,
                "resolution_percent": percent(self.resolved, self.references),
            },
            "exploratory_thresholds": {
                "function_hotspot": thresholds(&self.function_scores),
                "module_hotspot": thresholds(&self.module_scores),
                "total_score": thresholds(&self.total_scores),
            },
            "risk_model_v5_candidate": {
                "status": "deferred",
                "reason": "New CRAP, architecture-rule, and cohesion evidence has not yet been validated against held-out reviewed outcomes.",
                "required_before_activation": [
                    "fresh function coverage across the calibration corpus",
                    "project-specific architecture rules for each corpus project",
                    "held-out reviewed outcome labels",
                    "risk-ranking lift without degrading identity confidence"
                ]
            },
            "limitations": [
                "Percentile thresholds are empirical triage bands, not defect probabilities.",
                "Static correctness catalogs leave unexecuted tests unknown; run and coverage evidence should be added for release gates.",
                "The sample is intentionally heterogeneous but too small to establish ecosystem-wide norms."
            ],
        })
    }
}

fn calibrate_project(
    spec: &str,
    output_dir: &Path,
    helper_manifest: &Path,
) -> Result<ProjectCalibration> {
    let (name, root) = parse_project(spec)?;
    let config = calibration_config(&name, root.clone(), output_dir, helper_manifest)?;
    let context = RunContext::new(&config, &CALIBRATION_TOOLS)
        .with_context(|| format!("extracting calibration facts for {name}"))?;
    let payloads = calibration_payloads(&config, &context)?;
    let hotspots = &payloads["hotspots"];
    let map = &payloads["map"];
    let cohesion = &payloads["module-cohesion"];
    let function_scores = scores_where(hotspots, "function", "score");
    let module_scores = scores_where(hotspots, "module", "score");
    let total_scores = map_scores(map, "total_score");
    let cyclomatic_complexities = scores_where(hotspots, "function", "cyclomatic_complexity");
    let cognitive_complexities = scores_where(hotspots, "function", "cognitive_complexity");
    let report = project_report(
        name,
        root,
        &config,
        &context,
        (hotspots, map, cohesion),
        (&function_scores, &module_scores, &total_scores),
    );
    Ok(ProjectCalibration {
        report,
        function_scores,
        module_scores,
        total_scores,
        cyclomatic_complexities,
        cognitive_complexities,
        semantic: context.identity_resolution.clone(),
    })
}

fn calibration_config(
    name: &str,
    root: PathBuf,
    output_dir: &Path,
    helper_manifest: &Path,
) -> Result<LensConfig> {
    let source_roots = calibration_source_roots(&root);
    if source_roots.is_empty() {
        bail!("calibration project {name} has no discoverable Rust source roots");
    }
    Ok(LensConfig {
        project_name: name.to_string(),
        project_root: root,
        source_roots: source_roots
            .into_iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect(),
        output_dir: output_dir.join(safe_name(name)),
        helper_manifest: helper_manifest.to_path_buf(),
        identity_resolution: SemanticIdentityMode::Auto,
        rust_analyzer: PathBuf::from("rust-analyzer"),
        identity_timeout_seconds: 90,
        identity_offline: true,
        verification: Default::default(),
        architecture: Default::default(),
        policy: Default::default(),
    })
}

fn calibration_payloads(
    config: &LensConfig,
    context: &RunContext,
) -> Result<BTreeMap<&'static str, Value>> {
    fs::create_dir_all(&config.output_dir)?;
    let mut payloads = BTreeMap::new();
    for tool in &CALIBRATION_TOOLS {
        let payload = producers::produce_measurement(tool, config, context)?;
        write_json(
            &config.output_dir.join(tool.output_file()),
            &artifact_document(tool, config, context, payload.clone()),
        )?;
        payloads.insert(tool.name(), payload);
    }
    Ok(payloads)
}

fn project_report(
    name: String,
    root: PathBuf,
    config: &LensConfig,
    context: &RunContext,
    evidence: (&Value, &Value, &Value),
    scores: (&[f64], &[f64], &[f64]),
) -> Value {
    let (hotspots, map, cohesion) = evidence;
    let (function_scores, module_scores, total_scores) = scores;
    let identity_backends =
        context
            .source_facts
            .iter()
            .fold(BTreeMap::<String, usize>::new(), |mut counts, fact| {
                *counts.entry(fact.identity_backend.clone()).or_default() += 1;
                counts
            });
    let function_count = context
        .source_facts
        .iter()
        .map(|fact| fact.items.functions.len())
        .sum::<usize>();
    let sloc = context
        .source_facts
        .iter()
        .map(|fact| fact.source.source_nonblank_line_count)
        .sum::<usize>();
    let module_count = map["graph"]["nodes"].as_array().map_or(0, Vec::len);
    let unknown_modules = map["meta"]["summary"]["unknown_module_count"]
        .as_u64()
        .unwrap_or_default() as usize;
    json!({
        "name": name,
        "path": root,
        "git_revision": git_revision(&config.project_root),
        "rust_file_count": context.source_facts.len(),
        "function_count": function_count,
        "module_count": module_count,
        "source_nonblank_lines": sloc,
        "identity_backends": identity_backends,
        "cargo_identity_percent": percent(*identity_backends.get("cargo_metadata").unwrap_or(&0), context.source_facts.len()),
        "semantic_identity": context.identity_resolution.to_json(),
        "unknown_module_count": unknown_modules,
        "unknown_module_percent": percent(unknown_modules, module_count),
        "change_entropy": cohesion["summary"]["change_entropy"],
        "distributions": {
            "function_hotspot_score": distribution(function_scores),
            "module_hotspot_score": distribution(module_scores),
            "maintainability_risk": distribution(&map_scores(map, "maintainability_risk")),
            "change_risk": distribution(&map_scores(map, "change_risk")),
            "correctness_risk": distribution(&map_scores(map, "correctness_risk")),
            "architectural_risk": distribution(&map_scores(map, "architectural_risk")),
            "quality_risk": distribution(&map_scores(map, "quality_risk")),
            "total_score": distribution(total_scores),
        },
        "top_function_hotspots": top_hotspots(hotspots, "function", 10),
        "top_module_hotspots": top_hotspots(hotspots, "module", 10),
    })
}

fn parse_project(spec: &str) -> Result<(String, PathBuf)> {
    let (name, path) = spec
        .split_once('=')
        .with_context(|| format!("invalid --project {spec:?}; expected NAME=PATH"))?;
    if name.trim().is_empty() || path.trim().is_empty() {
        bail!("invalid --project {spec:?}; expected NAME=PATH");
    }
    let path = path.trim();
    let root =
        fs::canonicalize(path).with_context(|| format!("resolving calibration project {path}"))?;
    Ok((name.trim().to_string(), root))
}

fn safe_name(name: &str) -> String {
    name.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn calibration_source_roots(root: &Path) -> Vec<PathBuf> {
    let conventional = root.join("src");
    if conventional.is_dir() {
        return vec![conventional];
    }
    let crates = root.join("crates");
    let Ok(entries) = fs::read_dir(crates) else {
        return Vec::new();
    };
    let mut roots = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| {
            let src = entry.path().join("src");
            if src.is_dir() {
                Some(src)
            } else if entry.path().join("main.rs").is_file() {
                Some(entry.path())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    roots.sort();
    roots
}

fn scores_where(rows: &Value, kind: &str, field: &str) -> Vec<f64> {
    artifact_payload(rows)
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| row["kind"] == kind)
        .filter_map(|row| row[field].as_f64())
        .collect()
}

fn map_scores(map: &Value, field: &str) -> Vec<f64> {
    artifact_payload(map)["graph"]["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|node| node["data"][field].as_f64())
        .collect()
}

fn top_hotspots(rows: &Value, kind: &str, limit: usize) -> Vec<Value> {
    let mut rows = artifact_payload(rows)
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| row["kind"] == kind)
        .filter_map(|row| Some((row["score"].as_f64()?, row)))
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| right.0.total_cmp(&left.0));
    rows.into_iter()
        .take(limit)
        .map(|(score, row)| {
            json!({
                "name": row["name"],
                "module_key": row["module_key"],
                "path": row["path"],
                "start_line": row["start_line"],
                "score": score,
            })
        })
        .collect()
}

fn distribution(values: &[f64]) -> Value {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mean = if sorted.is_empty() {
        0.0
    } else {
        sorted.iter().sum::<f64>() / sorted.len() as f64
    };
    json!({
        "count": sorted.len(),
        "mean": round2(mean),
        "p50": percentile(&sorted, 0.50),
        "p75": percentile(&sorted, 0.75),
        "p90": percentile(&sorted, 0.90),
        "p95": percentile(&sorted, 0.95),
        "p99": percentile(&sorted, 0.99),
        "max": sorted.last().copied().map(round2).unwrap_or_default(),
    })
}

fn thresholds(values: &[f64]) -> Value {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    json!({
        "watch_at_p90": percentile(&sorted, 0.90),
        "high_at_p95": percentile(&sorted, 0.95),
        "extreme_at_p99": percentile(&sorted, 0.99),
    })
}

fn percentile(sorted: &[f64], quantile: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let index = ((sorted.len() - 1) as f64 * quantile).ceil() as usize;
    round2(sorted[index.min(sorted.len() - 1)])
}

fn percent(part: usize, whole: usize) -> f64 {
    if whole == 0 {
        0.0
    } else {
        round2(part as f64 * 100.0 / whole as f64)
    }
}

fn git_revision(root: &Path) -> Value {
    Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|revision| Value::from(revision.trim().to_string()))
        .unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::{calibration_source_roots, distribution, parse_project, safe_name, thresholds};
    use serde_json::json;
    use std::fs;

    #[test]
    fn calibration_statistics_use_empirical_percentiles() {
        let values = [1.0, 2.0, 3.0, 10.0];
        assert_eq!(
            distribution(&values),
            json!({
                "count": 4,
                "mean": 4.0,
                "p50": 3.0,
                "p75": 10.0,
                "p90": 10.0,
                "p95": 10.0,
                "p99": 10.0,
                "max": 10.0,
            })
        );
        assert_eq!(
            thresholds(&values),
            json!({
                "watch_at_p90": 10.0,
                "high_at_p95": 10.0,
                "extreme_at_p99": 10.0,
            })
        );
    }

    #[test]
    fn calibration_project_specs_are_validated_and_normalized() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let specification = format!(" demo={} ", root.path().display());
        let (name, path) = parse_project(&specification)?;
        assert_eq!(name, "demo");
        assert_eq!(path, fs::canonicalize(root.path())?);
        assert!(parse_project("missing-separator").is_err());
        assert!(parse_project("=missing-name").is_err());
        assert_eq!(safe_name("workspace/core.v1"), "workspace_core_v1");
        Ok(())
    }

    #[test]
    fn calibration_source_roots_support_src_and_crate_layouts() -> anyhow::Result<()> {
        let conventional = tempfile::tempdir()?;
        fs::create_dir(conventional.path().join("src"))?;
        assert_eq!(
            calibration_source_roots(conventional.path()),
            [conventional.path().join("src")]
        );

        let workspace = tempfile::tempdir()?;
        fs::create_dir_all(workspace.path().join("crates/a/src"))?;
        fs::create_dir_all(workspace.path().join("crates/b"))?;
        fs::write(workspace.path().join("crates/b/main.rs"), "fn main() {}")?;
        assert_eq!(
            calibration_source_roots(workspace.path()),
            [
                workspace.path().join("crates/a/src"),
                workspace.path().join("crates/b"),
            ]
        );
        Ok(())
    }
}
