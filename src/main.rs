use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use clap::{Parser, Subcommand, ValueEnum};
use regex::Regex;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;
use tempfile::NamedTempFile;
use walkdir::WalkDir;

const MODEL_ID: &str = "rqlens.architecture_risk";
const MODEL_VERSION: u64 = 1;
const RULESET_ID: &str = "rqlens.generic_layers";
const RULESET_VERSION: u64 = 1;

#[derive(Parser)]
#[command(name = "rqlens")]
#[command(about = "Reusable Rust measurement JSON producers")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Measure {
        #[arg(default_value = "all")]
        tool: MeasureTool,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    Catalog {
        #[arg(long)]
        config: Option<PathBuf>,
    },
}

#[derive(Clone, Debug, ValueEnum)]
enum MeasureTool {
    All,
    Hotspots,
    Clones,
    #[value(name = "escape-hatches")]
    EscapeHatches,
    #[value(name = "type-health")]
    TypeHealth,
    Correctness,
    #[value(name = "correctness-run")]
    CorrectnessRun,
    Locality,
    Leverage,
    Map,
}

impl MeasureTool {
    fn all_tools() -> Vec<Self> {
        vec![
            Self::Hotspots,
            Self::Clones,
            Self::EscapeHatches,
            Self::TypeHealth,
            Self::Correctness,
            Self::CorrectnessRun,
            Self::Locality,
            Self::Leverage,
            Self::Map,
        ]
    }

    fn name(&self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Hotspots => "hotspots",
            Self::Clones => "clones",
            Self::EscapeHatches => "escape-hatches",
            Self::TypeHealth => "type-health",
            Self::Correctness => "correctness",
            Self::CorrectnessRun => "correctness-run",
            Self::Locality => "locality",
            Self::Leverage => "leverage",
            Self::Map => "map",
        }
    }

    fn output_file(&self) -> &'static str {
        match self {
            Self::Hotspots => "hotspots.json",
            Self::Clones => "clones.json",
            Self::EscapeHatches => "rust_escape_hatches.json",
            Self::TypeHealth => "type_health.json",
            Self::Correctness | Self::CorrectnessRun => "correctness_review.json",
            Self::Locality => "locality_metrics.json",
            Self::Leverage => "leverage_metrics.json",
            Self::Map => "map.json",
            Self::All => unreachable!("all has no direct output file"),
        }
    }
}

#[derive(Debug, Deserialize)]
struct RawConfig {
    project_name: Option<String>,
    project_root: Option<PathBuf>,
    source_roots: Option<Vec<String>>,
    output_dir: Option<PathBuf>,
    rust: Option<RawRustConfig>,
}

#[derive(Debug, Deserialize)]
struct RawRustConfig {
    helper_manifest: Option<PathBuf>,
}

#[derive(Clone, Debug)]
struct LensConfig {
    project_name: String,
    project_root: PathBuf,
    source_roots: Vec<String>,
    output_dir: PathBuf,
    helper_manifest: PathBuf,
}

impl LensConfig {
    fn load(path: Option<PathBuf>) -> Result<Self> {
        let config_path = path.as_ref().map(|p| absolutize(p));
        let config_dir = config_path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .unwrap_or(env::current_dir()?);
        let raw = match &config_path {
            Some(path) if path.exists() => toml::from_str::<RawConfig>(
                &fs::read_to_string(path)
                    .with_context(|| format!("reading config {}", path.display()))?,
            )?,
            _ => RawConfig {
                project_name: None,
                project_root: None,
                source_roots: None,
                output_dir: None,
                rust: None,
            },
        };
        let project_root =
            resolve_config_path(raw.project_root.unwrap_or_else(|| ".".into()), &config_dir);
        let source_roots = raw.source_roots.unwrap_or_else(|| vec!["src".to_string()]);
        let output_dir = resolve_project_path(
            raw.output_dir.unwrap_or_else(|| "target/analysis".into()),
            &project_root,
        );
        let default_helper_manifest = repo_root()?.join("rust_helpers").join("Cargo.toml");
        let helper_manifest = resolve_project_path(
            raw.rust
                .and_then(|rust| rust.helper_manifest)
                .unwrap_or(default_helper_manifest),
            &project_root,
        );
        let project_name = raw.project_name.unwrap_or_else(|| {
            project_root
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });
        Ok(Self {
            project_name,
            project_root,
            source_roots,
            output_dir,
            helper_manifest,
        })
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Catalog { config } => print_catalog(&LensConfig::load(config)?),
        Commands::Measure { tool, config } => measure(tool, LensConfig::load(config)?),
    }
}

fn measure(tool: MeasureTool, config: LensConfig) -> Result<()> {
    fs::create_dir_all(&config.output_dir)?;
    let tools = if matches!(tool, MeasureTool::All) {
        MeasureTool::all_tools()
    } else {
        vec![tool]
    };
    let old_cwd = env::current_dir()?;
    env::set_current_dir(&config.project_root)?;
    let result = (|| {
        for tool in tools {
            let output = config.output_dir.join(tool.output_file());
            let payload = match tool {
                MeasureTool::Hotspots => hotspots(&config)?,
                MeasureTool::Clones => clones(&config)?,
                MeasureTool::EscapeHatches => escape_hatches(&config)?,
                MeasureTool::TypeHealth => type_health(&config)?,
                MeasureTool::Correctness => correctness(&config, false)?,
                MeasureTool::CorrectnessRun => correctness(&config, true)?,
                MeasureTool::Locality => locality(&config)?,
                MeasureTool::Leverage => leverage(&config)?,
                MeasureTool::Map => architecture_map(&config)?,
                MeasureTool::All => unreachable!(),
            };
            write_json(&output, &payload)?;
            if matches!(tool, MeasureTool::Correctness | MeasureTool::CorrectnessRun) {
                write_json(
                    &config.output_dir.join("test_catalog.json"),
                    &payload["tests"],
                )?;
            }
            println!(
                "Wrote {} visibility data to {}",
                tool.name(),
                output.display()
            );
        }
        Ok(())
    })();
    env::set_current_dir(old_cwd)?;
    result
}

