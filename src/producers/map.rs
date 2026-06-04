use anyhow::Result;
use serde_json::{Value, json};
use std::collections::BTreeMap;

use crate::artifacts::{
    MapArtifacts, cycle_modules, git_history_facts, is_layer_violation, layer_violations,
    map_measurement_confidence,
};
use crate::config::LensConfig;
use crate::facts::{ModuleGraph, ModuleInfo, RunContext, module_graph};
use crate::measurement::{
    MODEL_ID, MODEL_VERSION, RULESET_ID, RULESET_VERSION, classify_module, layer_color,
    option_json, risk_model_classification, risk_model_tool_scores, risk_model_weights,
};
use crate::risk_model::{ArchitectureRiskInputs, architecture_risk_scores};
use crate::util::normalize_slashes;

use super::topology::dependency_counts;

pub(super) fn produce(config: &LensConfig, context: &RunContext) -> Result<Value> {
    let graph = module_graph(&context.source_facts);
    let artifacts = MapArtifacts::load(config);
    let git_history = git_history_facts(config, &graph);
    let cycle_modules = cycle_modules(&graph.dependencies);
    let layer_violations = layer_violations(&graph);
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut unknown_metric_counts = BTreeMap::new();
    let mut unknown_module_count = 0usize;

    for module in graph.modules.values() {
        let node = MapNodeInputs {
            graph: &graph,
            module,
            artifacts: &artifacts,
            git_history: &git_history,
            cycle_modules: &cycle_modules,
            layer_violations: &layer_violations,
        }
        .build(&mut unknown_metric_counts, &mut unknown_module_count);
        nodes.push(node);
        edges.extend(module_edges(&graph, module));
    }

    let summary = json!({
        "module_count": graph.modules.len(),
        "edge_count": edges.len(),
        "unknown_module_count": unknown_module_count,
        "unknown_metrics": unknown_metric_counts,
        "artifact_status": artifacts.status_json(&git_history),
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

struct MapNodeInputs<'a> {
    graph: &'a ModuleGraph,
    module: &'a ModuleInfo,
    artifacts: &'a MapArtifacts,
    git_history: &'a crate::artifacts::GitHistoryIndex,
    cycle_modules: &'a std::collections::BTreeSet<String>,
    layer_violations: &'a BTreeMap<String, usize>,
}

impl MapNodeInputs<'_> {
    fn build(
        &self,
        unknown_metric_counts: &mut BTreeMap<String, usize>,
        unknown_module_count: &mut usize,
    ) -> Value {
        let counts = dependency_counts(self.graph, &self.module.key);
        let correctness = self
            .artifacts
            .correctness
            .as_ref()
            .map(|correctness| correctness.for_module(&self.module.key));
        let scores = architecture_risk_scores(ArchitectureRiskInputs {
            is_entrypoint: self.module.is_entrypoint,
            sloc: self.module.source_nonblank_line_count,
            public_api_count: self.module.public_api_count,
            outbound_dependencies: counts.outbound,
            inbound_dependencies: counts.inbound,
            complexity_score: self
                .artifacts
                .hotspots
                .as_ref()
                .and_then(|hotspots| hotspots.get(&self.module.key).copied()),
            change: self
                .git_history
                .for_module(&self.module.key, correctness.as_ref()),
            correctness,
            locality_risk: self
                .artifacts
                .locality
                .as_ref()
                .map(|locality| locality.get(&self.module.key).copied().unwrap_or_default()),
            leverage_pressure: self
                .artifacts
                .leverage
                .as_ref()
                .map(|leverage| leverage.get(&self.module.key).copied().unwrap_or_default()),
            layer_violations: *self.layer_violations.get(&self.module.key).unwrap_or(&0),
            in_cycle: self.cycle_modules.contains(&self.module.key),
        });
        record_unknown_metrics(
            &scores.unknown_metrics,
            unknown_metric_counts,
            unknown_module_count,
        );
        let layer = module_layer(self.module);
        json!({
            "data": {
                "id": self.module.key,
                "label": self.module.key,
                "path": normalize_slashes(&self.module.path),
                "target_kind": self.module.target_kind,
                "entrypoint_kind": self.module.entrypoint_kind,
                "is_entrypoint": self.module.is_entrypoint,
                "layer": layer,
                "layer_color": layer_color(layer),
                "sloc": self.module.source_nonblank_line_count,
                "public_api_count": self.module.public_api_count,
                "outbound_dependencies": counts.outbound,
                "inbound_dependencies": counts.inbound,
                "layer_violation_count": *self.layer_violations.get(&self.module.key).unwrap_or(&0),
                "cycle_member": self.cycle_modules.contains(&self.module.key),
                "maintainability_risk": option_json(scores.maintainability_risk),
                "change_risk": option_json(scores.change_risk),
                "correctness_risk": option_json(scores.correctness_risk),
                "quality_risk": option_json(scores.quality_risk),
                "architectural_risk": scores.architectural_risk,
                "total_score": option_json(scores.total_score),
                "unknown_metrics": scores.unknown_metrics,
                "raw_facts": self.raw_facts(),
            }
        })
    }

    fn raw_facts(&self) -> Value {
        json!({
            "complexity_score": self.artifacts.hotspots.as_ref().and_then(|hotspots| hotspots.get(&self.module.key).copied()).map(Value::from).unwrap_or(Value::Null),
            "correctness": self.artifacts.correctness.as_ref().map(|correctness| correctness.for_module(&self.module.key).to_json()).unwrap_or(Value::Null),
            "locality": self.artifacts.locality.as_ref().and_then(|locality| locality.get(&self.module.key).copied()).map(|locality_risk| json!({"locality_risk": locality_risk})).unwrap_or(Value::Null),
            "leverage": self.artifacts.leverage.as_ref().and_then(|leverage| leverage.get(&self.module.key).copied()).map(|pressure_score| json!({"pressure_score": pressure_score})).unwrap_or(Value::Null),
            "git_history": self.git_history.raw_for_module(&self.module.key),
            "target": {
                "target_kind": self.module.target_kind,
                "entrypoint_kind": self.module.entrypoint_kind,
                "is_entrypoint": self.module.is_entrypoint,
            },
        })
    }
}

fn module_layer(module: &ModuleInfo) -> &'static str {
    if module.is_entrypoint {
        "Entrypoint"
    } else {
        classify_module(&module.key)
    }
}

fn record_unknown_metrics(
    unknown_metrics: &[String],
    unknown_metric_counts: &mut BTreeMap<String, usize>,
    unknown_module_count: &mut usize,
) {
    if unknown_metrics.is_empty() {
        return;
    }
    *unknown_module_count += 1;
    for metric in unknown_metrics {
        *unknown_metric_counts.entry(metric.clone()).or_default() += 1;
    }
}

fn module_edges(graph: &ModuleGraph, module: &ModuleInfo) -> Vec<Value> {
    let dependency_edges = graph
        .dependencies
        .get(&module.key)
        .into_iter()
        .flatten()
        .map(|target| {
            json!({"data": {
                "source": module.key,
                "target": target,
                "kind": "dependency",
                "layer_violation": is_layer_violation(&module.key, target),
            }})
        });
    let containment_edges = graph
        .containment_dependencies
        .get(&module.key)
        .into_iter()
        .flatten()
        .map(|target| {
            json!({"data": {
                "source": module.key,
                "target": target,
                "kind": "containment",
                "layer_violation": false,
            }})
        });
    dependency_edges.chain(containment_edges).collect()
}
