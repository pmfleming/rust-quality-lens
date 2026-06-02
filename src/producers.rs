use anyhow::Result;
use regex::Regex;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;

use crate::artifacts::{
    MapArtifacts, cycle_modules, git_history_facts, is_layer_violation, json_f64, layer_violations,
    map_measurement_confidence,
};
use crate::config::LensConfig;
use crate::facts::{
    AstCloneFact, ModuleGraph, RunContext, ast_clone_facts_for_paths, module_graph, run_tests,
};
use crate::risk_model::{ArchitectureRiskInputs, architecture_risk_scores};
use crate::util::{iter_rust_files, normalize_slashes, round2, stable_hash};
use crate::{
    MODEL_ID, MODEL_VERSION, MeasureTool, RULESET_ID, RULESET_VERSION, classify_module,
    classify_path, escape_labels, escape_weights, layer_color, module_for_path, option_json,
    project_relative_path, provenance, risk_model_classification, risk_model_tool_scores,
    risk_model_weights, source_confidence, source_scan_confidence_with_errors, test_kind_for_path,
    title_from_name, tool_score_metadata, type_risk,
};

pub(crate) fn produce_measurement(
    tool: &MeasureTool,
    config: &LensConfig,
    context: &RunContext,
) -> Result<Value> {
    match tool {
        MeasureTool::Hotspots => hotspots(config, context),
        MeasureTool::Clones => clones(config, context),
        MeasureTool::EscapeHatches => escape_hatches(config, context),
        MeasureTool::TypeHealth => type_health(config, context),
        MeasureTool::Correctness => correctness(config, context, false),
        MeasureTool::CorrectnessRun => correctness(config, context, true),
        MeasureTool::Locality => locality(config, context),
        MeasureTool::Leverage => leverage(config, context),
        MeasureTool::Map => architecture_map(config, context),
        MeasureTool::All => unreachable!(),
    }
}
fn escape_hatches(config: &LensConfig, context: &RunContext) -> Result<Value> {
    let facts = &context.source_facts;
    let confidence = source_confidence(&config.source_roots, &facts);
    let weights = escape_weights();
    let labels = escape_labels();
    let metadata = tool_score_metadata("escape_hatches");
    let provenance = provenance();
    let mut rows = Vec::new();
    for fact in facts {
        let mut counts = BTreeMap::new();
        for key in weights.keys() {
            counts.insert(
                key.to_string(),
                *fact.escapes.escape_counts.get(*key).unwrap_or(&0),
            );
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
            .escapes
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
        json_f64(b, "escape_hatch_score")
            .total_cmp(&json_f64(a, "escape_hatch_score"))
            .then_with(|| b["total_count"].as_u64().cmp(&a["total_count"].as_u64()))
            .then_with(|| a["module_key"].as_str().cmp(&b["module_key"].as_str()))
    });
    Ok(Value::Array(rows))
}

