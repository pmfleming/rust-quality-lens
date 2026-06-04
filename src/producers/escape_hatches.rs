use anyhow::Result;
use serde_json::{Value, json};
use std::collections::BTreeMap;

use crate::artifacts::json_f64;
use crate::config::LensConfig;
use crate::facts::{FileFacts, RunContext};
use crate::measurement::{
    escape_labels, escape_weights, provenance, source_confidence, tool_score_metadata,
};
use crate::util::round2;

pub(super) fn produce(config: &LensConfig, context: &RunContext) -> Result<Value> {
    let facts = &context.source_facts;
    let confidence = source_confidence(&config.source_roots, facts);
    let weights = escape_weights();
    let labels = escape_labels();
    let metadata = tool_score_metadata("escape_hatches");
    let provenance = provenance();
    let mut rows = facts
        .iter()
        .filter_map(|fact| escape_row(fact, &weights, &labels, &metadata, &provenance, &confidence))
        .collect::<Vec<_>>();

    rows.sort_by(|a, b| {
        json_f64(b, "escape_hatch_score")
            .total_cmp(&json_f64(a, "escape_hatch_score"))
            .then_with(|| b["total_count"].as_u64().cmp(&a["total_count"].as_u64()))
            .then_with(|| a["module_key"].as_str().cmp(&b["module_key"].as_str()))
    });
    Ok(Value::Array(rows))
}

fn escape_row(
    fact: &FileFacts,
    weights: &BTreeMap<&'static str, f64>,
    labels: &BTreeMap<&'static str, &'static str>,
    metadata: &crate::measurement::ToolScoreMetadata,
    provenance: &crate::measurement::Provenance,
    confidence: &Value,
) -> Option<Value> {
    let counts = normalized_escape_counts(fact, weights);
    let total_count: usize = counts.values().sum();
    if total_count == 0 {
        return None;
    }

    let scoring_counts = scoring_escape_counts(&counts);
    let grouped = grouped_counts(&counts);
    let locations = escape_locations(fact, labels);
    let allow_count = grouped.allow_count;
    Some(json!({
        "module_name": fact.module_key,
        "module_key": fact.module_key,
        "path": fact.path,
        "escape_hatch_score": escape_score(&scoring_counts, weights),
        "total_count": total_count,
        "unsafe_count": grouped.unsafe_count,
        "ffi_count": grouped.ffi_count,
        "global_mutability_count": count(&counts, "static_mut"),
        "raw_memory_count": grouped.raw_memory_count,
        "deref_coercion_count": grouped.deref_count,
        "glob_import_count": count(&counts, "glob_import"),
        "container_ref_return_count": count(&counts, "container_ref_return"),
        "layout_linkage_count": grouped.layout_linkage_count,
        "clippy_suppression_count": count(&counts, "clippy_suppression"),
        "lint_suppression_count": count(&counts, "lint_suppression"),
        "allow_attribute_count": allow_count,
        "clippy_allow_count": count(&counts, "clippy_suppression"),
        "counts": counts,
        "scoring_counts": scoring_counts,
        "locations": locations,
        "allow_locations": locations.iter().filter(|item| matches!(item["kind"].as_str(), Some("lint_suppression" | "clippy_suppression"))).collect::<Vec<_>>(),
        "signals": escape_signals(&counts, labels, allow_count),
        "measured_at": provenance.measured_at,
        "command": provenance.command,
        "host": provenance.host,
        "measurement_confidence": confidence,
        "source": "static_rust_escape_hatches",
        "mock": false,
        "risk_model_id": metadata.risk_model_id,
        "risk_model_version": metadata.risk_model_version,
        "risk_calibration": metadata.risk_calibration,
    }))
}

fn normalized_escape_counts(
    fact: &FileFacts,
    weights: &BTreeMap<&'static str, f64>,
) -> BTreeMap<String, usize> {
    weights
        .keys()
        .map(|key| {
            (
                key.to_string(),
                *fact.escapes.escape_counts.get(*key).unwrap_or(&0),
            )
        })
        .collect()
}

fn scoring_escape_counts(counts: &BTreeMap<String, usize>) -> BTreeMap<String, usize> {
    let mut scoring_counts = counts.clone();
    if count(&scoring_counts, "clippy_suppression") > 0 {
        scoring_counts.insert("lint_suppression".to_string(), 0);
    }
    scoring_counts
}

fn escape_score(
    scoring_counts: &BTreeMap<String, usize>,
    weights: &BTreeMap<&'static str, f64>,
) -> f64 {
    round2(
        scoring_counts
            .iter()
            .map(|(key, count)| *count as f64 * weights[key.as_str()])
            .sum(),
    )
}

#[derive(Default)]
struct GroupedCounts {
    unsafe_count: usize,
    ffi_count: usize,
    raw_memory_count: usize,
    deref_count: usize,
    layout_linkage_count: usize,
    allow_count: usize,
}

fn grouped_counts(counts: &BTreeMap<String, usize>) -> GroupedCounts {
    GroupedCounts {
        unsafe_count: count(counts, "unsafe_block")
            + count(counts, "unsafe_fn")
            + count(counts, "unsafe_impl")
            + count(counts, "unsafe_trait"),
        ffi_count: count(counts, "extern_block") + count(counts, "extern_fn"),
        raw_memory_count: count(counts, "union")
            + count(counts, "raw_borrow")
            + count(counts, "asm_macro")
            + count(counts, "transmute")
            + count(counts, "maybe_uninit"),
        deref_count: count(counts, "deref_impl") + count(counts, "deref_mut_impl"),
        layout_linkage_count: count(counts, "repr_escape") + count(counts, "linkage_escape"),
        allow_count: count(counts, "lint_suppression") + count(counts, "clippy_suppression"),
    }
}

fn escape_locations(fact: &FileFacts, labels: &BTreeMap<&'static str, &'static str>) -> Vec<Value> {
    fact.escapes
        .escape_locations
        .iter()
        .filter_map(|loc| {
            labels
                .get(loc.kind.as_str())
                .map(|label| json!({"kind": loc.kind, "label": label, "line": loc.line}))
        })
        .collect()
}

fn escape_signals(
    counts: &BTreeMap<String, usize>,
    labels: &BTreeMap<&'static str, &'static str>,
    allow_count: usize,
) -> Vec<String> {
    let mut signals = counts
        .iter()
        .filter(|(_, count)| **count > 0)
        .map(|(key, count)| {
            format!(
                "{} {}",
                labels.get(key.as_str()).unwrap_or(&key.as_str()),
                count
            )
        })
        .collect::<Vec<_>>();
    if allow_count > 0 {
        signals.push(format!("allow/expect attributes {allow_count}"));
    }
    if signals.is_empty() {
        signals.push("stable".to_string());
    }
    signals
}

fn count(counts: &BTreeMap<String, usize>, key: &str) -> usize {
    counts.get(key).copied().unwrap_or(0)
}
