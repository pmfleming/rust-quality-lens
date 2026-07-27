use anyhow::Result;
use serde_json::{Value, json};
use std::collections::BTreeMap;

use crate::artifacts::MapEvidence;
use crate::config::LensConfig;
use crate::facts::{ModuleGraph, ModuleInfo, RunContext, module_graph};
use crate::measurement::{
    MODEL_ID, MODEL_VERSION, RULESET_ID, RULESET_VERSION, layer_color, option_json,
    risk_model_classification, risk_model_tool_scores, risk_model_weights,
};
use crate::risk_model::{ArchitectureRiskInputs, architecture_risk_scores};
use crate::util::normalize_slashes;

pub(super) fn produce(config: &LensConfig, context: &RunContext) -> Result<Value> {
    let graph = module_graph(&context.source_facts);
    let evidence = MapEvidence::load(config, &graph);
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut unknown_metric_counts = BTreeMap::new();
    let mut unknown_module_count = 0usize;

    for module in graph.modules.values() {
        let node = MapNodeInputs {
            graph: &graph,
            module,
            evidence: &evidence,
        }
        .build(&mut unknown_metric_counts, &mut unknown_module_count);
        nodes.push(node);
        edges.extend(module_edges(&graph, module, &evidence));
    }

    let summary = json!({
        "module_count": graph.modules.len(),
        "edge_count": edges.len(),
        "unknown_module_count": unknown_module_count,
        "unknown_metrics": unknown_metric_counts,
        "artifact_status": evidence.status_json(),
    });
    let mut measurement_confidence =
        evidence.measurement_confidence(&config.source_roots, &graph.facts);
    if let Some(object) = measurement_confidence.as_object_mut() {
        object.insert(
            "semantic_identity".to_string(),
            context.identity_resolution.to_json(),
        );
    }
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
            "identity_resolution": context.identity_resolution.to_json(),
        },
        "graph": {
            "nodes": nodes,
            "edges": edges,
        },
        "modules": graph.modules.values().map(|module| json!({"module_id": module.id, "module_key": module.module_key})).collect::<Vec<_>>(),
        "measurement_confidence": measurement_confidence,
    }))
}

struct MapNodeInputs<'a> {
    graph: &'a ModuleGraph,
    module: &'a ModuleInfo,
    evidence: &'a MapEvidence,
}

