use crate::{config::LensConfig, tool::MeasureTool};
use anyhow::Result;
use serde_json::{Value, json};

pub(crate) fn print_catalog(config: &LensConfig) -> Result<()> {
    let tasks: Vec<Value> = task_definitions()
        .into_iter()
        .map(|task| {
            let output_files = if matches!(task.tool, MeasureTool::Correctness) {
                vec!["correctness_review.json", "test_catalog.json"]
            } else {
                vec![task.tool.output_file()]
            };
            json!({
                "id": task.id,
                "category": task.category,
                "subcategory": task.subcategory,
                "title": task.title,
                "description": task.description,
                "commands": [["rqlens", "measure", task.tool.name()]],
                "output_artifacts": output_files.iter().map(|file| format!("target/analysis/{file}")).collect::<Vec<_>>(),
                "absolute_output_artifacts": output_files.iter().map(|file| config.output_dir.join(file).to_string_lossy().to_string()).collect::<Vec<_>>(),
                "depends_on": task.dependencies(),
                "expensive": task.expensive(),
                "supports_individual_run": true,
                "lens": "rust-quality-lens",
                "tool": task.tool.name(),
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
            "tasks": tasks,
            "blocking_rule_contracts": serde_json::from_str::<Value>(include_str!("../contracts/blocking-rules.json"))?
        }))?
    );
    Ok(())
}

struct TaskDefinition {
    tool: MeasureTool,
    id: &'static str,
    aliases: Vec<&'static str>,
    category: &'static str,
    subcategory: &'static str,
    title: &'static str,
    description: &'static str,
}

impl TaskDefinition {
    fn dependencies(&self) -> &[&str] {
        match self.tool {
            MeasureTool::TestQuality => &["correctness.catalog"],
            MeasureTool::FunctionRisk => &["quality.hotspots", "correctness.coverage"],
            MeasureTool::Map => &[
                "quality.hotspots",
                "correctness.catalog",
                "quality.locality_dynamic",
                "quality.locality_leverage",
            ],
            _ => &[],
        }
    }

    fn expensive(&self) -> bool {
        matches!(
            self.tool,
            MeasureTool::SemanticApi
                | MeasureTool::SourceExpansion
                | MeasureTool::Practices
                | MeasureTool::CorrectnessRun
                | MeasureTool::Coverage
        )
    }
}

