use serde_json::Value;
use std::collections::BTreeMap;

use crate::artifacts::{first_f64, module_from_record};
use crate::risk_model::CorrectnessFacts;

pub(super) fn hotspot_index(value: &Value) -> BTreeMap<String, f64> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let module = module_from_record(row)?;
            let score = first_f64(row, &["score", "quality_score", "complexity_score"])?;
            Some((module, score))
        })
        .collect()
}

pub(super) fn metric_index(value: &Value, score_keys: &[&str]) -> BTreeMap<String, f64> {
    value
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

#[derive(Clone, Default)]
pub(crate) struct CorrectnessIndex {
    by_module: BTreeMap<String, CorrectnessFacts>,
}

impl CorrectnessIndex {
    pub(super) fn from_review(value: &Value) -> Self {
        let mut by_module = BTreeMap::<String, CorrectnessFacts>::new();
        for test in value["tests"].as_array().into_iter().flatten() {
            let Some(module) = test["module"]
                .as_str()
                .map(str::to_string)
                .or_else(|| module_from_record(test))
            else {
                continue;
            };
            update_correctness(by_module.entry(module).or_default(), test);
        }
        Self { by_module }
    }

    pub(crate) fn for_module(&self, module: &str) -> CorrectnessFacts {
        self.by_module.get(module).cloned().unwrap_or_default()
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
