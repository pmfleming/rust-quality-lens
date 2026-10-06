use anyhow::Result;
use serde_json::{Value, json};

use crate::config::LensConfig;
use crate::facts::RunContext;
use crate::util::{project_relative_path, round2};

mod maintenance;
mod structural;

pub(super) fn produce(config: &LensConfig, context: &RunContext) -> Result<Value> {
    let graph = context.module_graph();
    let changes = maintenance::churn(config, graph);
    let total_changes = changes
        .as_ref()
        .ok()
        .map(|changes| changes.values().sum::<u64>());
    let records = context
        .source_facts
        .iter()
        .filter(|fact| fact.source_metrics_available)
        .map(|fact| {
            let path = project_relative_path(&config.project_root, &fact.path);
            let changed_lines = changes.as_ref().ok().map(|changes| changes.get(&path).copied().unwrap_or_default());
            let change_share = changed_lines.zip(total_changes.filter(|total| *total > 0))
                .map(|(lines, total)| lines as f64 / total as f64);
            let (outbound, inbound) = graph.dependency_counts(&fact.module_id);
            let dimensions = [
                fact.source.function_count > 0,
                !fact.items.types.is_empty(),
                !fact.items.impls.is_empty(),
                fact.graph.public_api_count > 0,
            ]
            .into_iter()
            .filter(|present| *present)
            .count();
            json!({
                "module_key": fact.module_key,
                "module_id": fact.module_id,
                "path": path,
                "dimension_basis": "syntactic declaration categories, not inferred responsibilities",
                "calibration": "uncalibrated-advisory",
                "declaration_category_count": dimensions,
                "responsibility_dimension_count": dimensions,
                "responsibility_focus": round2((1.0 - dimensions.saturating_sub(1) as f64 / 3.0).max(0.0) * 100.0),
                "outbound_dependencies": outbound,
                "inbound_dependencies": inbound,
                "changed_lines": changed_lines,
                "change_share_percent": change_share.map(|share| round2(share * 100.0)),
                "change_entropy_contribution": change_share.map(|share| round2(entropy_contribution(share))),
            })
        })
        .collect::<Vec<_>>();
    let git_available = changes.is_ok();
    let maintenance = maintenance::produce(config, graph);
    let structural = structural::produce(config, context);
    let mut confidence: crate::contracts::MeasurementConfidence =
        serde_json::from_value(cohesion_confidence(context, git_available)?)?;
    if maintenance["measurement_confidence"]["complete"] != true {
        confidence.mark_partial(
            "maintenance history is incomplete; see maintenance.measurement_confidence",
        );
    }
    if structural["measurement_confidence"]["complete"] != true {
        confidence.mark_partial("structural cohesion has unsupported source relationships; see structural.measurement_confidence");
    }
    let entropy = changes
        .as_ref()
        .ok()
        .zip(total_changes.filter(|total| *total > 0))
        .map(|(changes, total)| {
            round2(
                changes
                    .values()
                    .map(|lines| entropy_contribution(*lines as f64 / total as f64))
                    .sum(),
            )
        });
    Ok(json!({
        "entropy_model_version": 2,
        "entropy_scope": "unique current measured files; all Git history; renames not followed",
        "history_error": changes.as_ref().err().map(ToString::to_string),
        "summary": {
            "module_count": records.len(),
            "total_changed_lines": total_changes,
            "change_entropy": entropy,
        },
        "records": records,
        "maintenance": maintenance,
        "structural": structural,
        "measurement_confidence": confidence,
    }))
}

fn cohesion_confidence(context: &RunContext, git_available: bool) -> Result<Value> {
    let mut source = context.source_confidence();
    source["confidence_scope"] = json!("module_cohesion_and_change_entropy");
    source["observed_inputs"]["module_count"] = json!(context.source_facts.len());
    source["observed_inputs"]["git_history"] = json!(git_available);
    let mut confidence: crate::contracts::MeasurementConfidence = serde_json::from_value(source)?;
    if !git_available {
        confidence.mark_partial("git history unavailable or malformed");
    }
    context
        .identity_resolution
        .apply_confidence(&mut confidence);
    Ok(serde_json::to_value(confidence)?)
}

fn entropy_contribution(share: f64) -> f64 {
    if share <= 0.0 {
        0.0
    } else {
        -share * share.log2()
    }
}

#[cfg(test)]
mod tests {
    use super::entropy_contribution;

    #[test]
    fn entropy_contribution_is_zero_for_absent_change() {
        assert_eq!(entropy_contribution(0.0), 0.0);
        assert_eq!(entropy_contribution(0.5), 0.5);
    }
}
