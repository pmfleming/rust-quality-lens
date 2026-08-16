use anyhow::Result;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::process::Command;

use crate::config::LensConfig;
use crate::facts::{RunContext, module_graph};
use crate::measurement::project_relative_path;
use crate::util::round2;

pub(super) fn produce(config: &LensConfig, context: &RunContext) -> Result<Value> {
    let graph = module_graph(&context.source_facts);
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
    Ok(json!({
        "summary": {
            "module_count": records.len(),
            "total_changed_lines": total_changes,
            "change_entropy": round2(records.iter().filter_map(|record| record["change_entropy_contribution"].as_f64()).sum()),
        },
        "records": records,
        "measurement_confidence": {
            "complete": git_available,
            "partial": !git_available,
            "confidence_scope": "module_cohesion_and_change_entropy",
            "required_inputs": ["rust_syntax_facts", "git_history"],
            "observed_inputs": {"module_count": context.source_facts.len(), "git_history": git_available},
            "missing_input": if git_available { Vec::<String>::new() } else { vec!["git history unavailable or empty".to_string()] },
            "stale_input": [],
            "unsupported_pattern": [],
        }
    }))
}

fn git_changes(config: &LensConfig) -> BTreeMap<String, u64> {
    let Ok(output) = Command::new("git")
        .args(["log", "--numstat", "--format="])
        .current_dir(&config.project_root)
        .output()
    else {
        return BTreeMap::new();
    };
    if !output.status.success() {
        return BTreeMap::new();
    }
    String::from_utf8_lossy(&output.stdout)
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
