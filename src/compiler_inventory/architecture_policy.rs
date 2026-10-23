use super::{architecture_graph::Graph, capture, validate_report};
use crate::{
    architecture::dependency_violation,
    config::{ArchitectureConfig, PolicyRuleLevel},
};
use anyhow::{Result, ensure};
use serde_json::{Value, json};

impl super::Inventory {
    pub(crate) fn architecture_policy(
        &self,
        rules: &ArchitectureConfig,
        fingerprint_complete: bool,
    ) -> Value {
        let graphs = self.architecture_graphs();
        let confidence = self.architecture_confidence(&graphs, fingerprint_complete);
        let mut violations = Vec::new();
        for graph in &graphs {
            for rule in rules
                .rules
                .iter()
                .filter(|rule| rule.level != PolicyRuleLevel::Off)
            {
                for edge in &graph.edges {
                    let source = &graph.modules[&edge.source_id];
                    let target = &graph.modules[&edge.target_id];
                    if let Some(violation) = dependency_violation(
                        rule,
                        &source.id,
                        &target.id,
                        &source.selector,
                        &target.selector,
                    ) {
                        let location = source
                            .origin
                            .as_ref()
                            .and_then(|origin| origin.location.as_ref());
                        violations.push(json!({
                            "rule_id": violation.rule_id, "category": "static-finding", "kind": "architecture-boundary", "severity": violation.level.as_str(),
                            "compiler_context_id": graph.compiler_context_id,
                            "source_module_id": source.id, "target_module_id": target.id,
                            "source_selector": source.selector, "target_selector": target.selector,
                            "module_key": source.module_path, "package_name": source.package_name,
                            "path": location.map(|location| &location.path), "line": location.map(|location| location.line),
                            "reference_count": edge.observations.len(),
                            "reference_origins": edge.observations.iter().map(|r| &r.origin).collect::<Vec<_>>(),
                            "message": violation.message, "source": "configured_architecture_rule",
                        }));
                    }
                }
            }
        }
        violations.sort_by_key(Value::to_string);
        json!({
            "backend": "rustc-hir", "architecture_model_version": 1,
            "summary": {"configured_rule_count": rules.rules.len(), "violation_count": violations.len(),
                "unresolved_reference_count": graphs.iter().map(|g| g.unresolved_references.len()).sum::<usize>(),
                "compilation_context_count": graphs.len()},
            "violations": violations, "measurement_confidence": confidence,
            "rules": rules, "compiler_inventory": self.report(fingerprint_complete),
        })
    }

    fn architecture_confidence(&self, graphs: &[Graph], fingerprint_complete: bool) -> Value {
        let mut missing = self.reasons.clone();
        if !fingerprint_complete {
            missing.push("quality input fingerprint is incomplete".into());
        }
        if let Err(error) = capture::validate_coverage(&self.profiles, &self.expected_contexts) {
            missing.push(format!("architecture compilation coverage: {error}"));
        }
        for graph in graphs {
            missing.extend(
                graph
                    .missing_evidence
                    .iter()
                    .map(|reason| format!("{}: {reason}", graph.compiler_context_id)),
            );
        }
        missing.sort();
        missing.dedup();
        let complete = missing.is_empty() && !graphs.is_empty();
        json!({"complete": complete, "partial": !complete, "confidence_scope": "compiled_static_module_dependencies",
            "inventory_scope": "compiled_selected_contexts_after_cfg_and_expansion",
            "required_inputs": ["successful_compilation", "complete_selected_target_contexts", "all_reference_categories", "compiler_resolved_module_owners", "input_fingerprint"],
            "observed_inputs": {"compilation_contexts": graphs.len(), "module_edges_across_contexts": graphs.iter().map(|g| g.edges.len()).sum::<usize>()},
            "missing_input": missing, "unsupported_pattern": [], "stale_input": [],
            "count_scope": "Static HIR references per selected compiled context, not runtime reachability, all feature combinations or source/body rule completeness."})
    }
}

pub(crate) fn validate_architecture_policy(
    value: &Value,
    fingerprint_complete: bool,
) -> Result<()> {
    ensure!(
        value["backend"] == "rustc-hir" && value["architecture_model_version"] == 1,
        "unknown compiler architecture backend/model"
    );
    let wire = &value["compiler_inventory"];
    validate_report(wire)?;
    let inventory = super::Inventory {
        profiles: serde_json::from_value(wire["profiles"].clone())?,
        expected_contexts: serde_json::from_value(wire["expected_contexts"].clone())?,
        reasons: serde_json::from_value(wire["capture_failures"].clone())?,
        requested_scope: wire["requested_scope"].clone(),
    };
    let rules: ArchitectureConfig = serde_json::from_value(value["rules"].clone())?;
    let expected = inventory.architecture_policy(&rules, fingerprint_complete);
    for key in ["summary", "measurement_confidence", "compiler_inventory"] {
        ensure!(
            value[key] == expected[key],
            "compiler architecture {key} contradicts captured evidence"
        );
    }
    // Finding normalization adds source-anchor IDs, but must not alter evaluation.
    let mut violations = value["violations"].clone();
    if let Some(rows) = violations.as_array_mut() {
        for row in rows {
            if let Some(row) = row.as_object_mut() {
                row.remove("id");
                row.remove("identity_version");
            }
        }
    }
    ensure!(
        violations == expected["violations"],
        "compiler architecture violations differ from graph/rules"
    );
    Ok(())
}
