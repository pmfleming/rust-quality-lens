use anyhow::Result;
use chrono::Utc;
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

mod artifacts;
mod catalog;
mod config;
mod facts;
mod producers;
mod risk_model;
mod util;

use catalog::print_catalog;
use config::LensConfig;
use facts::{FileFacts, RunContext, TypeFact};
use util::{iter_rust_files, normalize_slashes, over_free, round2, write_json};

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
    let context = RunContext::new(&config, &tools)?;
    for tool in tools {
        let output = config.output_dir.join(tool.output_file());
        let payload = producers::produce_measurement(&tool, &config, &context)?;
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
}

fn option_json(value: Option<f64>) -> Value {
    value.map(Value::from).unwrap_or(Value::Null)
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

fn source_scan_confidence_with_errors(paths: &[String], read_errors: Vec<String>) -> Value {
    let files = iter_rust_files(paths);
    let mut missing = Vec::new();
    if files.is_empty() {
        missing.push("no Rust source files matched the configured paths");
    }
    let complete = missing.is_empty() && read_errors.is_empty();
    json!({
        "complete": complete,
        "partial": !complete,
        "confidence_scope": "source_scan",
        "required_inputs": ["rust_source_files"],
        "observed_inputs": {
            "rust_source_files": files.len(),
        },
        "missing_input": missing,
        "stale_input": [],
        "unsupported_pattern": read_errors,
    })
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
                .flat_map(|fact| fact.graph.unsupported_patterns.clone()),
        )
        .collect();
    let complete = missing.is_empty() && unsupported.is_empty();
    json!({
        "complete": complete,
        "partial": !complete,
        "confidence_scope": "syntax_facts",
        "required_inputs": ["rust_source_files", "rust_syntax_facts"],
        "observed_inputs": {
            "rust_source_files": files.len(),
            "rust_syntax_fact_files": facts.len(),
        },
        "missing_input": missing,
        "stale_input": [],
        "unsupported_pattern": unsupported,
    })
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

const ESCAPE_CALIBRATIONS: &[(&str, &str, f64)] = &[
    ("unsafe_block", "unsafe block", 10.0),
    ("unsafe_fn", "unsafe fn", 10.0),
    ("unsafe_impl", "unsafe impl", 10.0),
    ("unsafe_trait", "unsafe trait", 10.0),
    ("extern_block", "extern block", 8.0),
    ("extern_fn", "extern fn", 7.0),
    ("static_mut", "mutable static", 14.0),
    ("union", "union", 12.0),
    ("raw_borrow", "raw borrow", 6.0),
    ("asm_macro", "inline assembly", 14.0),
    ("transmute", "transmute", 12.0),
    ("maybe_uninit", "MaybeUninit", 5.0),
    ("deref_impl", "Deref impl", 4.0),
    ("deref_mut_impl", "DerefMut impl", 5.0),
    ("glob_import", "glob import", 2.0),
    ("container_ref_return", "container ref return", 3.0),
    ("repr_escape", "layout repr", 5.0),
    ("linkage_escape", "linkage attribute", 8.0),
    ("clippy_suppression", "Clippy suppression", 3.0),
    ("lint_suppression", "lint suppression", 2.0),
];

fn escape_weights() -> BTreeMap<&'static str, f64> {
    ESCAPE_CALIBRATIONS
        .iter()
        .map(|(key, _, weight)| (*key, *weight))
        .collect()
}

fn escape_labels() -> BTreeMap<&'static str, &'static str> {
    ESCAPE_CALIBRATIONS
        .iter()
        .map(|(key, label, _)| (*key, *label))
        .collect()
}

fn risk_model_weights() -> Value {
    json!({
        "maintainability": {
            "complexity": {"weight": 1.0},
            "sloc": {"weight": 0.12, "cap": 70.0},
            "public_api": {"weight": 2.5, "cap": 30.0},
            "dependencies": {"outbound_weight": 4.0, "inbound_weight": 1.0, "cap": 35.0}
        },
        "change": {
            "churn": {"divisor": 12.0, "cap": 160.0},
            "commit_count": {"weight": 2.5, "cap": 100.0},
            "contributor_count": {"weight": 14.0, "cap": 80.0},
            "defect_keyword_commits": {"weight": 18.0, "cap": 90.0},
            "missing_test_evidence": 90.0
        },
        "correctness": {
            "any_failed_tests": 140.0,
            "failed_test_count": {"weight": 45.0, "cap": 120.0},
            "unknown_test_count": {"weight": 4.0, "cap": 80.0},
            "skipped_test_count": {"weight": 10.0, "cap": 40.0},
            "missing_test_evidence": 90.0
        },
        "performance": {
            "benchmark_score": {"weight": 1.0},
            "mean_runtime_ms": {"weight": 2.5, "cap": 120.0},
            "runtime_variance": {"weight": 180.0, "cap": 90.0}
        },
        "quality": {
            "maintainability_risk": {"weight": 1.0},
            "locality_risk": {"weight": 1.0},
            "leverage_pressure": {"weight": 1.0},
            "cap": 600.0
        },
        "architectural": {
            "outbound_dependencies": {"weight": 10.0, "cap": 120.0},
            "inbound_dependencies": {"weight": 8.0, "cap": 120.0},
            "layer_violations": {"weight": 32.0, "cap": 120.0},
            "cycle_membership_penalty": 110.0,
            "large_module": {"threshold_sloc": 250, "penalty": 60.0}
        }
    })
}

fn risk_model_tool_scores() -> Value {
    json!({
        "escape_hatches": {"weights": escape_weights()},
        "type_health": {"score_cap": 100.0},
        "locality": {"score_cap": 100.0},
        "leverage": {"score_cap": 100.0},
        "hotspots": {"quality_multiplier": 1.12},
        "clones_token": {"window_tokens": 50, "minimum_line_span": 5, "instance_weight": 5.0},
        "clones_ast": {"minimum_ast_nodes": 6, "cross_file_factor": 1.5}
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

fn project_relative_path(config: &LensConfig, path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        config.project_root.join(path)
    };
    absolute
        .strip_prefix(&config.project_root)
        .map(normalize_slashes)
        .unwrap_or_else(|_| normalize_slashes(absolute))
}
