use anyhow::Result;
use serde_json::{Value, json};

use crate::config::LensConfig;
use crate::facts::RunContext;
use crate::measurement::source_confidence;

pub(super) fn produce(config: &LensConfig, context: &RunContext) -> Result<Value> {
    let graph = context.module_graph();
    let evaluation = config.architecture.evaluate(&graph);
    let mut confidence = source_confidence(&config.source_roots, &context.source_facts);
    if !config.architecture.rules.is_empty()
        && evaluation.unresolved_references > 0
        && let Some(object) = confidence.as_object_mut()
    {
        object.insert("complete".to_string(), Value::Bool(false));
        object.insert("partial".to_string(), Value::Bool(true));
        if let Some(patterns) = object
            .entry("unsupported_pattern".to_string())
            .or_insert_with(|| json!([]))
            .as_array_mut()
        {
            patterns.push(json!(format!(
                "{} dependency references were unresolved",
                evaluation.unresolved_references
            )));
        }
    }
    let violations = evaluation
        .violations
        .into_iter()
        .map(|violation| {
            let source = graph.modules.get(&violation.source_module_id);
            json!({
                "rule_id": violation.rule_id,
                "category": "static-finding",
                "kind": "architecture-boundary",
                "severity": violation.level.as_str(),
                "source_module_id": violation.source_module_id,
                "target_module_id": violation.target_module_id,
                "module_key": source.map(|module| module.module_key.as_str()),
                "package_name": source.map(|module| module.package_name.as_str()),
                "path": source.map(|module| module.path.to_string_lossy().to_string()),
                "message": violation.message,
                "source": "configured_architecture_rule",
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "summary": {
            "configured_rule_count": config.architecture.rules.len(),
            "violation_count": violations.len(),
            "unresolved_reference_count": evaluation.unresolved_references,
        },
        "violations": violations,
        "measurement_confidence": confidence,
    }))
}
