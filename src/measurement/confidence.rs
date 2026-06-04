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
    let unsupported = unsupported_patterns(facts);
    let complete = missing.is_empty() && unsupported.is_empty();
    json!({
        "complete": complete,
        "partial": !complete,
        "confidence_scope": "syntax_facts",
        "required_inputs": ["rust_source_files", "rust_syntax_facts"],
        "observed_inputs": {
            "rust_source_files": files.len(),
            "rust_syntax_fact_files": facts.len(),
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
