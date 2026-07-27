use crate::config::LensConfig;
use anyhow::Result;
use serde_json::{Value, json};

pub(crate) fn print_catalog(config: &LensConfig) -> Result<()> {
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
            description: "Ranks file-level branch, path, size, and maintainability pressure using RQLens-specific proxies.",
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
            tool: "reliability",
            id: "quality.reliability",
            aliases: vec!["quality.safety-contracts"],
            category: "quality",
            subcategory: "safety",
            title: "Safety and Reliability Findings",
            description: "Reports undocumented unsafe contracts and explicit panic paths as source evidence.",
            output_file: "reliability_findings.json",
            depends_on: vec![],
            expensive: false,
        },
        TaskDefinition {
            tool: "api-health",
            id: "quality.api_health",
            aliases: vec!["quality.documentation"],
            category: "quality",
            subcategory: "api",
            title: "API Documentation Health",
            description: "Measures public-item and crate-level documentation evidence without treating coverage as correctness.",
            output_file: "api_health.json",
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
            tool: "practices",
            id: "quality.rust_practices",
            aliases: vec!["quality.verification"],
            category: "quality",
            subcategory: "standards",
            title: "Rust Practices",
            description: "Runs official Rust gates and records project maturity evidence without mixing them into architecture scores.",
            output_file: "rust_practices.json",
            depends_on: vec![],
            expensive: true,
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
            tool: "coverage",
            id: "correctness.coverage",
            aliases: vec![],
            category: "correctness",
            subcategory: "coverage",
            title: "Test Coverage",
            description: "Runs cargo-llvm-cov and reports line, region, function, and branch coverage by Rust module.",
            output_file: "coverage.json",
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
