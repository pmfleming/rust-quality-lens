use anyhow::Result;
use serde::Serialize;
use serde_json::Value;

use crate::config::LensConfig;
use crate::facts::{FileFacts, RunContext, TypeFact, TypeImplIndex};
use crate::measurement::{provenance, tool_score_metadata, type_risk};
use crate::util::round2;

#[cfg(test)]
#[path = "type_health_tests.rs"]
mod tests;

pub(super) fn produce(_config: &LensConfig, context: &RunContext) -> Result<Value> {
    if let Some(inventory) = &context.compiler_inventory
        && inventory.has_profiles()
    {
        return Ok(inventory.type_rows(context.snapshot.fingerprint["complete"] == true));
    }
    let confidence = context.type_confidence();
    let metadata = tool_score_metadata("type_health");
    let provenance = provenance();
    let impls = context.type_impl_index();
    let mut rows = context
        .source_facts
        .iter()
        .filter(|fact| fact.parse_status == "ok")
        .flat_map(|fact| {
            fact.items
                .types
                .iter()
                .map(|ty| type_row(fact, ty, impls, &metadata, &provenance, &confidence))
        })
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| {
        b.structural_risk
            .unwrap_or(-1.0)
            .total_cmp(&a.structural_risk.unwrap_or(-1.0))
            .then_with(|| b.observed_method_count.cmp(&a.observed_method_count))
            .then_with(|| b.declaration.field_count.cmp(&a.declaration.field_count))
            .then_with(|| {
                (
                    a.package_name,
                    a.target_name,
                    &a.declaration.qualified_name,
                    &a.declaration.path,
                    a.declaration.line,
                )
                    .cmp(&(
                        b.package_name,
                        b.target_name,
                        &b.declaration.qualified_name,
                        &b.declaration.path,
                        b.declaration.line,
                    ))
            })
    });
    Ok(serde_json::to_value(rows)?)
}

#[derive(Serialize)]
struct TypeHealthRecord<'a> {
    #[serde(flatten)]
    declaration: &'a TypeFact,
    module_id: &'a str,
    package_name: &'a str,
    target_name: &'a str,
    identity_backend: &'a str,
    method_count: Option<usize>,
    impl_block_count: Option<usize>,
    impl_file_count: Option<usize>,
    observed_method_count: usize,
    observed_impl_block_count: usize,
    observed_impl_file_count: usize,
    impl_files: Vec<&'a str>,
    structural_risk: Option<f64>,
    structural_score: Option<f64>,
    signals: Vec<String>,
    score_components: Vec<crate::measurement::TypeScoreComponent>,
    #[serde(flatten)]
    provenance: &'a crate::measurement::Provenance,
    #[serde(flatten)]
    metadata: &'a crate::measurement::ToolScoreMetadata,
    measurement_confidence: &'a Value,
    source: &'static str,
    mock: bool,
}

fn type_row<'a>(
    fact: &'a FileFacts,
    ty: &'a TypeFact,
    impls: &'a TypeImplIndex,
    metadata: &'a crate::measurement::ToolScoreMetadata,
    provenance: &'a crate::measurement::Provenance,
    confidence: &'a Value,
) -> TypeHealthRecord<'a> {
    let summary = impls.get(fact, &ty.qualified_name);
    let methods = summary.map_or(0, |item| item.method_count);
    let blocks = summary.map_or(0, |item| item.block_count);
    let files = summary
        .into_iter()
        .flat_map(|item| item.files.iter().map(String::as_str))
        .collect::<Vec<_>>();
    let complete = confidence["complete"] == true;
    let (risk, signals, components) = if complete {
        let (risk, signals, components) = type_risk(ty, methods, blocks, files.len());
        (Some(risk), signals, components)
    } else {
        (
            None,
            vec![
                "incomplete type/impl inventory; only resolved source observations are shown"
                    .to_string(),
            ],
            Vec::new(),
        )
    };
    TypeHealthRecord {
        declaration: ty,
        module_id: &fact.module_id,
        package_name: &fact.package_name,
        target_name: &fact.target_name,
        identity_backend: &fact.identity_backend,
        method_count: complete.then_some(methods),
        impl_block_count: complete.then_some(blocks),
        impl_file_count: complete.then_some(files.len()),
        observed_method_count: methods,
        observed_impl_block_count: blocks,
        observed_impl_file_count: files.len(),
        impl_files: files,
        structural_risk: risk,
        structural_score: risk.map(|risk| round2(100.0 - risk)),
        signals,
        score_components: components,
        provenance,
        metadata,
        measurement_confidence: confidence,
        source: "static_type_health",
        mock: false,
    }
}
