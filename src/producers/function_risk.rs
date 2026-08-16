use anyhow::Result;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;

use crate::config::LensConfig;
use crate::facts::{FunctionFact, RunContext};
use crate::measurement::project_relative_path;
use crate::util::{project_input_fingerprint, round2};

pub(super) fn produce(config: &LensConfig, context: &RunContext) -> Result<Value> {
    let coverage = load_coverage(config);
    let coverage_index = coverage
        .document
        .as_ref()
        .map(coverage_index)
        .unwrap_or_default();
    let functions = context
        .source_facts
        .iter()
        .filter(|fact| fact.parse_status == "ok")
        .flat_map(|fact| {
            fact.items
                .functions
                .iter()
                .map(|function| function_row(config, function, &fact.module_id, &coverage_index))
        })
        .collect::<Vec<_>>();
    let measured = functions
        .iter()
        .filter(|function| function["crap"].is_number())
        .count();
    Ok(json!({
        "summary": {
            "function_count": functions.len(),
            "coverage_attributed_count": measured,
            "high_crap_count": functions.iter().filter(|function| function["crap"].as_f64().is_some_and(|value| value > 30.0)).count(),
        },
        "formula": "CC^2 * (1 - coverage)^3 + CC",
        "functions": functions,
        "measurement_confidence": {
            "complete": coverage.complete,
            "partial": !coverage.complete,
            "confidence_scope": "function_complexity_and_coverage",
            "required_inputs": ["rust_syntax_facts", "fresh_coverage"],
            "observed_inputs": {
                "rust_function_count": context.source_facts.iter().map(|fact| fact.items.functions.len()).sum::<usize>(),
                "coverage_attributed_count": measured,
            },
            "missing_input": coverage.missing,
            "stale_input": coverage.stale,
            "unsupported_pattern": [],
        }
    }))
}

struct CoverageLoad {
    document: Option<Value>,
    complete: bool,
    missing: Vec<String>,
    stale: Vec<String>,
}

fn load_coverage(config: &LensConfig) -> CoverageLoad {
    let path = config.output_dir.join("coverage.json");
    let Ok(contents) = fs::read_to_string(path) else {
        return CoverageLoad {
            document: None,
            complete: false,
            missing: vec!["coverage.json is missing".to_string()],
            stale: Vec::new(),
        };
    };
    let Ok(document) = serde_json::from_str::<Value>(&contents) else {
        return CoverageLoad {
            document: None,
            complete: false,
            missing: vec!["coverage.json is malformed".to_string()],
            stale: Vec::new(),
        };
    };
    let current = project_input_fingerprint(&config.project_root, &config.source_roots);
    let stale = document["input_fingerprint"]["digest"] != current["digest"];
    let coverage_complete = document["measurement_confidence"]["complete"] == true;
    CoverageLoad {
        document: Some(document),
        complete: coverage_complete && !stale,
        missing: if coverage_complete {
            Vec::new()
        } else {
            vec!["coverage measurement is incomplete".to_string()]
        },
        stale: if stale {
            vec!["coverage.json input fingerprint is stale".to_string()]
        } else {
            Vec::new()
        },
    }
}

fn coverage_index(document: &Value) -> BTreeMap<String, BTreeMap<usize, bool>> {
    document["data"]["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|file| {
            let path = file["path"].as_str()?.replace('\\', "/");
            let hits = file["line_hits"]
                .as_array()?
                .iter()
                .filter_map(|hit| Some((hit["line"].as_u64()? as usize, hit["covered"].as_bool()?)))
                .collect();
            Some((path, hits))
        })
        .collect()
}

fn function_row(
    config: &LensConfig,
    function: &FunctionFact,
    module_id: &str,
    coverage: &BTreeMap<String, BTreeMap<usize, bool>>,
) -> Value {
    let path = project_relative_path(&config.project_root, &function.path);
    let hits = coverage.get(&path);
    let executable = hits
        .map(|hits| hits.range(function.start_line..=function.end_line).count())
        .unwrap_or_default();
    let covered = hits
        .map(|hits| {
            hits.range(function.start_line..=function.end_line)
                .filter(|(_, covered)| **covered)
                .count()
        })
        .unwrap_or_default();
    let coverage_percent = (executable > 0).then(|| covered as f64 / executable as f64 * 100.0);
    let crap =
        coverage_percent.map(|coverage| crap_score(function.cyclomatic_complexity, coverage));
    json!({
        "name": function.qualified_name,
        "module_key": function.module_key,
        "module_id": module_id,
        "path": path,
        "start_line": function.start_line,
        "end_line": function.end_line,
        "cyclomatic_complexity": function.cyclomatic_complexity,
        "cognitive_complexity": function.cognitive_complexity,
        "executable_lines": executable,
        "covered_lines": covered,
        "coverage_percent": coverage_percent.map(round2),
        "crap": crap,
        "risk_level": crap.map_or("unknown", |value| if value > 30.0 { "high" } else { "normal" }),
    })
}

fn crap_score(complexity: usize, coverage_percent: f64) -> f64 {
    let complexity = complexity as f64;
    let uncovered = 1.0 - (coverage_percent / 100.0).clamp(0.0, 1.0);
    round2(complexity.powi(2) * uncovered.powi(3) + complexity)
}

#[cfg(test)]
mod tests {
    use super::crap_score;

    #[test]
    fn crap_collapses_to_complexity_at_full_coverage() {
        assert_eq!(crap_score(12, 100.0), 12.0);
        assert_eq!(crap_score(12, 0.0), 156.0);
    }
}
