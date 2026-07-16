use serde_json::Value;
use std::collections::BTreeMap;

use crate::artifacts::{artifact_payload, first_f64, module_from_record};
use crate::risk_model::CorrectnessFacts;

pub(super) fn hotspot_index(value: &Value) -> BTreeMap<String, f64> {
    artifact_payload(value)
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| row["kind"].as_str().is_none_or(|kind| kind == "module"))
        .filter_map(|row| {
            let module = module_from_record(row)?;
            let score = first_f64(row, &["score", "quality_score", "complexity_score"])?;
            Some((module, score))
        })
        .collect()
}

pub(super) fn metric_index(value: &Value, score_keys: &[&str]) -> BTreeMap<String, f64> {
    artifact_payload(value)
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let module = module_from_record(row)?;
            let score = first_f64(row, score_keys)?;
            Some((module, score))
        })
        .collect()
}

pub(super) fn coverage_index(value: &Value) -> BTreeMap<String, f64> {
    let value = artifact_payload(value);
    value["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| {
            Some((
                row["module_id"]
                    .as_str()
                    .or_else(|| row["module_key"].as_str())?
                    .to_string(),
                row["lines"]["percent"].as_f64()?,
            ))
        })
        .collect()
}

#[derive(Clone, Default)]
pub(crate) struct CorrectnessIndex {
    by_module: BTreeMap<String, CorrectnessFacts>,
    run_failed: bool,
    compile_failed: bool,
}

impl CorrectnessIndex {
    pub(super) fn from_review(value: &Value) -> Self {
        let value = artifact_payload(value);
        let mut by_module = BTreeMap::<String, CorrectnessFacts>::new();
        for test in value["tests"].as_array().into_iter().flatten() {
            let modules = test["tested_module_ids"]
                .as_array()
                .map(|modules| {
                    modules
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .filter(|modules| !modules.is_empty())
                .or_else(|| {
                    test["tested_modules"]
                        .as_array()
                        .map(|modules| {
                            modules
                                .iter()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect::<Vec<_>>()
                        })
                        .filter(|modules| !modules.is_empty())
                })
                .or_else(|| {
                    test["module"]
                        .as_str()
                        .map(|module| vec![module.to_string()])
                        .or_else(|| module_from_record(test).map(|module| vec![module]))
                })
                .unwrap_or_default();
            for module in modules {
                update_correctness(by_module.entry(module).or_default(), test);
            }
        }
        let run_failed = value["summary"]["run_failed"].as_bool().unwrap_or(false);
        let compile_failed = value["summary"]["compile_failed"]
            .as_bool()
            .unwrap_or(false);
        Self {
            by_module,
            run_failed,
            compile_failed,
        }
    }

    pub(crate) fn for_module(&self, module: &str) -> CorrectnessFacts {
        let mut facts = self.by_module.get(module).cloned().unwrap_or_default();
        facts.run_failed = self.run_failed;
        facts.compile_failed = self.compile_failed;
        facts
    }

    pub(crate) fn for_module_identity(
        &self,
        module_id: &str,
        module_key: &str,
    ) -> CorrectnessFacts {
        let selected = if self.by_module.contains_key(module_id) {
            module_id
        } else {
            module_key
        };
        self.for_module(selected)
    }
}

fn update_correctness(facts: &mut CorrectnessFacts, test: &Value) {
    facts.test_count += 1;
    match test["last_status"].as_str().unwrap_or("unknown") {
        "failed" => facts.failed_count += 1,
        "skipped" | "ignored" => facts.skipped_count += 1,
        "passed" => {}
        _ => facts.unknown_count += 1,
    }
}
