use anyhow::Result;
use serde_json::{Value, json};
use std::collections::HashSet;

use crate::facts::{ModuleGraph, ModuleInfo, RunContext, module_graph};
use crate::measurement::{MODEL_ID, MODEL_VERSION, classify_module};
use crate::util::round2;

pub(super) struct DependencyCounts {
    pub(super) outbound: usize,
    pub(super) inbound: usize,
}

pub(super) fn dependency_counts(graph: &ModuleGraph, module_key: &str) -> DependencyCounts {
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

pub(super) fn locality(context: &RunContext) -> Result<Value> {
    let graph = module_graph(&context.source_facts);
    Ok(Value::Array(
        graph
            .modules
            .values()
            .map(|module| {
                let counts = dependency_counts(&graph, &module.key);
                let outbound_free = if module.is_entrypoint { 8 } else { 5 };
                let risk = ((counts.outbound.saturating_sub(outbound_free) * 3) as f64
                    + (counts.inbound.saturating_sub(12) as f64 * 0.75))
                    .min(100.0);
                let signals = if risk > 0.0 {
                    vec!["dependency spread"]
                } else if module.is_entrypoint {
                    vec!["entrypoint orchestration"]
                } else {
                    vec!["stable"]
                };
                json!({
                    "module_key": module.key,
                    "path": module.path,
                    "target_kind": module.target_kind,
                    "entrypoint_kind": module.entrypoint_kind,
                    "is_entrypoint": module.is_entrypoint,
                    "layer": module_layer(module),
                    "outbound_dependencies": counts.outbound,
                    "inbound_dependencies": counts.inbound,
                    "locality_risk": round2(risk),
                    "locality_score": round2(100.0 - risk),
                    "signals": signals,
                    "risk_model_id": MODEL_ID,
                    "risk_model_version": MODEL_VERSION,
                    "risk_calibration": "locality",
                })
            })
            .collect(),
    ))
}

pub(super) fn leverage(context: &RunContext) -> Result<Value> {
    let graph = module_graph(&context.source_facts);
    Ok(Value::Array(
        graph
            .modules
            .values()
            .map(|module| {
                let counts = dependency_counts(&graph, &module.key);
                let outbound_weight = if module.is_entrypoint { 1.5 } else { 3.0 };
                let score = (68.0 + counts.inbound as f64 * 2.5
                    - counts.outbound as f64 * outbound_weight)
                    .clamp(0.0, 100.0);
                json!({
                    "module_key": module.key,
                    "path": module.path,
                    "target_kind": module.target_kind,
                    "entrypoint_kind": module.entrypoint_kind,
                    "is_entrypoint": module.is_entrypoint,
                    "layer": module_layer(module),
                    "reach": counts.inbound,
                    "outbound_dependencies": counts.outbound,
                    "leverage_score": round2(score),
                    "pressure_score": round2(100.0 - score),
                    "signals": if score >= 68.0 { vec!["high leverage"] } else { vec!["pressure"] },
                    "risk_model_id": MODEL_ID,
                    "risk_model_version": MODEL_VERSION,
                    "risk_calibration": "leverage",
                })
            })
            .collect(),
    ))
}

fn module_layer(module: &ModuleInfo) -> &'static str {
    if module.is_entrypoint {
        "Entrypoint"
    } else {
        classify_module(&module.key)
    }
}