fn print_catalog(config: &LensConfig) -> Result<()> {
    let tasks: Vec<Value> = task_definitions()
        .into_iter()
        .map(|task| {
            let output_files = if task.tool == "correctness" {
                vec!["correctness_review.json", "test_catalog.json"]
            } else {
                vec![task.output_file]
            };
            json!({
                "id": task.id,
                "category": task.category,
                "subcategory": task.subcategory,
                "title": task.title,
                "description": task.description,
                "commands": [["rqlens", "measure", task.tool]],
                "output_artifacts": output_files.iter().map(|file| format!("target/analysis/{file}")).collect::<Vec<_>>(),
                "absolute_output_artifacts": output_files.iter().map(|file| config.output_dir.join(file).to_string_lossy().to_string()).collect::<Vec<_>>(),
                "depends_on": task.depends_on,
                "expensive": task.expensive,
                "supports_individual_run": true,
                "lens": "rust-quality-lens",
                "tool": task.tool,
                "aliases": task.aliases,
            })
        })
        .collect();
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "version": 1,
            "project_name": config.project_name,
            "analysis_root": config.output_dir,
            "categories": [
                {"id": "quality", "title": "Quality Review"},
                {"id": "correctness", "title": "Correctness Review"},
                {"id": "map", "title": "Map"}
            ],
            "tasks": tasks
        }))?
    );
    Ok(())
}

struct TaskDefinition {
    tool: &'static str,
    id: &'static str,
    aliases: Vec<&'static str>,
    category: &'static str,
    subcategory: &'static str,
    title: &'static str,
    description: &'static str,
    output_file: &'static str,
    depends_on: Vec<&'static str>,
    expensive: bool,
}

fn task_definitions() -> Vec<TaskDefinition> {
    vec![
        TaskDefinition {
            tool: "hotspots",
            id: "quality.hotspots",
            aliases: vec![],
            category: "quality",
            subcategory: "hotspots",
            title: "Hotspots",
            description: "Ranks complexity risk without SLOC-only scoring.",
            output_file: "hotspots.json",
            depends_on: vec![],
            expensive: false,
        },
        TaskDefinition {
            tool: "clones",
            id: "quality.clones",
            aliases: vec![],
            category: "quality",
            subcategory: "clones",
            title: "Clones",
            description: "Finds repeated token and AST-like code structures.",
            output_file: "clones.json",
            depends_on: vec![],
            expensive: false,
        },
        TaskDefinition {
            tool: "escape-hatches",
            id: "quality.escape_hatches",
            aliases: vec!["quality.escape-hatches"],
            category: "quality",
            subcategory: "safety",
            title: "Rust Escape Hatches",
            description: "Tracks unsafe, FFI, raw memory, globals, glob imports, and lint suppressions.",
            output_file: "rust_escape_hatches.json",
            depends_on: vec![],
            expensive: false,
        },
        TaskDefinition {
            tool: "type-health",
            id: "quality.type_health",
            aliases: vec!["quality.type-health"],
            category: "quality",
            subcategory: "structure",
            title: "Type Health",
            description: "Ranks wide structs, large enums, broad method surfaces, and impl spread.",
            output_file: "type_health.json",
            depends_on: vec![],
            expensive: false,
        },
        TaskDefinition {
            tool: "locality",
            id: "quality.locality_dynamic",
            aliases: vec!["quality.locality"],
            category: "quality",
            subcategory: "locality",
            title: "Code Locality",
            description: "Measures dependency spread, hidden coupling, interface explicitness, and change locality.",
            output_file: "locality_metrics.json",
            depends_on: vec![],
            expensive: false,
        },
        TaskDefinition {
            tool: "leverage",
            id: "quality.locality_leverage",
            aliases: vec!["quality.leverage"],
            category: "quality",
            subcategory: "leverage",
            title: "Architecture Leverage",
            description: "Measures reach, invariant surface, divergence pressure, and co-change ripple.",
            output_file: "leverage_metrics.json",
            depends_on: vec![],
            expensive: false,
        },
        TaskDefinition {
            tool: "correctness",
            id: "correctness.catalog",
            aliases: vec![],
            category: "correctness",
            subcategory: "tests",
            title: "Correctness Catalog",
            description: "Discovers Rust tests and groups them by generic architecture area.",
            output_file: "correctness_review.json",
            depends_on: vec![],
            expensive: false,
        },
        TaskDefinition {
            tool: "correctness-run",
            id: "correctness.all",
            aliases: vec![],
            category: "correctness",
            subcategory: "tests",
            title: "All Tests",
            description: "Runs the full Rust test suite and attaches status to the correctness catalog.",
            output_file: "correctness_review.json",
            depends_on: vec![],
            expensive: true,
        },
        TaskDefinition {
            tool: "map",
            id: "map.architecture",
            aliases: vec![],
            category: "map",
            subcategory: "architecture",
            title: "Architecture Map",
            description: "Builds module health, dependency, and risk map data.",
            output_file: "map.json",
            depends_on: vec![
                "quality.hotspots",
                "correctness.catalog",
                "quality.locality_dynamic",
                "quality.locality_leverage",
            ],
            expensive: false,
        },
    ]
}

#[derive(Debug, Deserialize)]
struct FileFacts {
    path: String,
    module_key: String,
    parse_status: String,
    dependencies: Vec<String>,
    child_modules: Vec<String>,
    module_files: Vec<ModuleFileFact>,
    unsupported_patterns: Vec<String>,
    public_api_count: usize,
    types: Vec<TypeFact>,
    impls: Vec<ImplFact>,
    tests: Vec<TestFact>,
    escape_counts: BTreeMap<String, usize>,
    escape_locations: Vec<LocationFact>,
}

#[derive(Debug, Deserialize)]
struct ModuleFileFact {
    module_key: String,
    path: String,
}

#[derive(Debug, Deserialize)]
struct TypeFact {
    type_name: String,
    qualified_name: String,
    module_key: String,
    path: String,
    line: usize,
    kind: String,
    shape: String,
    field_count: usize,
    variant_count: usize,
    variant_field_count: usize,
    declaration_span: usize,
}

#[derive(Debug, Deserialize)]
struct ImplFact {
    type_name: String,
    module_key: String,
    path: String,
    method_count: usize,
}

#[derive(Debug, Deserialize)]
struct TestFact {
    name: String,
    qualified_name: String,
    path: String,
    line: usize,
}

#[derive(Debug, Deserialize)]
struct LocationFact {
    kind: String,
    line: usize,
}

