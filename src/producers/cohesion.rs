use anyhow::Result;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::process::Command;

use crate::config::LensConfig;
use crate::facts::RunContext;
use crate::util::{project_relative_path, round2};

mod maintenance;

pub(super) fn produce(config: &LensConfig, context: &RunContext) -> Result<Value> {
    let graph = context.module_graph();
    let changes = git_changes(config);
    let total_changes = changes.values().sum::<u64>();
    let records = context
        .source_facts
        .iter()
        .filter(|fact| fact.source_metrics_available)
        .map(|fact| {
            let path = project_relative_path(&config.project_root, &fact.path);
            let changed_lines = changes.get(&path).copied().unwrap_or_default();
            let change_share = if total_changes == 0 {
                None
            } else {
                Some(changed_lines as f64 / total_changes as f64)
            };
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
                "responsibility_dimension_count": dimensions,
                "responsibility_focus": round2((1.0 - dimensions.saturating_sub(1) as f64 / 3.0).max(0.0) * 100.0),
                "outbound_dependencies": outbound,
                "inbound_dependencies": inbound,
                "changed_lines": changed_lines,
                "change_share_percent": change_share.map(|share| round2(share * 100.0)),
                "change_entropy_contribution": change_share.map(entropy_contribution),
            })
        })
        .collect::<Vec<_>>();
    let git_available = !changes.is_empty();
    let maintenance = maintenance::produce(config, graph);
    let mut confidence: crate::contracts::MeasurementConfidence =
        serde_json::from_value(cohesion_confidence(context, git_available)?)?;
    if maintenance["measurement_confidence"]["complete"] != true {
        confidence.mark_partial(
            "maintenance history is incomplete; see maintenance.measurement_confidence",
        );
    }
    Ok(json!({
        "summary": {
            "module_count": records.len(),
            "total_changed_lines": total_changes,
            "change_entropy": round2(records.iter().filter_map(|record| record["change_entropy_contribution"].as_f64()).sum()),
        },
        "records": records,
        "maintenance": maintenance,
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
        confidence.mark_partial("git history unavailable or empty");
    }
    context
        .identity_resolution
        .apply_confidence(&mut confidence);
    Ok(serde_json::to_value(confidence)?)
}

fn git_changes(config: &LensConfig) -> BTreeMap<String, u64> {
    let Ok(output) = crate::command_runner::checked_output(
        Command::new("git")
            .args(["log", "--numstat", "--format="])
            .current_dir(&config.project_root),
        std::time::Duration::from_secs(config.verification.timeout_seconds),
    ) else {
        return BTreeMap::new();
    };
    output
        .stdout
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(3, '\t');
            let added = fields.next()?.parse::<u64>().ok()?;
            let deleted = fields.next()?.parse::<u64>().ok()?;
            let path = fields.next()?.replace('\\', "/");
            Some((path, added + deleted))
        })
        .fold(BTreeMap::new(), |mut changes, (path, count)| {
            *changes.entry(path).or_default() += count;
            changes
        })
}

fn entropy_contribution(share: f64) -> f64 {
    if share <= 0.0 {
        0.0
    } else {
        round2(-share * share.log2())
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
