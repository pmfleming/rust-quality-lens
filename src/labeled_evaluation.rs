use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Deserialize)]
pub(crate) struct Observation {
    pub(crate) id: String,
    pub(crate) rule: String,
    pub(crate) partition: String,
    pub(crate) expected: bool,
    pub(crate) observed: Option<bool>,
}

#[derive(Serialize)]
pub(crate) struct Evaluation {
    partition: String,
    rule: String,
    labels: usize,
    true_positives: usize,
    false_positives: usize,
    false_negatives: usize,
    true_negatives: usize,
    abstained: usize,
    precision: Option<f64>,
    recall: Option<f64>,
    evaluation_coverage: Option<f64>,
    abstention_rate: Option<f64>,
}

pub(crate) fn evaluate(cases: &[Observation]) -> anyhow::Result<Vec<Evaluation>> {
    let mut ids = BTreeSet::new();
    let mut groups = BTreeMap::<(&str, &str), Vec<&Observation>>::new();
    for case in cases {
        anyhow::ensure!(
            !case.id.is_empty() && ids.insert(&case.id),
            "duplicate or empty calibration ID"
        );
        anyhow::ensure!(
            matches!(case.partition.as_str(), "development" | "holdout"),
            "invalid partition"
        );
        groups
            .entry((&case.partition, &case.rule))
            .or_default()
            .push(case);
    }
    let ratio = |part: usize, total: usize| (total > 0).then(|| part as f64 / total as f64);
    Ok(groups
        .into_iter()
        .map(|((partition, rule), group)| {
            let tp = group
                .iter()
                .filter(|case| case.expected && case.observed == Some(true))
                .count();
            let fp = group
                .iter()
                .filter(|case| !case.expected && case.observed == Some(true))
                .count();
            let fn_count = group
                .iter()
                .filter(|case| case.expected && case.observed != Some(true))
                .count();
            let tn = group
                .iter()
                .filter(|case| !case.expected && case.observed == Some(false))
                .count();
            let abstained = group.iter().filter(|case| case.observed.is_none()).count();
            Evaluation {
                partition: partition.into(),
                rule: rule.into(),
                labels: group.len(),
                true_positives: tp,
                false_positives: fp,
                false_negatives: fn_count,
                true_negatives: tn,
                abstained,
                precision: ratio(tp, tp + fp),
                recall: ratio(tp, tp + fn_count),
                evaluation_coverage: ratio(group.len() - abstained, group.len()),
                abstention_rate: ratio(abstained, group.len()),
            }
        })
        .collect())
}

#[cfg(test)]
#[path = "labeled_evaluation_tests.rs"]
mod tests;