fn rust_facts_for_paths(config: &LensConfig, paths: &[String]) -> Result<Vec<FileFacts>> {
    let files: Vec<String> = iter_rust_files(paths)
        .into_iter()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .collect();
    if files.is_empty() {
        return Ok(Vec::new());
    }
    let mut temp = NamedTempFile::new()?;
    for file in &files {
        writeln!(temp, "{file}")?;
    }
    let output = Command::new("cargo")
        .arg("run")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(&config.helper_manifest)
        .arg("--bin")
        .arg("rust_facts")
        .arg("--")
        .arg(temp.path())
        .env(
            "RQLENS_SOURCE_ROOTS",
            env::join_paths(config.source_roots.iter().map(PathBuf::from))?,
        )
        .output()
        .with_context(|| {
            format!(
                "running Rust fact extractor at {}",
                config.helper_manifest.display()
            )
        })?;
    if !output.status.success() {
        return Err(anyhow!(
            "Rust fact extraction failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn escape_hatches(config: &LensConfig) -> Result<Value> {
    let facts = rust_facts_for_paths(config, &config.source_roots)?;
    let confidence = source_confidence(&config.source_roots, &facts);
    let weights = escape_weights();
    let labels = escape_labels();
    let metadata = tool_score_metadata("escape_hatches");
    let provenance = provenance();
    let mut rows = Vec::new();
    for fact in facts {
        let mut counts = BTreeMap::new();
        for key in weights.keys() {
            counts.insert(key.to_string(), *fact.escape_counts.get(*key).unwrap_or(&0));
        }
        let mut scoring_counts = counts.clone();
        if scoring_counts
            .get("clippy_suppression")
            .copied()
            .unwrap_or(0)
            > 0
        {
            scoring_counts.insert("lint_suppression".to_string(), 0);
        }
        let score = round2(
            scoring_counts
                .iter()
                .map(|(key, count)| *count as f64 * weights[key.as_str()])
                .sum(),
        );
        let locations: Vec<Value> = fact
            .escape_locations
            .iter()
            .filter_map(|loc| {
                labels
                    .get(loc.kind.as_str())
                    .map(|label| json!({"kind": loc.kind, "label": label, "line": loc.line}))
            })
            .collect();
        let get = |key: &str| counts.get(key).copied().unwrap_or(0);
        let unsafe_count =
            get("unsafe_block") + get("unsafe_fn") + get("unsafe_impl") + get("unsafe_trait");
        let ffi_count = get("extern_block") + get("extern_fn");
        let raw_memory_count = get("union")
            + get("raw_borrow")
            + get("asm_macro")
            + get("transmute")
            + get("maybe_uninit");
        let deref_count = get("deref_impl") + get("deref_mut_impl");
        let layout_linkage_count = get("repr_escape") + get("linkage_escape");
        let allow_count = get("lint_suppression") + get("clippy_suppression");
        let mut signals: Vec<String> = counts
            .iter()
            .filter(|(_, count)| **count > 0)
            .map(|(key, count)| {
                format!(
                    "{} {}",
                    labels.get(key.as_str()).unwrap_or(&key.as_str()),
                    count
                )
            })
            .collect();
        if allow_count > 0 {
            signals.push(format!("allow/expect attributes {allow_count}"));
        }
        if signals.is_empty() {
            signals.push("stable".to_string());
        }
        let total_count: usize = counts.values().sum();
        if total_count == 0 {
            continue;
        }
        rows.push(json!({
            "module_name": fact.module_key,
            "module_key": fact.module_key,
            "path": fact.path,
            "escape_hatch_score": score,
            "total_count": total_count,
            "unsafe_count": unsafe_count,
            "ffi_count": ffi_count,
            "global_mutability_count": get("static_mut"),
            "raw_memory_count": raw_memory_count,
            "deref_coercion_count": deref_count,
            "glob_import_count": get("glob_import"),
            "container_ref_return_count": get("container_ref_return"),
            "layout_linkage_count": layout_linkage_count,
            "clippy_suppression_count": get("clippy_suppression"),
            "lint_suppression_count": get("lint_suppression"),
            "allow_attribute_count": allow_count,
            "clippy_allow_count": get("clippy_suppression"),
            "counts": counts,
            "scoring_counts": scoring_counts,
            "locations": locations,
            "allow_locations": locations.iter().filter(|item| matches!(item["kind"].as_str(), Some("lint_suppression" | "clippy_suppression"))).collect::<Vec<_>>(),
            "signals": signals,
            "measured_at": provenance.measured_at,
            "command": provenance.command,
            "host": provenance.host,
            "measurement_confidence": confidence,
            "source": "static_rust_escape_hatches",
            "mock": false,
            "risk_model_id": metadata.risk_model_id,
            "risk_model_version": metadata.risk_model_version,
            "risk_calibration": metadata.risk_calibration,
        }));
    }
    rows.sort_by(|a, b| {
        b["escape_hatch_score"]
            .as_f64()
            .partial_cmp(&a["escape_hatch_score"].as_f64())
            .unwrap()
            .then_with(|| b["total_count"].as_u64().cmp(&a["total_count"].as_u64()))
            .then_with(|| a["module_key"].as_str().cmp(&b["module_key"].as_str()))
    });
    Ok(Value::Array(rows))
}

fn type_health(config: &LensConfig) -> Result<Value> {
    let facts = rust_facts_for_paths(config, &config.source_roots)?;
    let confidence = source_confidence(&config.source_roots, &facts);
    let metadata = tool_score_metadata("type_health");
    let provenance = provenance();
    let mut impls: HashMap<(String, String), (usize, usize, BTreeSet<String>)> = HashMap::new();
    for fact in &facts {
        for imp in &fact.impls {
            let entry = impls
                .entry((imp.module_key.clone(), imp.type_name.clone()))
                .or_insert((0, 0, BTreeSet::new()));
            entry.0 += imp.method_count;
            entry.1 += 1;
            entry.2.insert(imp.path.clone());
        }
    }
    let mut rows = Vec::new();
    for fact in &facts {
        if fact.parse_status != "ok" {
            continue;
        }
        for ty in &fact.types {
            let (method_count, impl_block_count, impl_files) = impls
                .get(&(ty.module_key.clone(), ty.type_name.clone()))
                .cloned()
                .unwrap_or_default();
            let impl_files_vec: Vec<String> = impl_files.into_iter().collect();
            let (risk, signals) =
                type_risk(ty, method_count, impl_block_count, impl_files_vec.len());
            rows.push(json!({
                "type_name": ty.type_name,
                "qualified_name": ty.qualified_name,
                "module_key": ty.module_key,
                "path": ty.path,
                "line": ty.line,
                "kind": ty.kind,
                "shape": ty.shape,
                "field_count": ty.field_count,
                "variant_count": ty.variant_count,
                "variant_field_count": ty.variant_field_count,
                "declaration_span": ty.declaration_span,
                "method_count": method_count,
                "impl_block_count": impl_block_count,
                "impl_file_count": impl_files_vec.len(),
                "impl_files": impl_files_vec,
                "structural_risk": risk,
                "structural_score": round2(100.0 - risk),
                "signals": signals,
                "measured_at": provenance.measured_at,
                "command": provenance.command,
                "host": provenance.host,
                "measurement_confidence": confidence,
                "risk_model_id": metadata.risk_model_id,
                "risk_model_version": metadata.risk_model_version,
                "risk_calibration": metadata.risk_calibration,
                "source": "static_type_health",
                "mock": false,
            }));
        }
    }
    rows.sort_by(|a, b| {
        b["structural_risk"]
            .as_f64()
            .partial_cmp(&a["structural_risk"].as_f64())
            .unwrap()
            .then_with(|| b["method_count"].as_u64().cmp(&a["method_count"].as_u64()))
            .then_with(|| b["field_count"].as_u64().cmp(&a["field_count"].as_u64()))
            .then_with(|| {
                a["qualified_name"]
                    .as_str()
                    .cmp(&b["qualified_name"].as_str())
            })
    });
    Ok(Value::Array(rows))
}

fn correctness(config: &LensConfig, run: bool) -> Result<Value> {
    let paths = correctness_paths(config);
    let facts = rust_facts_for_paths(config, &paths)?;
    let confidence = source_confidence(&paths, &facts);
    let statuses = if run { run_tests()? } else { HashMap::new() };
    let mut tests = Vec::new();
    for fact in &facts {
        if fact.parse_status != "ok" {
            continue;
        }
        for test in &fact.tests {
            let path = normalize_slashes(&test.path);
            let name = test.name.clone();
            let qualified_name = test.qualified_name.clone();
            let id = format!("{path}::{qualified_name}");
            let mut last_status = "unknown".to_string();
            let mut last_duration = Value::Null;
            for key in [
                &id,
                &format!("{path}::{qualified_name}"),
                &qualified_name,
                &name,
            ] {
                if let Some(status) = statuses.get(key) {
                    last_status = status.status.clone();
                    last_duration = status.duration.map(Value::from).unwrap_or(Value::Null);
                    break;
                }
            }
            tests.push(json!({
                "id": id,
                "name": name,
                "qualified_name": qualified_name,
                "path": path,
                "line": test.line,
                "layer": classify_path(&test.path),
                "module": module_for_path(&test.path),
                "description": title_from_name(&test.name),
                "kind": test_kind_for_path(&test.path),
                "last_status": last_status,
                "last_duration": last_duration,
                "command": format!("cargo test {}", test.name),
            }));
        }
    }
    let mut by_layer: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
    for test in &tests {
        let layer = test["layer"].as_str().unwrap_or("Unclassified").to_string();
        let status = test["last_status"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        let entry = by_layer.entry(layer).or_default();
        *entry.entry("total".into()).or_default() += 1;
        *entry.entry(status).or_default() += 1;
    }
    let layers: Vec<Value> = by_layer
        .into_iter()
        .map(|(layer, counts)| {
            let total = *counts.get("total").unwrap_or(&0);
            let failed = *counts.get("failed").unwrap_or(&0);
            json!({
                "name": layer,
                "total": total,
                "passed": *counts.get("passed").unwrap_or(&0),
                "failed": failed,
                "skipped": *counts.get("skipped").unwrap_or(&0),
                "unknown": *counts.get("unknown").unwrap_or(&0),
                "failed_ratio": if total == 0 { 0.0 } else { failed as f64 / total as f64 },
            })
        })
        .collect();
    let summary = json!({
        "test_count": tests.len(),
        "integration_count": tests.iter().filter(|t| t["kind"] == "integration").count(),
        "inline_count": tests.iter().filter(|t| t["kind"] == "inline").count(),
        "benchmark_count": tests.iter().filter(|t| t["kind"] == "benchmark").count(),
        "example_count": tests.iter().filter(|t| t["kind"] == "example").count(),
        "binary_count": tests.iter().filter(|t| t["kind"] == "binary").count(),
        "layers": layers.len(),
        "failed": tests.iter().filter(|t| t["last_status"] == "failed").count(),
        "unknown": tests.iter().filter(|t| t["last_status"] == "unknown").count(),
        "last_run": statuses.get("__run__").map(|s| json!({"status": s.status, "duration": s.duration, "stdout_tail": s.stdout_tail, "stderr_tail": s.stderr_tail})),
    });
    Ok(json!({
        "version": 1,
        "generated_from": "rqlens",
        "layer_ruleset": {"id": RULESET_ID, "version": RULESET_VERSION},
        "summary": summary,
        "measurement_confidence": confidence,
        "layers": layers,
        "tests": tests,
    }))
}

fn hotspots(config: &LensConfig) -> Result<Value> {
    let confidence = source_confidence(&config.source_roots, &[]);
    let mut rows = Vec::new();
    for path in iter_rust_files(&config.source_roots) {
        let text = fs::read_to_string(&path).unwrap_or_default();
        let sloc = text
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                !trimmed.is_empty() && !trimmed.starts_with("//")
            })
            .count() as f64;
        let cognitive = count_matches(
            &text,
            &[
                " if ", " match ", " for ", " while ", " loop ", "&&", "||", "?",
            ],
        ) as f64;
        let cyclomatic =
            1.0 + count_matches(&text, &[" if ", " match ", " for ", " while ", "&&", "||"]) as f64;
        let mi = (100.0 - sloc * 0.15 - cognitive * 1.7).max(0.0);
        let quality_score =
            round2((cognitive * 3.7 + cyclomatic * 2.0 + (65.0 - mi).max(0.0) * 1.2) * 1.12);
        rows.push(json!({
            "name": normalize_slashes(&path),
            "kind": "unit",
            "start_line": 1,
            "end_line": text.lines().count(),
            "cognitive": cognitive,
            "cyclomatic": cyclomatic,
            "mi": mi,
            "effort": sloc * cyclomatic * 25.0,
            "sloc": sloc,
            "ploc": sloc,
            "cloc": text.lines().filter(|line| line.trim().starts_with("//")).count(),
            "bugs": round2(sloc / 500.0),
            "abc_mag": cognitive + cyclomatic,
            "nom_fn": text.matches("fn ").count(),
            "nom_cl": 0,
            "quality_score": quality_score,
            "cognitive_score": round2(cognitive * 3.7),
            "cyclomatic_score": round2(cyclomatic * 2.0),
            "maintainability_score": round2((65.0 - mi).max(0.0) * 1.2),
            "effort_score": round2((sloc * cyclomatic * 25.0).ln_1p() * 4.0),
            "size_score": round2((sloc / 10.0).min(20.0)),
            "score": quality_score,
            "signals": if sloc >= 150.0 || cognitive >= 8.0 { "watch" } else { "stable" },
            "abc_density": if sloc > 0.0 { round2((cognitive + cyclomatic) / sloc) } else { 0.0 },
            "complexity_density": if sloc > 0.0 { round2(quality_score / sloc) } else { 0.0 },
            "risk_model_id": MODEL_ID,
            "risk_model_version": MODEL_VERSION,
            "risk_calibration": "hotspots",
            "measurement_confidence": confidence,
        }));
    }
    rows.sort_by(|a, b| {
        b["score"]
            .as_f64()
            .partial_cmp(&a["score"].as_f64())
            .unwrap()
    });
    Ok(Value::Array(rows))
}

fn clones(config: &LensConfig) -> Result<Value> {
    let confidence = source_confidence(&config.source_roots, &[]);
    let token_re = Regex::new(
        r"[A-Za-z_][A-Za-z0-9_]*|\d+|::|->|=>|==|!=|<=|>=|&&|\|\||[{}\(\)\[\];,.:+\-*/%&|^!<>=?]",
    )?;
    let mut windows: HashMap<String, Vec<(String, usize, usize, String)>> = HashMap::new();
    for path in iter_rust_files(&config.source_roots) {
        let text = fs::read_to_string(&path).unwrap_or_default();
        let lines: Vec<&str> = text.lines().collect();
        let mut tokens = Vec::new();
        for (line_index, line) in lines.iter().enumerate() {
            for cap in token_re.find_iter(line) {
                let token = cap.as_str();
                let normalized = if token
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                {
                    match token {
                        "fn" | "let" | "pub" | "struct" | "enum" | "impl" | "if" | "match"
                        | "for" | "while" | "loop" | "use" | "mod" | "return" | "Self" | "self" => {
                            token.to_string()
                        }
                        _ => "ID".to_string(),
                    }
                } else if token.chars().all(|c| c.is_ascii_digit()) {
                    "LIT".to_string()
                } else {
                    token.to_string()
                };
                tokens.push((normalized, line_index + 1));
            }
        }
        for window in tokens.windows(50) {
            let start = window.first().unwrap().1;
            let end = window.last().unwrap().1;
            if end.saturating_sub(start) + 1 < 3 {
                continue;
            }
            let key = window
                .iter()
                .map(|(token, _)| token.as_str())
                .collect::<Vec<_>>()
                .join("|");
            let snippet = lines[start - 1..end.min(lines.len())].join("\n");
            windows
                .entry(key)
                .or_default()
                .push((normalize_slashes(&path), start, end, snippet));
        }
    }
    let mut rows = Vec::new();
    for (key, instances) in windows {
        let file_count = instances
            .iter()
            .map(|item| &item.0)
            .collect::<HashSet<_>>()
            .len();
        if instances.len() < 2 {
            continue;
        }
        let max_line_span = instances
            .iter()
            .map(|(_, start, end, _)| end - start + 1)
            .max()
            .unwrap_or(0);
        rows.push(json!({
            "engine": "token",
            "hash": stable_hash(&key),
            "token_count": 50,
            "instance_count": instances.len(),
            "file_count": file_count,
            "max_line_span": max_line_span,
            "score": round2(instances.len() as f64 * 5.0),
            "signals": if file_count >= 2 { format!("cross-file x{file_count}") } else { "same-file repeat".to_string() },
            "instances": instances.into_iter().map(|(file_path, start_line, end_line, snippet)| json!({"file_path": file_path, "start_line": start_line, "end_line": end_line, "snippet": snippet})).collect::<Vec<_>>(),
            "measurement_confidence": confidence,
        }));
    }
    rows.sort_by(|a, b| {
        b["score"]
            .as_f64()
            .partial_cmp(&a["score"].as_f64())
            .unwrap()
    });
    Ok(Value::Array(rows))
}

fn locality(config: &LensConfig) -> Result<Value> {
    let graph = module_graph(config)?;
    let mut rows = Vec::new();
    for module in graph.modules.values() {
        let outbound = graph
            .dependencies
            .get(&module.key)
            .map(HashSet::len)
            .unwrap_or(0);
        let inbound = graph
            .reverse_dependencies
            .get(&module.key)
            .map(HashSet::len)
            .unwrap_or(0);
        let risk = ((outbound.saturating_sub(5) * 3) as f64
            + (inbound.saturating_sub(12) as f64 * 0.75))
            .min(100.0);
        rows.push(json!({
            "module_key": module.key,
            "path": module.path,
            "layer": classify_module(&module.key),
            "outbound_dependencies": outbound,
            "inbound_dependencies": inbound,
            "locality_risk": round2(risk),
            "locality_score": round2(100.0 - risk),
            "signals": if risk > 0.0 { vec!["dependency spread"] } else { vec!["stable"] },
            "risk_model_id": MODEL_ID,
            "risk_model_version": MODEL_VERSION,
            "risk_calibration": "locality",
        }));
    }
    Ok(Value::Array(rows))
}

fn leverage(config: &LensConfig) -> Result<Value> {
    let graph = module_graph(config)?;
    let mut rows = Vec::new();
    for module in graph.modules.values() {
        let outbound = graph
            .dependencies
            .get(&module.key)
            .map(HashSet::len)
            .unwrap_or(0);
        let inbound = graph
            .reverse_dependencies
            .get(&module.key)
            .map(HashSet::len)
            .unwrap_or(0);
        let score = (68.0 + inbound as f64 * 2.5 - outbound as f64 * 3.0).clamp(0.0, 100.0);
        rows.push(json!({
            "module_key": module.key,
            "path": module.path,
            "layer": classify_module(&module.key),
            "reach": inbound,
            "outbound_dependencies": outbound,
            "leverage_score": round2(score),
            "pressure_score": round2(100.0 - score),
            "signals": if score >= 68.0 { vec!["high leverage"] } else { vec!["pressure"] },
            "risk_model_id": MODEL_ID,
            "risk_model_version": MODEL_VERSION,
            "risk_calibration": "leverage",
        }));
    }
    Ok(Value::Array(rows))
}

fn architecture_map(config: &LensConfig) -> Result<Value> {
    let graph = module_graph(config)?;
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for module in graph.modules.values() {
        let outbound = graph
            .dependencies
            .get(&module.key)
            .map(HashSet::len)
            .unwrap_or(0);
        let inbound = graph
            .reverse_dependencies
            .get(&module.key)
            .map(HashSet::len)
            .unwrap_or(0);
        let sloc = fs::read_to_string(&module.path)
            .unwrap_or_default()
            .lines()
            .filter(|line| !line.trim().is_empty() && !line.trim().starts_with("//"))
            .count();
        let maintainability = round2(
            (sloc as f64 * 0.12
                + module.public_api_count as f64 * 2.5
                + outbound as f64 * 4.0
                + inbound as f64)
                .min(400.0),
        );
        let architectural = round2((outbound as f64 * 10.0 + inbound as f64 * 8.0).min(300.0));
        nodes.push(json!({
            "data": {
                "id": module.key,
                "label": module.key,
                "path": normalize_slashes(&module.path),
                "layer": classify_module(&module.key),
                "layer_color": layer_color(&classify_module(&module.key)),
                "sloc": sloc,
                "public_api_count": module.public_api_count,
                "outbound_dependencies": outbound,
                "inbound_dependencies": inbound,
                "maintainability_risk": maintainability,
                "change_risk": null,
                "performance_risk": 0.0,
                "correctness_risk": null,
                "quality_risk": maintainability,
                "architectural_risk": architectural,
                "total_score": null,
                "unknown_metrics": ["change_risk", "correctness_risk", "total_score"],
            }
        }));
        if let Some(targets) = graph.dependencies.get(&module.key) {
            for target in targets {
                edges.push(json!({"data": {"source": module.key, "target": target}}));
            }
        }
    }
    Ok(json!({
        "meta": {
            "project_name": config.project_name,
            "source_roots": config.source_roots,
            "risk_model_id": MODEL_ID,
            "risk_model_version": MODEL_VERSION,
            "risk_model_weights": risk_model_weights(),
            "risk_model_tool_scores": risk_model_tool_scores(),
            "risk_model_classification": risk_model_classification(),
            "layer_ruleset": {"id": RULESET_ID, "version": RULESET_VERSION},
        },
        "graph": {
            "nodes": nodes,
            "edges": edges,
        },
        "modules": graph.modules.keys().collect::<Vec<_>>(),
        "measurement_confidence": source_confidence(&config.source_roots, &graph.facts),
    }))
}

#[derive(Clone)]
struct ModuleInfo {
    key: String,
    path: PathBuf,
    public_api_count: usize,
}

struct ModuleGraph {
    facts: Vec<FileFacts>,
    modules: BTreeMap<String, ModuleInfo>,
    dependencies: HashMap<String, HashSet<String>>,
    reverse_dependencies: HashMap<String, HashSet<String>>,
}

fn module_graph(config: &LensConfig) -> Result<ModuleGraph> {
    let facts = rust_facts_for_paths(config, &config.source_roots)?;
    let mut modules = BTreeMap::new();
    for fact in &facts {
        if fact.parse_status == "ok" {
            modules.insert(
                fact.module_key.clone(),
                ModuleInfo {
                    key: fact.module_key.clone(),
                    path: PathBuf::from(&fact.path),
                    public_api_count: fact.public_api_count,
                },
            );
        }
        for module_file in &fact.module_files {
            modules
                .entry(module_file.module_key.clone())
                .or_insert(ModuleInfo {
                    key: module_file.module_key.clone(),
                    path: PathBuf::from(&module_file.path),
                    public_api_count: 0,
                });
        }
    }
    let keys: Vec<String> = modules.keys().cloned().collect();
    let mut dependencies: HashMap<String, HashSet<String>> = HashMap::new();
    for fact in &facts {
        let entry = dependencies.entry(fact.module_key.clone()).or_default();
        for child in &fact.child_modules {
            if modules.contains_key(child) && child != &fact.module_key {
                entry.insert(child.clone());
            }
        }
        for raw in &fact.dependencies {
            if let Some(target) = resolve_dependency(raw, &fact.module_key, &keys) {
                if target != fact.module_key {
                    entry.insert(target);
                }
            }
        }
    }
    let mut reverse_dependencies: HashMap<String, HashSet<String>> = HashMap::new();
    for (source, targets) in &dependencies {
        for target in targets {
            reverse_dependencies
                .entry(target.clone())
                .or_default()
                .insert(source.clone());
        }
    }
    Ok(ModuleGraph {
        facts,
        modules,
        dependencies,
        reverse_dependencies,
    })
}

fn resolve_dependency(raw: &str, source_module: &str, modules: &[String]) -> Option<String> {
    let mut raw = raw.trim().trim_end_matches("::*").to_string();
    if raw.is_empty() {
        return None;
    }
    if let Some(stripped) = raw.strip_prefix("crate::") {
        raw = stripped.to_string();
    } else if let Some(stripped) = raw.strip_prefix("self::") {
        raw = format!("{source_module}::{stripped}");
    } else if raw.starts_with("super::") {
        let mut base: Vec<&str> = source_module.split("::").collect();
        while raw.starts_with("super::") {
            base.pop();
            raw = raw.trim_start_matches("super::").to_string();
        }
        raw = base
            .into_iter()
            .chain(std::iter::once(raw.as_str()))
            .collect::<Vec<_>>()
            .join("::");
    }
    let parts: Vec<&str> = raw.split("::").filter(|part| !part.is_empty()).collect();
    for end in (1..=parts.len()).rev() {
        let candidate = parts[..end].join("::");
        if modules.iter().any(|module| module == &candidate) {
            return Some(candidate);
        }
    }
    None
}

#[derive(Clone)]
struct Provenance {
    measured_at: String,
    command: String,
    host: String,
}

fn provenance() -> Provenance {
    Provenance {
        measured_at: Utc::now().to_rfc3339(),
        command: env::args().collect::<Vec<_>>().join(" "),
        host: hostname::get()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
    }
}

#[derive(Clone)]
struct ToolScoreMetadata {
    risk_model_id: &'static str,
    risk_model_version: u64,
    risk_calibration: &'static str,
}

fn tool_score_metadata(tool: &'static str) -> ToolScoreMetadata {
    ToolScoreMetadata {
        risk_model_id: MODEL_ID,
        risk_model_version: MODEL_VERSION,
        risk_calibration: tool,
    }
}

fn source_confidence(paths: &[String], facts: &[FileFacts]) -> Value {
    let files = iter_rust_files(paths);
    let mut missing = Vec::new();
    if files.is_empty() {
        missing.push("no Rust source files matched the configured paths");
    }
    if !files.is_empty() && facts.is_empty() {
        missing.push("Rust syntax fact extraction returned no files");
    }
    let unsupported: Vec<String> = facts
        .iter()
        .filter(|fact| fact.parse_status != "ok")
        .map(|fact| format!("{}: {}", fact.path, fact.parse_status))
        .chain(
            facts
                .iter()
                .flat_map(|fact| fact.unsupported_patterns.clone()),
        )
        .collect();
    let complete = missing.is_empty() && unsupported.is_empty();
    json!({
        "complete": complete,
        "partial": !complete,
        "missing_input": missing,
        "stale_input": [],
        "unsupported_pattern": unsupported,
    })
}

fn iter_rust_files(paths: &[String]) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut files = Vec::new();
    for raw_path in paths {
        let path = PathBuf::from(raw_path);
        if path.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
            if seen.insert(absolutize(&path)) {
                files.push(path);
            }
        } else if path.is_dir() {
            for entry in WalkDir::new(&path).into_iter().filter_map(Result::ok) {
                let entry_path = entry.path();
                if entry_path.is_file() && entry_path.extension().is_some_and(|ext| ext == "rs") {
                    if seen.insert(absolutize(entry_path)) {
                        files.push(entry_path.to_path_buf());
                    }
                }
            }
        }
    }
    files
}

