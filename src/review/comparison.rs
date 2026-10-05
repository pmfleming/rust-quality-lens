use crate::config::LensConfig;
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, path::Path};

pub(super) fn snapshot(config: &LensConfig, measurements: &[Value], complete: bool) -> Value {
    let profile = json!({"generator": env!("CARGO_PKG_VERSION"), "identity": "source-anchor-v1", "model": crate::measurement::MODEL_VERSION,
        "architecture": config.architecture, "verification": config.verification, "identity_resolution": config.identity_resolution,
        "source_roots": config.source_roots.iter().map(|root| crate::util::project_relative_path(&config.project_root, root)).collect::<Vec<_>>()});
    let mut ids = BTreeSet::new();
    for value in measurements {
        collect_ids(value, &mut ids);
    }
    json!({"version": 1, "analysis_identity": crate::util::stable_hash(&profile.to_string()), "complete": complete, "finding_ids": ids})
}

fn collect_ids(value: &Value, ids: &mut BTreeSet<String>) {
    match value {
        Value::Array(items) => {
            for value in items {
                collect_ids(value, ids);
            }
        }
        Value::Object(item) => {
            if item.get("rule_id").and_then(Value::as_str).is_some()
                && let Some(id) = item.get("id").and_then(Value::as_str)
            {
                ids.insert(id.to_string());
            }
            for value in item.values() {
                collect_ids(value, ids);
            }
        }
        _ => {}
    }
}

pub(super) fn compare_file(current: &Value, baseline: Option<&Path>) -> Value {
    let Some(path) = baseline else {
        return json!({"status": "not_configured"});
    };
    match fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
    {
        Some(base) => compare(current, &base["finding_snapshot"]),
        None => {
            json!({"status": "incomplete", "reason": "Baseline review is missing or malformed"})
        }
    }
}

fn compare(current: &Value, base: &Value) -> Value {
    if current["version"] != 1
        || base["version"] != 1
        || current["complete"] != true
        || base["complete"] != true
        || !current["analysis_identity"].is_string()
        || current["analysis_identity"] != base["analysis_identity"]
    {
        return json!({"status": "incomparable", "reason": "Snapshot version, profile or completeness differs"});
    }
    let ids = |value: &Value| -> Option<BTreeSet<String>> {
        value["finding_ids"]
            .as_array()?
            .iter()
            .map(|id| id.as_str().filter(|id| !id.is_empty()).map(str::to_string))
            .collect()
    };
    let (Some(current), Some(base)) = (ids(current), ids(base)) else {
        return json!({"status": "incomplete", "reason": "Invalid finding identities"});
    };
    json!({"status": "compared", "introduced": current.difference(&base).collect::<Vec<_>>(),
        "resolved": base.difference(&current).collect::<Vec<_>>(), "inherited": current.intersection(&base).collect::<Vec<_>>()})
}

#[cfg(test)]
mod tests {
    use super::compare;
    use serde_json::json;
    #[test]
    fn snapshots_compare_semantic_occurrences_not_changed_hunks() {
        let snapshot = |ids| json!({"version":1,"complete":true,"analysis_identity":"profile","finding_ids":ids});
        let current = snapshot(vec!["retained", "new-in-unchanged-file"]);
        let baseline = snapshot(vec!["retained", "removed"]);
        let result = compare(&current, &baseline);
        assert_eq!(result["introduced"], json!(["new-in-unchanged-file"]));
        assert_eq!(result["resolved"], json!(["removed"]));
        let mut incompatible = baseline.clone();
        incompatible["complete"] = json!(false);
        assert_eq!(compare(&current, &incompatible)["status"], "incomparable");
        incompatible["complete"] = json!(true);
        incompatible["analysis_identity"] = json!("different");
        assert_eq!(compare(&current, &incompatible)["status"], "incomparable");
    }
}