fn task_definitions() -> Vec<TaskDefinition> {
    vec![
        TaskDefinition {
            tool: MeasureTool::Hotspots,
            id: "quality.hotspots",
            aliases: vec![],
            category: "quality",
            subcategory: "hotspots",
            title: "Hotspots",
            description: "Reports standard cyclomatic and cognitive complexity, then ranks function and module branch, path, nesting, and size pressure using RQLens-specific proxies.",
        },
        TaskDefinition {
            tool: MeasureTool::Clones,
            id: "quality.clones",
            aliases: vec![],
            category: "quality",
            subcategory: "clones",
            title: "Clones",
            description: "Finds repeated token and AST-like code structures.",
        },
        TaskDefinition {
            tool: MeasureTool::EscapeHatches,
            id: "quality.escape_hatches",
            aliases: vec!["quality.escape-hatches"],
            category: "quality",
            subcategory: "safety",
            title: "Rust Escape Hatches",
            description: "Tracks unsafe, FFI, raw memory, globals, glob imports, and lint suppressions.",
        },
        TaskDefinition {
            tool: MeasureTool::Reliability,
            id: "quality.reliability",
            aliases: vec!["quality.safety-contracts"],
            category: "quality",
            subcategory: "safety",
            title: "Safety and Reliability Findings",
            description: "Reports undocumented unsafe contracts and explicit panic paths as source evidence.",
        },
        TaskDefinition {
            tool: MeasureTool::ArchitectureRules,
            id: "quality.architecture_rules",
            aliases: vec!["quality.boundaries"],
            category: "quality",
            subcategory: "architecture",
            title: "Architecture Rules",
            description: "Evaluates configured Cargo-qualified module dependency boundaries.",
        },
        TaskDefinition {
            tool: MeasureTool::TestQuality,
            id: "quality.test_quality",
            aliases: vec![],
            category: "correctness",
            subcategory: "tests",
            title: "Test Quality",
            description: "Reports assertion, system-under-test call, and ignored-test evidence.",
        },
        TaskDefinition {
            tool: MeasureTool::ApiHealth,
            id: "quality.api_health",
            aliases: vec!["quality.documentation"],
            category: "quality",
            subcategory: "api",
            title: "API Documentation Health",
            description: "Measures public-item and crate-level documentation evidence without treating coverage as correctness.",
        },
        TaskDefinition {
            tool: MeasureTool::SemanticApi,
            id: "quality.semantic_api",
            aliases: vec!["quality.public_api"],
            category: "quality",
            subcategory: "api",
            title: "Reachable Public API",
            description: "Optionally inventories compiler-derived reachable public API through cargo-public-api.",
        },
        TaskDefinition {
            tool: MeasureTool::SourceExpansion,
            id: "quality.source_expansion",
            aliases: vec![],
            category: "quality",
            subcategory: "semantics",
            title: "Source Expansion Observations",
            description: "Opt-in item/impl macro expansion evidence; does not certify cfg, generated facts or compiler ownership. Not included in measure all.",
        },
        TaskDefinition {
            tool: MeasureTool::TypeHealth,
            id: "quality.type_health",
            aliases: vec!["quality.type-health"],
            category: "quality",
            subcategory: "structure",
            title: "Type Health",
            description: "Ranks wide structs, large enums, broad method surfaces, and impl spread.",
        },
        TaskDefinition {
            tool: MeasureTool::Practices,
            id: "quality.rust_practices",
            aliases: vec!["quality.verification"],
            category: "quality",
            subcategory: "standards",
            title: "Rust Practices",
            description: "Runs official Rust gates and records project maturity evidence without mixing them into architecture scores.",
        },
        TaskDefinition {
            tool: MeasureTool::Locality,
            id: "quality.locality_dynamic",
            aliases: vec!["quality.locality"],
            category: "quality",
            subcategory: "locality",
            title: "Code Locality",
            description: "Measures dependency spread, hidden coupling, interface explicitness, and change locality.",
        },
        TaskDefinition {
            tool: MeasureTool::Leverage,
            id: "quality.locality_leverage",
            aliases: vec!["quality.leverage"],
            category: "quality",
            subcategory: "leverage",
            title: "Architecture Leverage",
            description: "Measures reach, invariant surface, divergence pressure, and co-change ripple.",
        },
        TaskDefinition {
            tool: MeasureTool::ModuleCohesion,
            id: "quality.module_cohesion",
            aliases: vec!["quality.change_entropy"],
            category: "quality",
            subcategory: "architecture",
            title: "Module Cohesion and Change Entropy",
            description: "Reports responsibility spread, dependency pressure, and Git change entropy without treating them as defect predictions.",
        },
        TaskDefinition {
            tool: MeasureTool::Correctness,
            id: "correctness.catalog",
            aliases: vec![],
            category: "correctness",
            subcategory: "tests",
            title: "Correctness Catalog",
            description: "Discovers Rust tests and groups them by generic architecture area.",
        },
        TaskDefinition {
            tool: MeasureTool::CorrectnessRun,
            id: "correctness.all",
            aliases: vec![],
            category: "correctness",
            subcategory: "tests",
            title: "All Tests",
            description: "Runs the full Rust test suite and attaches status to the correctness catalog.",
        },
        TaskDefinition {
            tool: MeasureTool::Coverage,
            id: "correctness.coverage",
            aliases: vec![],
            category: "correctness",
            subcategory: "coverage",
            title: "Test Coverage",
            description: "Runs cargo-llvm-cov and reports line, region, function, and branch coverage by Rust module.",
        },
        TaskDefinition {
            tool: MeasureTool::FunctionRisk,
            id: "correctness.function_risk",
            aliases: vec!["quality.crap"],
            category: "correctness",
            subcategory: "risk",
            title: "Function Risk",
            description: "Combines function cyclomatic complexity with fresh executable-line coverage using the CRAP formula.",
        },
        TaskDefinition {
            tool: MeasureTool::Map,
            id: "map.architecture",
            aliases: vec![],
            category: "map",
            subcategory: "architecture",
            title: "Architecture Map",
            description: "Builds module health, dependency, and risk map data.",
        },
    ]
}