fn correctness_paths(config: &LensConfig) -> Vec<String> {
    let mut paths = config.source_roots.clone();
    for extra in ["tests", "benches", "examples"] {
        if Path::new(extra).exists() {
            paths.push(extra.to_string());
        }
    }
    let cargo = Path::new("Cargo.toml");
    if let Ok(text) = fs::read_to_string(cargo) {
        let path_re = Regex::new(r#"(?m)^\s*path\s*=\s*"([^"]+\.rs)""#).unwrap();
        for cap in path_re.captures_iter(&text) {
            paths.push(cap[1].to_string());
        }
    }
    dedupe(paths)
}

#[derive(Clone)]
struct TestStatus {
    status: String,
    duration: Option<f64>,
    stdout_tail: Option<String>,
    stderr_tail: Option<String>,
}

fn run_tests() -> Result<HashMap<String, TestStatus>> {
    let started = Instant::now();
    let output = Command::new("cargo").arg("test").output()?;
    let duration = started.elapsed().as_secs_f64();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut statuses = parse_cargo_test_statuses(&stdout);
    statuses.insert(
        "__run__".to_string(),
        TestStatus {
            status: if output.status.success() {
                "passed"
            } else {
                "failed"
            }
            .to_string(),
            duration: Some(duration),
            stdout_tail: Some(tail(&stdout, 40)),
            stderr_tail: Some(tail(&stderr, 40)),
        },
    );
    Ok(statuses)
}

fn parse_cargo_test_statuses(stdout: &str) -> HashMap<String, TestStatus> {
    let running_re =
        Regex::new(r#"\s*Running\s+(?:(?:unittests|tests?)\s+)?(.+?\.rs)(?:\s+\(|$)"#).unwrap();
    let status_re = Regex::new(r"test\s+(.+?)\s+\.\.\.\s+(ok|FAILED|ignored)").unwrap();
    let mut current_path: Option<String> = None;
    let mut statuses = HashMap::new();
    for line in stdout.lines() {
        if let Some(cap) = running_re.captures(line) {
            current_path = Some(cap[1].trim_matches('"').replace('\\', "/"));
            continue;
        }
        if let Some(cap) = status_re.captures(line.trim()) {
            let name = cap[1].to_string();
            let status = match &cap[2] {
                "ok" => "passed",
                "FAILED" => "failed",
                "ignored" => "skipped",
                _ => "unknown",
            }
            .to_string();
            let record = TestStatus {
                status,
                duration: None,
                stdout_tail: None,
                stderr_tail: None,
            };
            statuses.insert(name.clone(), record.clone());
            if let Some(path) = &current_path {
                statuses.insert(format!("{path}::{name}"), record);
            }
        }
    }
    statuses
}

fn type_risk(
    ty: &TypeFact,
    method_count: usize,
    impl_block_count: usize,
    impl_file_count: usize,
) -> (f64, Vec<String>) {
    let field_pressure = over_free(ty.field_count as f64, 8.0, 2.5, 35.0);
    let variant_pressure = over_free(ty.variant_count as f64, 8.0, 1.8, 28.0);
    let payload_pressure = over_free(ty.variant_field_count as f64, 10.0, 1.4, 18.0);
    let method_pressure = over_free(method_count as f64, 12.0, 0.85, 32.0);
    let impl_spread_pressure = ((impl_file_count.saturating_sub(2) as f64 * 4.5)
        + (impl_block_count.saturating_sub(4) as f64 * 1.2))
        .min(24.0);
    let declaration_pressure = over_free(ty.declaration_span as f64, 30.0, 0.35, 12.0);
    let risk = round2(
        (field_pressure
            + variant_pressure
            + payload_pressure
            + method_pressure
            + impl_spread_pressure
            + declaration_pressure)
            .clamp(0.0, 100.0),
    );
    let mut signals = Vec::new();
    if ty.kind == "struct" && ty.shape == "tuple" && ty.field_count >= 8 {
        signals.push(format!("wide tuple struct {} fields", ty.field_count));
    } else if ty.kind == "struct" && ty.field_count >= 16 {
        signals.push(format!("wide struct {} fields", ty.field_count));
    }
    if ty.kind == "struct" && ty.shape == "unit" {
        signals.push("unit struct".to_string());
    }
    if ty.variant_count >= 12 {
        signals.push(format!("large enum {} variants", ty.variant_count));
    }
    if ty.variant_field_count >= 12 {
        signals.push(format!(
            "enum payload surface {} fields",
            ty.variant_field_count
        ));
    }
    if method_count >= 20 {
        signals.push(format!("broad method surface {method_count}"));
    }
    if impl_file_count >= 4 {
        signals.push(format!("impl spread {impl_file_count} files"));
    }
    if impl_block_count >= 6 {
        signals.push(format!("many impl blocks {impl_block_count}"));
    }
    if ty.declaration_span >= 45 {
        signals.push(format!("large declaration {} lines", ty.declaration_span));
    }
    if signals.is_empty() {
        signals.push("stable".to_string());
    }
    (risk, signals)
}

fn escape_weights() -> BTreeMap<&'static str, f64> {
    BTreeMap::from([
        ("unsafe_block", 10.0),
        ("unsafe_fn", 10.0),
        ("unsafe_impl", 10.0),
        ("unsafe_trait", 10.0),
        ("extern_block", 8.0),
        ("extern_fn", 7.0),
        ("static_mut", 14.0),
        ("union", 12.0),
        ("raw_borrow", 6.0),
        ("asm_macro", 14.0),
        ("transmute", 12.0),
        ("maybe_uninit", 5.0),
        ("deref_impl", 4.0),
        ("deref_mut_impl", 5.0),
        ("glob_import", 2.0),
        ("container_ref_return", 3.0),
        ("repr_escape", 5.0),
        ("linkage_escape", 8.0),
        ("clippy_suppression", 3.0),
        ("lint_suppression", 2.0),
    ])
}

fn escape_labels() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        ("unsafe_block", "unsafe block"),
        ("unsafe_fn", "unsafe fn"),
        ("unsafe_impl", "unsafe impl"),
        ("unsafe_trait", "unsafe trait"),
        ("extern_block", "extern block"),
        ("extern_fn", "extern fn"),
        ("static_mut", "mutable static"),
        ("union", "union"),
        ("raw_borrow", "raw borrow"),
        ("asm_macro", "inline assembly"),
        ("transmute", "transmute"),
        ("maybe_uninit", "MaybeUninit"),
        ("deref_impl", "Deref impl"),
        ("deref_mut_impl", "DerefMut impl"),
        ("glob_import", "glob import"),
        ("container_ref_return", "container ref return"),
        ("repr_escape", "layout repr"),
        ("linkage_escape", "linkage attribute"),
        ("clippy_suppression", "Clippy suppression"),
        ("lint_suppression", "lint suppression"),
    ])
}

