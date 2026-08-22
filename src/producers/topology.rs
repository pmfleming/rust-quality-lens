use crate::facts::{ModuleGraph, ModuleInfo, RunContext};
use crate::measurement::{MODEL_ID, MODEL_VERSION};
use crate::util::round2;
use anyhow::Result;
use serde_json::{Value, json};

pub(super) fn locality(context: &RunContext) -> Result<Value> {
    let graph = context.module_graph();
    Ok(Value::Array(
        graph
            .modules
            .values()
            .map(|module| locality_record(&graph, module))
            .collect(),
    ))
}

fn locality_record(graph: &ModuleGraph, module: &ModuleInfo) -> Value {
    let (outbound, inbound) = graph.dependency_counts(&module.key);
    let outbound_free = if module.is_entrypoint { 8 } else { 5 };
    let outbound_excess = outbound.saturating_sub(outbound_free);
    let inbound_excess = inbound.saturating_sub(12);
    let risk = ((outbound_excess * 3) as f64 + inbound_excess as f64 * 0.75).min(100.0);
    let signals = if risk > 0.0 {
        vec!["dependency spread"]
    } else if module.is_entrypoint {
        vec!["entrypoint orchestration"]
    } else {
        vec!["stable"]
    };
    module_record(
        graph,
        module,
        json!({
            "outbound_dependencies": outbound,
            "inbound_dependencies": inbound,
            "locality_risk": round2(risk),
            "locality_score": round2(100.0 - risk),
            "signals": signals,
            "score_components": [
                {"signal": "outbound_over_allowance", "raw": outbound_excess, "contribution": (outbound_excess * 3) as f64},
                {"signal": "inbound_over_12", "raw": inbound_excess, "contribution": round2(inbound_excess as f64 * 0.75)},
            ],
            "risk_calibration": "locality",
        }),
    )
}

pub(super) fn leverage(context: &RunContext) -> Result<Value> {
    let graph = context.module_graph();
    Ok(Value::Array(
        graph
            .modules
            .values()
            .map(|module| leverage_record(&graph, module))
            .collect(),
    ))
}

fn leverage_record(graph: &ModuleGraph, module: &ModuleInfo) -> Value {
    let (outbound, inbound) = graph.dependency_counts(&module.key);
    let outbound_weight = if module.is_entrypoint { 1.5 } else { 3.0 };
    let outbound_pressure = outbound as f64 * outbound_weight;
    let score = (68.0 + inbound as f64 * 2.5 - outbound_pressure).clamp(0.0, 100.0);
    module_record(
        graph,
        module,
        json!({
            "reach": inbound,
            "outbound_dependencies": outbound,
            "leverage_score": round2(score),
            "pressure_score": round2(100.0 - score),
            "signals": if score >= 68.0 { vec!["high leverage"] } else { vec!["pressure"] },
            "score_components": [
                {"signal": "base", "raw": 68.0, "contribution": 68.0},
                {"signal": "inbound_reach", "raw": inbound, "contribution": round2(inbound as f64 * 2.5)},
                {"signal": "outbound_pressure", "raw": outbound, "contribution": round2(-outbound_pressure)},
            ],
            "risk_calibration": "leverage",
        }),
    )
}

fn module_record(graph: &ModuleGraph, module: &ModuleInfo, metrics: Value) -> Value {
    let mut record = json!({
        "module_key": module.module_key,
        "module_id": module.id,
        "package_name": module.package_name,
        "target_name": module.target_name,
        "identity_backend": module.identity_backend,
        "path": module.path,
        "target_kind": module.target_kind,
        "entrypoint_kind": module.entrypoint_kind,
        "is_entrypoint": module.is_entrypoint,
        "layer": module.layer(),
        "dependency_identity_backends": dependency_identity_backends(graph, &module.key),
        "risk_model_id": MODEL_ID,
        "risk_model_version": MODEL_VERSION,
    });
    if let (Some(record), Some(metrics)) = (record.as_object_mut(), metrics.as_object()) {
        record.extend(metrics.clone());
    }
    record
}

fn dependency_identity_backends(graph: &ModuleGraph, source: &str) -> Vec<String> {
    graph
        .dependency_provenance
        .iter()
        .filter(|((edge_source, _), _)| edge_source == source)
        .flat_map(|(_, backends)| backends.iter().cloned())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}
