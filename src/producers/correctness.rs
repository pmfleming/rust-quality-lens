use anyhow::Result;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};

use crate::config::LensConfig;
use crate::facts::{RunContext, TestFact, TestStatus, run_tests};
use crate::measurement::{
    RULESET_ID, RULESET_VERSION, classify_path, module_for_path, project_relative_path,
    source_confidence, test_kind_for_path, title_from_name,
};

pub(super) fn produce(config: &LensConfig, context: &RunContext, run: bool) -> Result<Value> {
    let paths = &context.correctness_paths;
    let facts = &context.correctness_facts;
    let confidence = source_confidence(paths, facts);
    let statuses = if run {
        run_tests(config)?
    } else {
        HashMap::new()
    };
    let tests = facts
        .iter()
        .filter(|fact| fact.parse_status == "ok")
        .flat_map(|fact| &fact.items.tests)
        .map(|test| test_row(config, test, &statuses))
        .collect::<Vec<_>>();
    let layers = layer_rows(&tests);
    let summary = correctness_summary(&tests, &layers, &statuses);
    Ok(json!({
        "version": 1,
        "generated_from": "rqlens",
        "layer_ruleset": {"id": RULESET_ID, "version": RULESET_VERSION},
        "summary": summary,
        "measurement_confidence": confidence,
        "layers": layers,
        "tests": tests,
    }))
}

fn test_row(config: &LensConfig, test: &TestFact, statuses: &HashMap<String, TestStatus>) -> Value {
    let path = project_relative_path(&config.project_root, &test.path);
    let name = test.name.clone();
    let qualified_name = test.qualified_name.clone();
    let id = format!("{path}::{qualified_name}");
    let status_keys = status_keys(&id, &path, test, &qualified_name, &name);
    let (last_status, last_duration) = status_for_test(statuses, &status_keys, &qualified_name)
        .map(|status| {
            (
                status.status.clone(),
                status.duration.map(Value::from).unwrap_or(Value::Null),
            )
        })
        .unwrap_or_else(|| ("unknown".to_string(), Value::Null));

    json!({
        "id": id,
        "name": name,
        "qualified_name": qualified_name,
        "path": path,
        "line": test.line,
        "layer": classify_path(&path),
        "module": if test.module_key.is_empty() { module_for_path(&test.path) } else { test.module_key.clone() },
        "description": title_from_name(&test.name),
        "kind": test_kind_for_path(&path),
        "last_status": last_status,
        "last_duration": last_duration,
        "command": format!("cargo test {}", test.name),
    })
}

fn status_keys(
    id: &str,
    path: &str,
    test: &TestFact,
    qualified_name: &str,
    name: &str,
) -> Vec<String> {
    vec![
        id.to_string(),
        format!("{path}::{qualified_name}"),
        format!("{}::{}", test.module_key, test.name),
        qualified_name.to_string(),
        name.to_string(),
    ]
}

fn layer_rows(tests: &[Value]) -> Vec<Value> {
    let mut by_layer: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
    for test in tests {
        let layer = test["layer"].as_str().unwrap_or("Unclassified").to_string();
        let status = test["last_status"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        let entry = by_layer.entry(layer).or_default();
        *entry.entry("total".into()).or_default() += 1;
        *entry.entry(status).or_default() += 1;
    }
    by_layer
        .into_iter()
        .map(|(layer, counts)| {
            let total = *counts.get("total").unwrap_or(&0);
            let failed = *counts.get("failed").unwrap_or(&0);
            json!({
                "name": layer,
                "total": total,
                "passed": *counts.get("passed").unwrap_or(&0),
                "failed": failed,
                "skipped": *counts.get("skipped").unwrap_or(&0),
                "unknown": *counts.get("unknown").unwrap_or(&0),
                "failed_ratio": if total == 0 { 0.0 } else { failed as f64 / total as f64 },
            })
        })
        .collect()
}

fn correctness_summary(
    tests: &[Value],
    layers: &[Value],
    statuses: &HashMap<String, TestStatus>,
) -> Value {
    json!({
        "test_count": tests.len(),
        "integration_count": tests.iter().filter(|t| t["kind"] == "integration").count(),
        "inline_count": tests.iter().filter(|t| t["kind"] == "inline").count(),
        "benchmark_count": tests.iter().filter(|t| t["kind"] == "benchmark").count(),
        "example_count": tests.iter().filter(|t| t["kind"] == "example").count(),
        "binary_count": tests.iter().filter(|t| t["kind"] == "binary").count(),
        "layers": layers.len(),
        "failed": tests.iter().filter(|t| t["last_status"] == "failed").count(),
        "unknown": tests.iter().filter(|t| t["last_status"] == "unknown").count(),
        "last_run": statuses.get("__run__").map(|s| json!({"status": s.status, "duration": s.duration, "stdout_tail": s.stdout_tail, "stderr_tail": s.stderr_tail})),
    })
}

fn status_for_test<'a>(
    statuses: &'a HashMap<String, TestStatus>,
    exact_keys: &[String],
    qualified_name: &str,
) -> Option<&'a TestStatus> {
    for key in exact_keys {
        if let Some(status) = statuses.get(key) {
            return Some(status);
        }
    }

    let suffix = format!("::{qualified_name}");
    let mut matches = statuses
        .iter()
        .filter(|(key, _)| key.ends_with(&suffix))
        .map(|(_, status)| status);
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::status_for_test;
    use crate::facts::TestStatus;
    use std::collections::HashMap;

    fn status(status: &str) -> TestStatus {
        TestStatus {
            status: status.to_string(),
            duration: None,
            stdout_tail: None,
            stderr_tail: None,
        }
    }

    #[test]
    fn status_lookup_uses_exact_key_first() {
        let mut statuses = HashMap::new();
        statuses.insert("tests::case".to_string(), status("passed"));
        statuses.insert("module::tests::case".to_string(), status("failed"));

        let found =
            status_for_test(&statuses, &["tests::case".to_string()], "tests::case").unwrap();

        assert_eq!(found.status, "passed");
    }

    #[test]
    fn status_lookup_uses_unambiguous_qualified_suffix() {
        let mut statuses = HashMap::new();
        statuses.insert("module::tests::case".to_string(), status("passed"));

        let found = status_for_test(
            &statuses,
            &["other.rs::tests::case".to_string()],
            "tests::case",
        )
        .unwrap();

        assert_eq!(found.status, "passed");
    }

    #[test]
    fn status_lookup_ignores_ambiguous_suffix_matches() {
        let mut statuses = HashMap::new();
        statuses.insert("one::tests::case".to_string(), status("passed"));
        statuses.insert("two::tests::case".to_string(), status("failed"));

        assert!(status_for_test(&statuses, &[], "tests::case").is_none());
    }
}