fn risk_model_weights() -> Value {
    json!({
        "maintainability": {
            "complexity": {"weight": 1.0},
            "sloc": {"weight": 0.12, "cap": 70.0},
            "public_api": {"weight": 2.5, "cap": 30.0},
            "dependencies": {"outbound_weight": 4.0, "inbound_weight": 1.0, "cap": 35.0}
        },
        "change": {"missing_test_evidence": 90.0},
        "correctness": {"any_failed_tests": 140.0, "missing_test_evidence": 90.0},
        "performance": {"perf_score": {"weight": 1.0}},
        "architectural": {
            "outbound_dependencies": {"weight": 10.0, "cap": 120.0},
            "inbound_dependencies": {"weight": 8.0, "cap": 120.0},
            "layer_violations": {"weight": 32.0, "cap": 120.0}
        }
    })
}

fn risk_model_tool_scores() -> Value {
    json!({
        "escape_hatches": {"weights": escape_weights()},
        "type_health": {"score_cap": 100.0},
        "locality": {"score_cap": 100.0},
        "leverage": {"score_cap": 100.0},
        "hotspots": {"quality_multiplier": 1.12}
    })
}

fn risk_model_classification() -> Value {
    json!({
        "warn_total_score": 300.0,
        "bad_total_score": 600.0,
        "warn_color_score": 350.0,
        "bad_color_score": 700.0
    })
}

