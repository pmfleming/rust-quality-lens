use serde_json::{Value, json};

use crate::facts::FileFacts;
use crate::util::iter_rust_files;

pub(crate) fn source_scan_confidence_with_errors(
    paths: &[String],
    read_errors: Vec<String>,
) -> Value {
    let files = iter_rust_files(paths);
    let missing = missing_source_files(files.is_empty());
    let complete = missing.is_empty() && read_errors.is_empty();
    json!({
        "complete": complete,
        "partial": !complete,
        "confidence_scope": "source_scan",
        "required_inputs": ["rust_source_files"],
        "observed_inputs": {
            "rust_source_files": files.len(),
        },
        "missing_input": missing,
        "stale_input": [],
        "unsupported_pattern": read_errors,
    })
}

pub(crate) fn source_confidence(paths: &[String], facts: &[FileFacts]) -> Value {
    let files = iter_rust_files(paths);
    let mut missing = missing_source_files(files.is_empty());
    if !files.is_empty() && facts.is_empty() {
        missing.push("Rust syntax fact extraction returned no files");
    }
    let mut unsupported = unsupported_patterns(facts);
    let identity_fallback_files = facts
        .iter()
        .filter(|fact| {
            !matches!(
                fact.identity_backend.as_str(),
                "cargo_metadata" | "cargo_manifest"
            )
        })
        .count();
    if identity_fallback_files > 0 {
        unsupported.push(format!(
            "{identity_fallback_files} files used path-based identity fallback"
        ));
    }
    let complete = missing.is_empty() && unsupported.is_empty();
    let cargo_metadata_identity_files = facts
        .iter()
        .filter(|fact| fact.identity_backend == "cargo_metadata")
        .count();
    let syntax_fact_files = facts
        .iter()
        .filter(|fact| fact.parse_status == "ok")
        .count();
    let partial_source_metric_files = facts
        .iter()
        .filter(|fact| fact.parse_status != "ok" && fact.source_metrics_available)
        .count();
    let unreadable_source_files = facts
        .iter()
        .filter(|fact| !fact.source_metrics_available)
        .count();
    let tree_sitter_fallback_files = facts
        .iter()
        .filter(|fact| fact.syntax_backend == "tree-sitter-rust")
        .count();
    let tree_sitter_error_nodes = facts
        .iter()
        .map(|fact| fact.syntax_error_count)
        .sum::<usize>();
    json!({
        "complete": complete,
        "partial": !complete,
        "confidence_scope": "syntax_facts",
        "required_inputs": ["rust_source_files", "rust_syntax_facts"],
        "observed_inputs": {
            "rust_source_files": files.len(),
            "rust_syntax_fact_files": syntax_fact_files,
            "partial_source_metric_files": partial_source_metric_files,
            "unreadable_source_files": unreadable_source_files,
            "tree_sitter_fallback_files": tree_sitter_fallback_files,
            "tree_sitter_error_nodes": tree_sitter_error_nodes,
            "cargo_metadata_identity_files": cargo_metadata_identity_files,
            "cargo_manifest_identity_files": facts.iter().filter(|fact| fact.identity_backend == "cargo_manifest").count(),
            "identity_fallback_files": identity_fallback_files,
        },
        "missing_input": missing,
        "stale_input": [],
        "unsupported_pattern": unsupported,
    })
}

fn missing_source_files(empty: bool) -> Vec<&'static str> {
    if empty {
        vec!["no Rust source files matched the configured paths"]
    } else {
        Vec::new()
    }
}

fn unsupported_patterns(facts: &[FileFacts]) -> Vec<String> {
    facts
        .iter()
        .filter(|fact| fact.parse_status != "ok")
        .map(|fact| format!("{}: {}", fact.path, fact.parse_status))
        .chain(
            facts
                .iter()
                .flat_map(|fact| fact.graph.unsupported_patterns.clone()),
        )
        .collect()
}
