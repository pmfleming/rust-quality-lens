use anyhow::Result;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;

use crate::config::LensConfig;
use crate::facts::{FileFacts, RunContext, TestFact, TestStatus, resolve_dependency, run_tests};
use crate::measurement::{
    RULESET_ID, RULESET_VERSION, classify_path, module_for_path, project_relative_path,
    source_confidence, test_kind_for_path, title_from_name,
};
use crate::util::project_input_fingerprint;

pub(super) fn produce(config: &LensConfig, context: &RunContext, run: bool) -> Result<Value> {
    let paths = &context.correctness_paths;
    let facts = &context.correctness_facts;
    let mut confidence = source_confidence(paths, facts);
    let statuses = if run {
        run_tests(config)?
    } else {
        HashMap::new()
    };
    let source_modules = context
        .source_facts
        .iter()
        .filter(|fact| fact.parse_status == "ok")
        .map(|fact| fact.module_key.clone())
        .collect::<BTreeSet<_>>();
    let statuses_ref = &statuses;
    let coverage = coverage_evidence(config);
    if coverage.stale {
        mark_stale_coverage(&mut confidence);
    }
    let coverage_evidence = coverage.records;
    let covered_modules = coverage_evidence
        .iter()
        .filter_map(|row| row["module_key"].as_str())
        .collect::<BTreeSet<_>>();
    let covered_module_ids = coverage_evidence
        .iter()
        .filter_map(|row| row["module_id"].as_str())
        .collect::<BTreeSet<_>>();
    let mut tests = facts
        .iter()
        .filter(|fact| fact.parse_status == "ok")
        .flat_map(|fact| {
            let tested_modules = tested_modules(config, fact, &source_modules);
            let tested_module_ids = context
                .source_facts
                .iter()
                .filter(|source| {
                    source.package_name == fact.package_name
                        && tested_modules.contains(&source.module_key)
                })
                .map(|source| source.module_id.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            fact.items.tests.iter().map(move |test| {
                test_row(
                    config,
                    fact,
                    test,
                    &tested_modules,
                    &tested_module_ids,
                    statuses_ref,
                )
            })
        })
        .collect::<Vec<_>>();
    for test in &mut tests {
        let observed = test["tested_modules"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|module| covered_modules.contains(module))
            .map(str::to_string)
            .collect::<Vec<_>>();
        if let Some(object) = test.as_object_mut() {
            object.insert("coverage_observed_modules".to_string(), json!(observed));
            let observed_ids = object
                .get("tested_module_ids")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .filter(|module| covered_module_ids.contains(module))
                .map(str::to_string)
                .collect::<Vec<_>>();
            object.insert(
                "coverage_observed_module_ids".to_string(),
                json!(observed_ids),
            );
        }
    }
    let layers = layer_rows(&tests);
    let mut summary = correctness_summary(&tests, &layers, &statuses);
    if let Some(object) = summary.as_object_mut() {
        object.insert(
            "coverage_module_count".to_string(),
            Value::from(coverage_evidence.len()),
        );
    }
    Ok(json!({
        "version": 1,
        "generated_from": "rqlens",
        "layer_ruleset": {"id": RULESET_ID, "version": RULESET_VERSION},
        "summary": summary,
        "measurement_confidence": confidence,
        "layers": layers,
        "tests": tests,
        "module_coverage_evidence": coverage_evidence,
        "attribution_sources": ["syntax_dependencies", "aggregate_line_coverage"],
    }))
}

#[derive(Default)]
struct CoverageEvidence {
    records: Vec<Value>,
    stale: bool,
}

fn coverage_evidence(config: &LensConfig) -> CoverageEvidence {
    let path = config.output_dir.join("coverage.json");
    let Ok(text) = fs::read_to_string(path) else {
        return CoverageEvidence::default();
    };
    let Ok(document) = serde_json::from_str::<Value>(&text) else {
        return CoverageEvidence::default();
    };
    let current = project_input_fingerprint(&config.project_root, &config.source_roots);
    coverage_from_document(&document, current["digest"].as_str())
}

fn coverage_from_document(document: &Value, current_digest: Option<&str>) -> CoverageEvidence {
    if document["measurement_confidence"]["complete"] != true {
        return CoverageEvidence::default();
    }
    if document["input_fingerprint"]["digest"].as_str() != current_digest {
        return CoverageEvidence {
            records: Vec::new(),
            stale: true,
        };
    }
    let data = document.get("data").unwrap_or(document);
    let records = data["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|file| file["lines"]["covered"].as_u64().unwrap_or(0) > 0)
        .map(|file| {
            json!({
                "module_key": file["module_key"],
                "module_id": file["module_id"],
                "path": file["path"],
                "line_coverage_percent": file["lines"]["percent"],
                "source": "cargo_llvm_cov_aggregate",
            })
        })
        .collect();
    CoverageEvidence {
        records,
        stale: false,
    }
}

fn mark_stale_coverage(confidence: &mut Value) {
    let Some(object) = confidence.as_object_mut() else {
        return;
    };
    object.insert("complete".to_string(), Value::Bool(false));
    object.insert("partial".to_string(), Value::Bool(true));
    if let Some(stale) = object
        .entry("stale_input")
        .or_insert_with(|| json!([]))
        .as_array_mut()
    {
        stale.push(Value::String(
            "coverage.json input fingerprint is stale".to_string(),
        ));
    }
}

fn test_row(
    config: &LensConfig,
    fact: &FileFacts,
    test: &TestFact,
    tested_modules: &[String],
    tested_module_ids: &[String],
    statuses: &HashMap<String, TestStatus>,
) -> Value {
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
        "module": tested_modules.first().cloned().unwrap_or_else(|| if test.module_key.is_empty() { module_for_path(&test.path) } else { test.module_key.clone() }),
        "tested_modules": tested_modules,
        "tested_module_ids": tested_module_ids,
        "defining_package": fact.package_name,
        "defining_target": fact.target_name,
        "identity_backend": fact.identity_backend,
        "description": title_from_name(&test.name),
        "kind": test_kind_for_path(&path),
        "last_status": last_status,
        "last_duration": last_duration,
        "command": format!(
            "cargo test --package {} {} -- --exact",
            fact.package_name, test.qualified_name
        ),
    })
}