fn classify_path(path: impl AsRef<str>) -> &'static str {
    let path = path.as_ref().replace('\\', "/").to_lowercase();
    let segments: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    for (layer, needles) in [
        (
            "Interface",
            vec![
                "api",
                "cli",
                "controller",
                "controllers",
                "handler",
                "handlers",
                "http",
                "route",
                "routes",
                "ui",
                "web",
            ],
        ),
        (
            "Application",
            vec![
                "app",
                "application",
                "command",
                "commands",
                "service",
                "services",
                "use_case",
                "use_cases",
                "workflow",
                "workflows",
            ],
        ),
        (
            "Domain",
            vec!["core", "domain", "entity", "entities", "model", "models"],
        ),
        (
            "Infrastructure",
            vec![
                "adapter",
                "adapters",
                "database",
                "db",
                "file",
                "fs",
                "infra",
                "infrastructure",
                "io",
                "persistence",
                "repository",
                "storage",
            ],
        ),
        ("Tests", vec!["spec", "specs", "test", "tests"]),
    ] {
        if needles
            .iter()
            .any(|needle| segments.iter().any(|segment| segment == needle))
        {
            return layer;
        }
    }
    "Unclassified"
}

fn classify_module(module: &str) -> &'static str {
    classify_path(module.replace("::", "/"))
}

