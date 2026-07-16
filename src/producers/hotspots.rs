use anyhow::Result;
use serde_json::{Value, json};

use crate::artifacts::json_f64;
use crate::config::LensConfig;
use crate::facts::{FileFacts, FunctionFact, RunContext};
use crate::measurement::{MODEL_ID, MODEL_VERSION, source_confidence};
use crate::util::{normalize_slashes, round2};

pub(super) fn produce(config: &LensConfig, context: &RunContext) -> Result<Value> {
    let facts = &context.source_facts;
    let confidence = source_confidence(&config.source_roots, facts);
    let mut rows = facts
        .iter()
        .filter(|fact| fact.parse_status == "ok")
        .flat_map(|fact| hotspot_rows(fact, &confidence))
        .collect::<Vec<_>>();

    rows.sort_by(|a, b| json_f64(b, "score").total_cmp(&json_f64(a, "score")));
    Ok(Value::Array(rows))
}

fn hotspot_rows(fact: &FileFacts, confidence: &Value) -> Vec<Value> {
    let function_scores = fact
        .items
        .functions
        .iter()
        .map(|function| (function, function_score(function)))
        .collect::<Vec<_>>();
    let mut rows = vec![module_hotspot_row(fact, &function_scores, confidence)];
    rows.extend(
        function_scores
            .into_iter()
            .map(|(function, score)| function_hotspot_row(fact, function, score, confidence)),
    );
    rows
}

fn function_score(function: &FunctionFact) -> f64 {
    let branch = function.branch_pressure as f64 * 3.7;
    let paths = function.path_pressure.saturating_sub(1) as f64 * 2.0;
    let nesting = function.max_nesting_depth.saturating_sub(2) as f64 * 3.0;
    let size = (function.source_line_count.saturating_sub(20) as f64 * 0.25).min(20.0);
    round2((branch + paths + nesting + size) * 1.12)
}

fn function_hotspot_row(
    fact: &FileFacts,
    function: &FunctionFact,
    score: f64,
    confidence: &Value,
) -> Value {
    let branch_contribution = round2(function.branch_pressure as f64 * 3.7 * 1.12);
    let path_contribution = round2(function.path_pressure.saturating_sub(1) as f64 * 2.0 * 1.12);
    let nesting_contribution =
        round2(function.max_nesting_depth.saturating_sub(2) as f64 * 3.0 * 1.12);
    let size_contribution =
        round2((function.source_line_count.saturating_sub(20) as f64 * 0.25).min(20.0) * 1.12);
    json!({
        "name": function.qualified_name,
        "qualified_name": function.qualified_name,
        "function_name": function.name,
        "module_key": function.module_key,
        "module_id": fact.module_id,
        "package_name": fact.package_name,
        "target_name": fact.target_name,
        "identity_backend": fact.identity_backend,
        "path": function.path,
        "kind": "function",
        "start_line": function.start_line,
        "end_line": function.end_line,
        "sloc": function.source_line_count,
        "branch_pressure": function.branch_pressure,
        "path_pressure": function.path_pressure,
        "max_nesting_depth": function.max_nesting_depth,
        "score": score,
        "quality_score": score,
        "risk_level": risk_level(score, 19.15, 32.98, 80.08),
        "score_components": [
            {"signal": "branch_pressure", "raw": function.branch_pressure, "contribution": branch_contribution},
            {"signal": "path_pressure", "raw": function.path_pressure, "contribution": path_contribution},
            {"signal": "nesting_depth_over_2", "raw": function.max_nesting_depth.saturating_sub(2), "contribution": nesting_contribution},
            {"signal": "lines_over_20", "raw": function.source_line_count.saturating_sub(20), "contribution": size_contribution},
        ],
        "signals": function_signals(function),
        "risk_model_id": MODEL_ID,
        "risk_model_version": MODEL_VERSION,
        "risk_calibration": "hotspots_function",
        "measurement_confidence": confidence,
    })
}

fn function_signals(function: &FunctionFact) -> Vec<&'static str> {
    let mut signals = Vec::new();
    if function.branch_pressure >= 8 {
        signals.push("branch pressure");
    }
    if function.max_nesting_depth >= 4 {
        signals.push("deep nesting");
    }
    if function.source_line_count >= 60 {
        signals.push("long function");
    }
    if signals.is_empty() {
        signals.push("stable");
    }
    signals
}

fn module_hotspot_row(
    fact: &FileFacts,
    function_scores: &[(&FunctionFact, f64)],
    confidence: &Value,
) -> Value {
    let sloc = fact.source.source_nonblank_line_count as f64;
    let mut scores = function_scores
        .iter()
        .map(|(_, score)| *score)
        .collect::<Vec<_>>();
    scores.sort_by(f64::total_cmp);
    let maximum = scores.last().copied().unwrap_or_default();
    let p95 = percentile(&scores, 0.95);
    let mean = if scores.is_empty() {
        0.0
    } else {
        scores.iter().sum::<f64>() / scores.len() as f64
    };
    let size_pressure = (sloc / 10.0).min(20.0);
    let max_contribution = maximum * 0.55;
    let p95_contribution = p95 * 0.25;
    let mean_contribution = mean * 0.10;
    let size_contribution = size_pressure * 0.50;
    let quality_score =
        round2(max_contribution + p95_contribution + mean_contribution + size_contribution);
    json!({
        "name": normalize_slashes(&fact.path),
        "module_key": fact.module_key,
        "module_id": fact.module_id,
        "package_name": fact.package_name,
        "target_name": fact.target_name,
        "identity_backend": fact.identity_backend,
        "kind": "module",
        "path": normalize_slashes(&fact.path),
        "start_line": 1,
        "end_line": fact.source.source_line_count,
        "sloc": sloc,
        "cloc": fact.source.source_comment_line_count,
        "nom_fn": fact.source.function_count,
        "function_count": function_scores.len(),
        "max_function_score": round2(maximum),
        "p95_function_score": round2(p95),
        "mean_function_score": round2(mean),
        "quality_score": quality_score,
        "size_score": round2(size_pressure),
        "score": quality_score,
        "risk_level": risk_level(quality_score, 67.30, 95.51, 139.94),
        "score_components": [
            {"signal": "max_function_score", "raw": round2(maximum), "contribution": round2(max_contribution)},
            {"signal": "p95_function_score", "raw": round2(p95), "contribution": round2(p95_contribution)},
            {"signal": "mean_function_score", "raw": round2(mean), "contribution": round2(mean_contribution)},
            {"signal": "module_size_pressure", "raw": round2(size_pressure), "contribution": round2(size_contribution)},
        ],
        "signals": if sloc >= 250.0 || maximum >= 60.0 { "watch" } else { "stable" },
        "complexity_density": if sloc > 0.0 { round2(quality_score / sloc) } else { 0.0 },
        "risk_model_id": MODEL_ID,
        "risk_model_version": MODEL_VERSION,
        "risk_calibration": "hotspots_module",
        "measurement_confidence": confidence,
    })
}

fn risk_level(score: f64, watch: f64, high: f64, extreme: f64) -> &'static str {
    if score >= extreme {
        "extreme"
    } else if score >= high {
        "high"
    } else if score >= watch {
        "watch"
    } else {
        "normal"
    }
}

fn percentile(sorted: &[f64], percentile: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let index = ((sorted.len() - 1) as f64 * percentile).ceil() as usize;
    sorted[index.min(sorted.len() - 1)]
}