fn type_health(config: &LensConfig, context: &RunContext) -> Result<Value> {
    let facts = &context.source_facts;
    let confidence = source_confidence(&config.source_roots, &facts);
    let metadata = tool_score_metadata("type_health");
    let provenance = provenance();
    let mut impls: HashMap<(String, String), (usize, usize, BTreeSet<String>)> = HashMap::new();
    for fact in facts {
        for imp in &fact.items.impls {
            let entry = impls
                .entry((imp.module_key.clone(), imp.type_name.clone()))
                .or_insert((0, 0, BTreeSet::new()));
            entry.0 += imp.method_count;
            entry.1 += 1;
            entry.2.insert(imp.path.clone());
        }
    }
    let mut rows = Vec::new();
    for fact in facts {
        if fact.parse_status != "ok" {
            continue;
        }
        for ty in &fact.items.types {
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
        json_f64(b, "structural_risk")
            .total_cmp(&json_f64(a, "structural_risk"))
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

fn correctness(config: &LensConfig, context: &RunContext, run: bool) -> Result<Value> {
    let paths = &context.correctness_paths;
    let facts = &context.correctness_facts;
    let confidence = source_confidence(&paths, &facts);
    let statuses = if run {
        run_tests(config)?
    } else {
        HashMap::new()
    };
    let mut tests = Vec::new();
    for fact in facts {
        if fact.parse_status != "ok" {
            continue;
        }
        for test in &fact.items.tests {
            let path = project_relative_path(config, &test.path);
            let name = test.name.clone();
            let qualified_name = test.qualified_name.clone();
            let id = format!("{path}::{qualified_name}");
            let mut last_status = "unknown".to_string();
            let mut last_duration = Value::Null;
            for key in [
                &id,
                &format!("{path}::{qualified_name}"),
                &format!("{}::{}", test.module_key, test.name),
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
                "layer": classify_path(&path),
                "module": if test.module_key.is_empty() { module_for_path(&test.path) } else { test.module_key.clone() },
                "description": title_from_name(&test.name),
                "kind": test_kind_for_path(&path),
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

fn hotspots(config: &LensConfig, context: &RunContext) -> Result<Value> {
    let facts = &context.source_facts;
    let confidence = source_confidence(&config.source_roots, facts);
    let mut rows = Vec::new();
    for fact in facts {
        if fact.parse_status != "ok" {
            continue;
        }
        let sloc = fact.source.source_nonblank_line_count as f64;
        let cognitive = fact.source.cognitive_complexity as f64;
        let cyclomatic = fact.source.cyclomatic_complexity as f64;
        let mi = (100.0 - sloc * 0.15 - cognitive * 1.7).max(0.0);
        let quality_score =
            round2((cognitive * 3.7 + cyclomatic * 2.0 + (65.0 - mi).max(0.0) * 1.2) * 1.12);
        rows.push(json!({
            "name": normalize_slashes(&fact.path),
            "module_key": fact.module_key,
            "kind": "unit",
            "start_line": 1,
            "end_line": fact.source.source_line_count,
            "cognitive": cognitive,
            "cyclomatic": cyclomatic,
            "mi": mi,
            "effort": sloc * cyclomatic * 25.0,
            "sloc": sloc,
            "ploc": sloc,
            "cloc": fact.source.source_comment_line_count,
            "bugs": round2(sloc / 500.0),
            "abc_mag": cognitive + cyclomatic,
            "nom_fn": fact.source.function_count,
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
    rows.sort_by(|a, b| json_f64(b, "score").total_cmp(&json_f64(a, "score")));
    Ok(Value::Array(rows))
}

struct CloneInstance {
    file_path: String,
    start_line: usize,
    end_line: usize,
    snippet: String,
}

const CLONE_WINDOW_TOKENS: usize = 50;
const MIN_CLONE_LINE_SPAN: usize = 5;

fn clones(config: &LensConfig, context: &RunContext) -> Result<Value> {
    let token_re = Regex::new(
        r"[A-Za-z_][A-Za-z0-9_]*|\d+|::|->|=>|==|!=|<=|>=|&&|\|\||[{}\(\)\[\];,.:+\-*/%&|^!<>=?]",
    )?;
    let mut windows: HashMap<String, Vec<CloneInstance>> = HashMap::new();
    let mut read_errors = Vec::new();
    for path in iter_rust_files(&config.source_roots) {
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                read_errors.push(format!("{}: {error}", normalize_slashes(&path)));
                continue;
            }
        };
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
        for window in tokens.windows(CLONE_WINDOW_TOKENS) {
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
            windows.entry(key).or_default().push(CloneInstance {
                file_path: normalize_slashes(&path),
                start_line: start,
                end_line: end,
                snippet,
            });
        }
    }
    let confidence = source_scan_confidence_with_errors(&config.source_roots, read_errors);
    let mut rows = Vec::new();
    for (key, instances) in windows {
        let instances = non_overlapping_clone_instances(instances);
        let file_count = instances
            .iter()
            .map(|item| &item.file_path)
            .collect::<HashSet<_>>()
            .len();
        if instances.len() < 2 {
            continue;
        }
        let max_line_span = instances
            .iter()
            .map(|instance| instance.end_line - instance.start_line + 1)
            .max()
            .unwrap_or(0);
        if max_line_span < MIN_CLONE_LINE_SPAN {
            continue;
        }
        rows.push(json!({
            "engine": "token",
            "hash": stable_hash(&key),
            "token_count": CLONE_WINDOW_TOKENS,
            "instance_count": instances.len(),
            "file_count": file_count,
            "max_line_span": max_line_span,
            "score": round2(instances.len() as f64 * 5.0),
            "signals": if file_count >= 2 { format!("cross-file x{file_count}") } else { "same-file repeat".to_string() },
            "risk_model_id": MODEL_ID,
            "risk_model_version": MODEL_VERSION,
            "risk_calibration": "clones_token",
            "instances": instances.into_iter().map(|instance| json!({
                "file_path": instance.file_path,
                "start_line": instance.start_line,
                "end_line": instance.end_line,
                "snippet": instance.snippet,
            })).collect::<Vec<_>>(),
            "measurement_confidence": confidence,
        }));
    }
    rows.extend(ast_clone_rows(
        config,
        &ast_clone_facts_for_paths(config, &config.source_roots)?,
        source_confidence(&config.source_roots, &context.source_facts),
    ));
    rows.sort_by(|a, b| json_f64(b, "score").total_cmp(&json_f64(a, "score")));
    Ok(Value::Array(rows))
}

fn ast_clone_rows(config: &LensConfig, facts: &[AstCloneFact], confidence: Value) -> Vec<Value> {
    let mut groups: BTreeMap<String, Vec<&AstCloneFact>> = BTreeMap::new();
    for fact in facts {
        groups.entry(fact.ast_hash.clone()).or_default().push(fact);
    }
    groups
        .into_iter()
        .filter_map(|(hash, facts)| ast_clone_row(config, hash, facts, &confidence))
        .collect()
}

fn ast_clone_row(
    config: &LensConfig,
    hash: String,
    facts: Vec<&AstCloneFact>,
    confidence: &Value,
) -> Option<Value> {
    if facts.len() < 2 {
        return None;
    }
    let file_count = facts
        .iter()
        .map(|fact| normalize_slashes(&fact.file))
        .collect::<HashSet<_>>()
        .len();
    let max_line_span = facts
        .iter()
        .map(|fact| fact.end_line.saturating_sub(fact.start_line) + 1)
        .max()
        .unwrap_or(0);
    if max_line_span < MIN_CLONE_LINE_SPAN {
        return None;
    }
    let max_node_count = facts.iter().map(|fact| fact.node_count).max().unwrap_or(0);
    let score =
        round2(facts.len() as f64 * max_line_span as f64 * ast_cross_file_factor(file_count));
    Some(json!({
        "engine": "ast",
        "hash": hash,
        "node_count": max_node_count,
        "instance_count": facts.len(),
        "file_count": file_count,
        "max_line_span": max_line_span,
        "score": score,
        "signals": if file_count >= 2 { format!("ast cross-file x{file_count}") } else { "ast same-file repeat".to_string() },
        "risk_model_id": MODEL_ID,
        "risk_model_version": MODEL_VERSION,
        "risk_calibration": "clones_ast",
        "instances": facts.into_iter().map(|fact| json!({
            "file_path": normalize_slashes(&fact.file),
            "module_path": project_relative_path(config, &fact.file),
            "name": fact.name,
            "qualified_name": fact.qualified_name,
            "start_line": fact.start_line,
            "end_line": fact.end_line,
            "node_count": fact.node_count,
            "snippet": snippet_for_range(&fact.file, fact.start_line, fact.end_line),
        })).collect::<Vec<_>>(),
        "measurement_confidence": confidence,
    }))
}

fn ast_cross_file_factor(file_count: usize) -> f64 {
    if file_count >= 2 { 1.5 } else { 1.0 }
}

fn snippet_for_range(path: &str, start_line: usize, end_line: usize) -> String {
    let Ok(text) = fs::read_to_string(path) else {
        return String::new();
    };
    text.lines()
        .skip(start_line.saturating_sub(1))
        .take(end_line.saturating_sub(start_line) + 1)
        .collect::<Vec<_>>()
        .join("\n")
}

fn non_overlapping_clone_instances(mut instances: Vec<CloneInstance>) -> Vec<CloneInstance> {
    instances.sort_by(|a, b| {
        a.file_path
            .cmp(&b.file_path)
            .then_with(|| a.start_line.cmp(&b.start_line))
            .then_with(|| a.end_line.cmp(&b.end_line))
    });
    let mut retained: Vec<CloneInstance> = Vec::new();
    let mut last_by_file: HashMap<String, usize> = HashMap::new();
    for instance in instances {
        let last_end = last_by_file
            .get(&instance.file_path)
            .copied()
            .unwrap_or_default();
        if instance.start_line <= last_end {
            continue;
        }
        last_by_file.insert(instance.file_path.clone(), instance.end_line);
        retained.push(instance);
    }
    retained
}

struct DependencyCounts {
    outbound: usize,
    inbound: usize,
}

fn dependency_counts(graph: &ModuleGraph, module_key: &str) -> DependencyCounts {
    DependencyCounts {
        outbound: graph
            .dependencies
            .get(module_key)
            .map(HashSet::len)
            .unwrap_or(0),
        inbound: graph
            .reverse_dependencies
            .get(module_key)
            .map(HashSet::len)
            .unwrap_or(0),
    }
}

fn locality(_config: &LensConfig, context: &RunContext) -> Result<Value> {
    let graph = module_graph(&context.source_facts);
    let mut rows = Vec::new();
    for module in graph.modules.values() {
        let counts = dependency_counts(&graph, &module.key);
        let risk = ((counts.outbound.saturating_sub(5) * 3) as f64
            + (counts.inbound.saturating_sub(12) as f64 * 0.75))
            .min(100.0);
        rows.push(json!({
            "module_key": module.key,
            "path": module.path,
            "layer": classify_module(&module.key),
            "outbound_dependencies": counts.outbound,
            "inbound_dependencies": counts.inbound,
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

fn leverage(_config: &LensConfig, context: &RunContext) -> Result<Value> {
    let graph = module_graph(&context.source_facts);
    let mut rows = Vec::new();
    for module in graph.modules.values() {
        let counts = dependency_counts(&graph, &module.key);
        let score =
            (68.0 + counts.inbound as f64 * 2.5 - counts.outbound as f64 * 3.0).clamp(0.0, 100.0);
        rows.push(json!({
            "module_key": module.key,
            "path": module.path,
            "layer": classify_module(&module.key),
            "reach": counts.inbound,
            "outbound_dependencies": counts.outbound,
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

fn architecture_map(config: &LensConfig, context: &RunContext) -> Result<Value> {
    let graph = module_graph(&context.source_facts);
    let artifacts = MapArtifacts::load(config);
    let git_history = git_history_facts(config, &graph);
    let cycle_modules = cycle_modules(&graph.dependencies);
    let layer_violations = layer_violations(&graph);
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut unknown_metric_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut unknown_module_count = 0usize;
    for module in graph.modules.values() {
        let counts = dependency_counts(&graph, &module.key);
        let sloc = module.source_nonblank_line_count;
        let correctness = artifacts
            .correctness
            .as_ref()
            .map(|correctness| correctness.for_module(&module.key));
        let scores = architecture_risk_scores(ArchitectureRiskInputs {
            sloc,
            public_api_count: module.public_api_count,
            outbound_dependencies: counts.outbound,
            inbound_dependencies: counts.inbound,
            complexity_score: artifacts
                .hotspots
                .as_ref()
                .and_then(|hotspots| hotspots.get(&module.key).copied()),
            change: git_history.for_module(&module.key, correctness.as_ref()),
            correctness,
            locality_risk: artifacts
                .locality
                .as_ref()
                .map(|locality| locality.get(&module.key).copied().unwrap_or_default()),
            leverage_pressure: artifacts
                .leverage
                .as_ref()
                .map(|leverage| leverage.get(&module.key).copied().unwrap_or_default()),
            performance: artifacts.slowspots.as_ref().map(|slowspots| {
                slowspots
                    .by_module
                    .get(&module.key)
                    .cloned()
                    .unwrap_or_default()
            }),
            layer_violations: *layer_violations.get(&module.key).unwrap_or(&0),
            in_cycle: cycle_modules.contains(&module.key),
        });
        if !scores.unknown_metrics.is_empty() {
            unknown_module_count += 1;
            for metric in &scores.unknown_metrics {
                *unknown_metric_counts.entry(metric.clone()).or_default() += 1;
            }
        }
        nodes.push(json!({
            "data": {
                "id": module.key,
                "label": module.key,
                "path": normalize_slashes(&module.path),
                "layer": classify_module(&module.key),
                "layer_color": layer_color(&classify_module(&module.key)),
                "sloc": sloc,
                "public_api_count": module.public_api_count,
                "outbound_dependencies": counts.outbound,
                "inbound_dependencies": counts.inbound,
                "layer_violation_count": *layer_violations.get(&module.key).unwrap_or(&0),
                "cycle_member": cycle_modules.contains(&module.key),
                "maintainability_risk": option_json(scores.maintainability_risk),
                "change_risk": option_json(scores.change_risk),
                "performance_risk": option_json(scores.performance_risk),
                "correctness_risk": option_json(scores.correctness_risk),
                "quality_risk": option_json(scores.quality_risk),
                "architectural_risk": scores.architectural_risk,
                "total_score": option_json(scores.total_score),
                "unknown_metrics": scores.unknown_metrics,
                "raw_facts": {
                    "complexity_score": artifacts.hotspots.as_ref().and_then(|hotspots| hotspots.get(&module.key).copied()).map(Value::from).unwrap_or(Value::Null),
                    "correctness": artifacts.correctness.as_ref().map(|correctness| correctness.for_module(&module.key).to_json()).unwrap_or(Value::Null),
                    "locality": artifacts.locality.as_ref().and_then(|locality| locality.get(&module.key).copied()).map(|locality_risk| json!({"locality_risk": locality_risk})).unwrap_or(Value::Null),
                    "leverage": artifacts.leverage.as_ref().and_then(|leverage| leverage.get(&module.key).copied()).map(|pressure_score| json!({"pressure_score": pressure_score})).unwrap_or(Value::Null),
                    "git_history": git_history.raw_for_module(&module.key),
                    "performance": artifacts.slowspots.as_ref().and_then(|slowspots| slowspots.raw.get(&module.key).cloned()).unwrap_or(Value::Null),
                },
            }
        }));
        if let Some(targets) = graph.dependencies.get(&module.key) {
            for target in targets {
                edges.push(json!({"data": {
                    "source": module.key,
                    "target": target,
                    "layer_violation": is_layer_violation(&module.key, target),
                }}));
            }
        }
    }
    let artifact_status = artifacts.status_json(&git_history);
    let summary = json!({
        "module_count": graph.modules.len(),
        "edge_count": edges.len(),
        "unknown_module_count": unknown_module_count,
        "unknown_metrics": unknown_metric_counts,
        "artifact_status": artifact_status,
    });
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
            "summary": summary,
        },
        "graph": {
            "nodes": nodes,
            "edges": edges,
        },
        "modules": graph.modules.keys().collect::<Vec<_>>(),
        "measurement_confidence": map_measurement_confidence(&config.source_roots, &graph.facts, &artifacts, &git_history),
    }))
}