fn layer_color(layer: &str) -> &'static str {
    match layer {
        "Interface" => "#569cd6",
        "Application" => "#d7ba7d",
        "Domain" => "#4ec9b0",
        "Infrastructure" => "#c586c0",
        "Tests" => "#9cdcfe",
        _ => "#808080",
    }
}

fn module_for_path(path: &str) -> String {
    let path = normalize_slashes(path);
    if let Some(src_path) = path.strip_prefix("src/") {
        src_path
            .trim_end_matches(".rs")
            .trim_end_matches("/mod")
            .replace('/', "::")
    } else {
        Path::new(&path)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
    }
}

fn test_kind_for_path(path: &str) -> &'static str {
    let normalized = normalize_slashes(path);
    if normalized.starts_with("tests/") {
        "integration"
    } else if normalized.starts_with("benches/") {
        "benchmark"
    } else if normalized.starts_with("examples/") {
        "example"
    } else if normalized.starts_with("src/bin/") {
        "binary"
    } else {
        "inline"
    }
}

fn title_from_name(name: &str) -> String {
    let stop = ["test", "should", "when"];
    let tokens: Vec<&str> = name
        .split(|ch: char| ch == '_' || !ch.is_alphanumeric())
        .filter(|token| !token.is_empty() && !stop.contains(token))
        .collect();
    let useful = if tokens.is_empty() {
        vec!["behavior"]
    } else {
        tokens
    };
    format!(
        "Checks {}.",
        useful.into_iter().take(7).collect::<Vec<_>>().join(" ")
    )
}

fn write_json(path: &Path, payload: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(payload)?),
    )?;
    Ok(())
}

fn repo_root() -> Result<PathBuf> {
    Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

fn resolve_config_path(path: PathBuf, config_dir: &Path) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        absolutize(&config_dir.join(path))
    }
}

fn resolve_project_path(path: PathBuf, project_root: &Path) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        absolutize(&project_root.join(path))
    }
}

fn absolutize(path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

fn normalize_slashes(path: impl AsRef<Path>) -> String {
    path.as_ref().to_string_lossy().replace('\\', "/")
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn over_free(value: f64, free: f64, weight: f64, cap: f64) -> f64 {
    ((value - free).max(0.0) * weight).min(cap)
}

fn count_matches(text: &str, needles: &[&str]) -> usize {
    needles
        .iter()
        .map(|needle| text.matches(needle).count())
        .sum()
}

fn stable_hash(value: &str) -> String {
    let mut hash: u64 = 1469598103934665603;
    for byte in value.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    format!("{hash:016x}")
}

fn tail(text: &str, lines: usize) -> String {
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

fn dedupe(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}