impl MapNodeInputs<'_> {
    fn build(
        &self,
        unknown_metric_counts: &mut BTreeMap<String, usize>,
        unknown_module_count: &mut usize,
    ) -> Value {
        let (outbound, inbound) = self.graph.dependency_counts(&self.module.key);
        let mut correctness = self
            .evidence
            .artifacts
            .correctness
            .as_ref()
            .map(|correctness| {
                correctness.for_module_identity(&self.module.id, &self.module.module_key)
            });
        if let Some(coverage) = self
            .evidence
            .artifacts
            .coverage
            .as_ref()
            .and_then(|coverage| module_metric(coverage, self.module))
        {
            let facts = correctness.get_or_insert_with(Default::default);
            facts.line_coverage_percent = Some(coverage);
        }
        let scores = architecture_risk_scores(ArchitectureRiskInputs {
            is_entrypoint: self.module.is_entrypoint,
            sloc: self.module.source_nonblank_line_count,
            public_api_count: self.module.public_api_count,
            outbound_dependencies: outbound,
            inbound_dependencies: inbound,
            complexity_score: self
                .evidence
                .artifacts
                .hotspots
                .as_ref()
                .and_then(|hotspots| module_metric(hotspots, self.module)),
            change: self
                .evidence
                .change_for(&self.module.key, correctness.as_ref()),
            correctness,
            locality_risk: self
                .evidence
                .artifacts
                .locality
                .as_ref()
                .map(|locality| module_metric(locality, self.module).unwrap_or_default()),
            leverage_pressure: self
                .evidence
                .artifacts
                .leverage
                .as_ref()
                .map(|leverage| module_metric(leverage, self.module).unwrap_or_default()),
            layer_violations: self.evidence.violation_count(&self.module.key),
            in_cycle: self.evidence.in_cycle(&self.module.key),
        });
        record_unknown_metrics(
            &scores.unknown_metrics,
            unknown_metric_counts,
            unknown_module_count,
        );
        let layer = self.module.layer();
        json!({
            "data": {
                "id": self.module.key,
                "module_id": self.module.id,
                "label": self.module.module_key,
                "module_key": self.module.module_key,
                "package_name": self.module.package_name,
                "target_name": self.module.target_name,
                "identity_backend": self.module.identity_backend,
                "path": normalize_slashes(&self.module.path),
                "target_kind": self.module.target_kind,
                "entrypoint_kind": self.module.entrypoint_kind,
                "is_entrypoint": self.module.is_entrypoint,
                "layer": layer,
                "layer_color": layer_color(layer),
                "sloc": self.module.source_nonblank_line_count,
                "public_api_count": self.module.public_api_count,
                "outbound_dependencies": outbound,
                "inbound_dependencies": inbound,
                "layer_violation_count": self.evidence.violation_count(&self.module.key),
                "cycle_member": self.evidence.in_cycle(&self.module.key),
                "maintainability_risk": option_json(scores.maintainability_risk),
                "change_risk": option_json(scores.change_risk),
                "correctness_risk": option_json(scores.correctness_risk),
                "quality_risk": option_json(scores.quality_risk),
                "architectural_risk": scores.architectural_risk,
                "total_score": option_json(scores.total_score),
                "unknown_metrics": scores.unknown_metrics,
                "score_components": scores.score_components,
                "raw_facts": self.raw_facts(),
            }
        })
    }

    fn raw_facts(&self) -> Value {
        json!({
            "complexity_score": self.evidence.artifacts.hotspots.as_ref().and_then(|hotspots| module_metric(hotspots, self.module)).map(Value::from).unwrap_or(Value::Null),
            "correctness": self.evidence.artifacts.correctness.as_ref().map(|correctness| {
                let mut facts = correctness.for_module_identity(&self.module.id, &self.module.module_key);
                facts.line_coverage_percent = self.evidence.artifacts.coverage.as_ref().and_then(|coverage| module_metric(coverage, self.module));
                facts.to_json()
            }).unwrap_or(Value::Null),
            "coverage": self.evidence.artifacts.coverage.as_ref().and_then(|coverage| module_metric(coverage, self.module)).map(|line_percent| json!({"line_percent": line_percent})).unwrap_or(Value::Null),
            "locality": self.evidence.artifacts.locality.as_ref().and_then(|locality| module_metric(locality, self.module)).map(|locality_risk| json!({"locality_risk": locality_risk})).unwrap_or(Value::Null),
            "leverage": self.evidence.artifacts.leverage.as_ref().and_then(|leverage| module_metric(leverage, self.module)).map(|pressure_score| json!({"pressure_score": pressure_score})).unwrap_or(Value::Null),
            "git_history": self.evidence.history_for(&self.module.key),
                "target": {
                "module_id": self.module.id,
                "package_name": self.module.package_name,
                "target_name": self.module.target_name,
                "identity_backend": self.module.identity_backend,
                "target_kind": self.module.target_kind,
                "entrypoint_kind": self.module.entrypoint_kind,
                "is_entrypoint": self.module.is_entrypoint,
            },
        })
    }
}

fn module_metric(metrics: &BTreeMap<String, f64>, module: &ModuleInfo) -> Option<f64> {
    metrics
        .get(&module.id)
        .or_else(|| metrics.get(&module.module_key))
        .copied()
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

fn module_edges(graph: &ModuleGraph, module: &ModuleInfo, evidence: &MapEvidence) -> Vec<Value> {
    let dependency_edges = graph
        .dependencies
        .get(&module.key)
        .into_iter()
        .flatten()
        .map(|target| {
            let edge = (module.key.clone(), target.clone());
            json!({"data": {
                "source": module.key,
                "target": target,
                "kind": "dependency",
                "layer_violation": evidence.is_layer_violation(&module.key, target),
                "identity_backends": graph.dependency_provenance.get(&edge).cloned().unwrap_or_default(),
                "symbol_identities": graph.dependency_symbols.get(&edge).cloned().unwrap_or_default(),
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