fn tested_modules(
    config: &LensConfig,
    fact: &FileFacts,
    source_modules: &BTreeSet<String>,
) -> Vec<String> {
    let module_keys = source_modules.iter().cloned().collect::<Vec<_>>();
    let mut targets = BTreeSet::new();

    if let Some(module) = nearest_source_module(&fact.module_key, source_modules) {
        targets.insert(module);
    }

    let mut crate_names = crate_names(config);
    crate_names.insert(fact.package_name.clone());
    crate_names.insert(fact.package_name.replace('-', "_"));
    for raw in &fact.graph.dependencies {
        let normalized = crate_names
            .iter()
            .find_map(|name| raw.strip_prefix(&format!("{name}::")))
            .unwrap_or(raw);
        if let Some(module) = resolve_dependency(normalized, &fact.module_key, &module_keys) {
            targets.insert(module);
        }
    }
    targets.into_iter().collect()
}

fn crate_names(config: &LensConfig) -> BTreeSet<String> {
    let mut names = BTreeSet::from([
        config.project_name.clone(),
        config.project_name.replace('-', "_"),
    ]);
    if let Ok(text) = std::fs::read_to_string(config.project_root.join("Cargo.toml"))
        && let Ok(manifest) = toml::from_str::<toml::Value>(&text)
        && let Some(name) = manifest["package"]["name"].as_str()
    {
        names.insert(name.to_string());
        names.insert(name.replace('-', "_"));
    }
    names
}

fn nearest_source_module(module: &str, source_modules: &BTreeSet<String>) -> Option<String> {
    let mut candidate = module.to_string();
    loop {
        if source_modules.contains(&candidate) {
            return Some(candidate);
        }
        let (parent, _) = candidate.rsplit_once("::")?;
        candidate = parent.to_string();
    }
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
    let run_status = statuses.get("__run__").map(|status| status.status.as_str());
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
        "run_failed": run_status.is_some_and(|status| status != "passed"),
        "compile_failed": run_status == Some("compile_failed"),
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
mod tests {
    use super::{coverage_from_document, status_for_test};
    use crate::facts::TestStatus;
    use serde_json::json;
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

        let found = status_for_test(&statuses, &["tests::case".to_string()], "tests::case");

        assert_eq!(found.map(|status| status.status.as_str()), Some("passed"));
    }

    #[test]
    fn status_lookup_uses_unambiguous_qualified_suffix() {
        let mut statuses = HashMap::new();
        statuses.insert("module::tests::case".to_string(), status("passed"));

        let found = status_for_test(
            &statuses,
            &["other.rs::tests::case".to_string()],
            "tests::case",
        );

        assert_eq!(found.map(|status| status.status.as_str()), Some("passed"));
    }

    #[test]
    fn status_lookup_ignores_ambiguous_suffix_matches() {
        let mut statuses = HashMap::new();
        statuses.insert("one::tests::case".to_string(), status("passed"));
        statuses.insert("two::tests::case".to_string(), status("failed"));

        assert!(status_for_test(&statuses, &[], "tests::case").is_none());
    }

    #[test]
    fn stale_coverage_is_not_attributed_to_current_sources() {
        let document = json!({
            "input_fingerprint": {"digest": "old"},
            "measurement_confidence": {"complete": true},
            "data": {"files": [{
                "module_key": "domain",
                "module_id": "app::lib::domain",
                "path": "src/domain.rs",
                "lines": {"covered": 4, "percent": 100.0}
            }]}
        });

        let stale = coverage_from_document(&document, Some("current"));
        assert!(stale.stale);
        assert!(stale.records.is_empty());

        let fresh = coverage_from_document(&document, Some("old"));
        assert!(!fresh.stale);
        assert_eq!(fresh.records.len(), 1);
    }
}
