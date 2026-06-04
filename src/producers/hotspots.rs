use anyhow::Result;
use serde_json::{Value, json};

use crate::artifacts::json_f64;
use crate::config::LensConfig;
use crate::facts::{FileFacts, RunContext};
use crate::measurement::{MODEL_ID, MODEL_VERSION, source_confidence};
use crate::util::{normalize_slashes, round2};

pub(super) fn produce(config: &LensConfig, context: &RunContext) -> Result<Value> {
    let facts = &context.source_facts;
    let confidence = source_confidence(&config.source_roots, facts);
    let mut rows = facts
        .iter()
        .filter(|fact| fact.parse_status == "ok")
        .map(|fact| hotspot_row(fact, &confidence))
        .collect::<Vec<_>>();

    rows.sort_by(|a, b| json_f64(b, "score").total_cmp(&json_f64(a, "score")));
    Ok(Value::Array(rows))
}

fn hotspot_row(fact: &FileFacts, confidence: &Value) -> Value {
    let sloc = fact.source.source_nonblank_line_count as f64;
    let cognitive = fact.source.cognitive_complexity as f64;
    let cyclomatic = fact.source.cyclomatic_complexity as f64;
    let mi = (100.0 - sloc * 0.15 - cognitive * 1.7).max(0.0);
    let quality_score =
        round2((cognitive * 3.7 + cyclomatic * 2.0 + (65.0 - mi).max(0.0) * 1.2) * 1.12);
    json!({
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
    })
}
