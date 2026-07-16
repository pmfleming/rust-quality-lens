use anyhow::Result;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

use crate::artifacts::json_f64;
use crate::config::LensConfig;
use crate::facts::{FileFacts, ImplFact, RunContext, TypeFact};
use crate::measurement::{provenance, source_confidence, tool_score_metadata, type_risk};
use crate::util::round2;

#[derive(Clone, Default)]
struct ImplSummary {
    method_count: usize,
    block_count: usize,
    files: BTreeSet<String>,
}

pub(super) fn produce(config: &LensConfig, context: &RunContext) -> Result<Value> {
    let facts = &context.source_facts;
    let confidence = source_confidence(&config.source_roots, facts);
    let metadata = tool_score_metadata("type_health");
    let provenance = provenance();
    let impls = impl_index(facts);
    let mut rows = facts
        .iter()
        .filter(|fact| fact.parse_status == "ok")
        .flat_map(|fact| {
            fact.items
                .types
                .iter()
                .map(|ty| type_row(fact, ty, &impls, &metadata, &provenance, &confidence))
        })
        .collect::<Vec<_>>();

    rows.sort_by(|a, b| {
        json_f64(b, "structural_risk")
            .total_cmp(&json_f64(a, "structural_risk"))
            .then_with(|| b["method_count"].as_u64().cmp(&a["method_count"].as_u64()))
            .then_with(|| b["field_count"].as_u64().cmp(&a["field_count"].as_u64()))
            .then_with(|| {
                a["qualified_name"]
                    .as_str()
                    .cmp(&b["qualified_name"].as_str())
            })
    });
    Ok(Value::Array(rows))
}

fn impl_index(facts: &[FileFacts]) -> BTreeMap<String, ImplSummary> {
    let definitions = facts
        .iter()
        .flat_map(|fact| &fact.items.types)
        .map(|ty| ty.qualified_name.clone())
        .collect::<BTreeSet<_>>();
    let mut impls = BTreeMap::<String, ImplSummary>::new();
    for imp in facts.iter().flat_map(|fact| &fact.items.impls) {
        let Some(identity) = impl_identity(imp, &definitions) else {
            continue;
        };
        let entry = impls.entry(identity).or_default();
        entry.method_count += imp.method_count;
        entry.block_count += 1;
        entry.files.insert(imp.path.clone());
    }
    impls
}

fn impl_identity(imp: &ImplFact, definitions: &BTreeSet<String>) -> Option<String> {
    let raw = if imp.qualified_type_name.is_empty() {
        imp.type_name.as_str()
    } else {
        imp.qualified_type_name.as_str()
    };
    let candidate = if let Some(rest) = raw.strip_prefix("crate::") {
        rest.to_string()
    } else if let Some(rest) = raw.strip_prefix("self::") {
        format!("{}::{rest}", imp.module_key)
    } else if raw.starts_with("super::") {
        resolve_super_type(raw, &imp.module_key)
    } else if raw.contains("::") {
        raw.to_string()
    } else {
        format!("{}::{raw}", imp.module_key)
    };
    if definitions.contains(&candidate) {
        return Some(candidate);
    }
    let suffix = format!("::{raw}");
    let mut matches = definitions.iter().filter(|name| name.ends_with(&suffix));
    let first = matches.next()?.clone();
    matches.next().is_none().then_some(first)
}

fn resolve_super_type(raw: &str, module: &str) -> String {
    let mut base = module.split("::").collect::<Vec<_>>();
    let mut remainder = raw;
    while let Some(rest) = remainder.strip_prefix("super::") {
        base.pop();
        remainder = rest;
    }
    base.into_iter()
        .chain(std::iter::once(remainder))
        .collect::<Vec<_>>()
        .join("::")
}

fn type_row(
    fact: &FileFacts,
    ty: &TypeFact,
    impls: &BTreeMap<String, ImplSummary>,
    metadata: &crate::measurement::ToolScoreMetadata,
    provenance: &crate::measurement::Provenance,
    confidence: &Value,
) -> Value {
    let summary = impls.get(&ty.qualified_name).cloned().unwrap_or_default();
    let impl_files_vec = summary.files.into_iter().collect::<Vec<_>>();
    let (risk, signals, score_components) = type_risk(
        ty,
        summary.method_count,
        summary.block_count,
        impl_files_vec.len(),
    );
    json!({
        "type_name": ty.type_name,
        "qualified_name": ty.qualified_name,
        "module_key": ty.module_key,
        "module_id": fact.module_id,
        "package_name": fact.package_name,
        "target_name": fact.target_name,
        "identity_backend": fact.identity_backend,
        "path": ty.path,
        "line": ty.line,
        "kind": ty.kind,
        "shape": ty.shape,
        "field_count": ty.field_count,
        "variant_count": ty.variant_count,
        "variant_field_count": ty.variant_field_count,
        "declaration_span": ty.declaration_span,
        "method_count": summary.method_count,
        "impl_block_count": summary.block_count,
        "impl_file_count": impl_files_vec.len(),
        "impl_files": impl_files_vec,
        "structural_risk": risk,
        "structural_score": round2(100.0 - risk),
        "signals": signals,
        "score_components": score_components,
        "measured_at": provenance.measured_at,
        "command": provenance.command,
        "host": provenance.host,
        "measurement_confidence": confidence,
        "risk_model_id": metadata.risk_model_id,
        "risk_model_version": metadata.risk_model_version,
        "risk_calibration": metadata.risk_calibration,
        "source": "static_type_health",
        "mock": false,
    })
}
