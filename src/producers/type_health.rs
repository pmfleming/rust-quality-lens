use anyhow::Result;
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

use crate::config::LensConfig;
use crate::facts::{FileFacts, ImplFact, RunContext, TypeFact};
use crate::measurement::{provenance, tool_score_metadata, type_risk};
use crate::util::round2;

#[cfg(test)]
#[path = "type_health_tests.rs"]
mod tests;

#[derive(Default)]
struct ImplSummary {
    method_count: usize,
    block_count: usize,
    files: BTreeSet<String>,
}

pub(super) fn produce(_config: &LensConfig, context: &RunContext) -> Result<Value> {
    let facts = &context.source_facts;
    let confidence = context.source_confidence();
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
        b.structural_risk
            .total_cmp(&a.structural_risk)
            .then_with(|| b.method_count.cmp(&a.method_count))
            .then_with(|| b.declaration.field_count.cmp(&a.declaration.field_count))
            .then_with(|| {
                a.declaration
                    .qualified_name
                    .cmp(&b.declaration.qualified_name)
            })
    });
    Ok(serde_json::to_value(rows)?)
}

type TypeIdentity = (String, String, String);

fn type_identity(fact: &FileFacts, name: &str) -> TypeIdentity {
    (
        fact.package_name.clone(),
        fact.target_name.clone(),
        name.to_string(),
    )
}

fn impl_index(facts: &[FileFacts]) -> BTreeMap<TypeIdentity, ImplSummary> {
    let mut definitions = BTreeMap::<(&str, &str), BTreeSet<String>>::new();
    for fact in facts {
        definitions
            .entry((&fact.package_name, &fact.target_name))
            .or_default()
            .extend(fact.items.types.iter().map(|ty| ty.qualified_name.clone()));
    }
    let mut impls = BTreeMap::<TypeIdentity, ImplSummary>::new();
    for fact in facts {
        let Some(definitions) =
            definitions.get(&(fact.package_name.as_str(), fact.target_name.as_str()))
        else {
            continue;
        };
        for imp in &fact.items.impls {
            let Some(identity) = impl_identity(imp, definitions) else {
                continue;
            };
            let entry = impls.entry(type_identity(fact, &identity)).or_default();
            entry.method_count += imp.method_count;
            entry.block_count += 1;
            entry.files.insert(imp.path.clone());
        }
    }
    impls
}

fn impl_identity(imp: &ImplFact, definitions: &BTreeSet<String>) -> Option<String> {
    let raw = if imp.qualified_type_name.is_empty() {
        imp.type_name.as_str()
    } else {
        imp.qualified_type_name.as_str()
    };
    let candidate = impl_candidate(raw, &imp.module_key);
    definitions
        .contains(&candidate)
        .then_some(candidate)
        .or_else(|| unique_suffix_match(raw, definitions))
}

fn impl_candidate(raw: &str, module: &str) -> String {
    if let Some(rest) = raw.strip_prefix("crate::") {
        rest.to_string()
    } else if let Some(rest) = raw.strip_prefix("self::") {
        format!("{module}::{rest}")
    } else if raw.starts_with("super::") {
        resolve_super_type(raw, module)
    } else if raw.contains("::") {
        raw.to_string()
    } else {
        format!("{module}::{raw}")
    }
}

fn unique_suffix_match(raw: &str, definitions: &BTreeSet<String>) -> Option<String> {
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

#[derive(Serialize)]
struct TypeHealthRecord<'a> {
    #[serde(flatten)]
    declaration: &'a TypeFact,
    module_id: &'a str,
    package_name: &'a str,
    target_name: &'a str,
    identity_backend: &'a str,
    method_count: usize,
    impl_block_count: usize,
    impl_file_count: usize,
    impl_files: Vec<&'a str>,
    structural_risk: f64,
    structural_score: f64,
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
    impls: &'a BTreeMap<TypeIdentity, ImplSummary>,
    metadata: &'a crate::measurement::ToolScoreMetadata,
    provenance: &'a crate::measurement::Provenance,
    confidence: &'a Value,
) -> TypeHealthRecord<'a> {
    let summary = impls.get(&type_identity(fact, &ty.qualified_name));
    let method_count = summary.map_or(0, |item| item.method_count);
    let impl_block_count = summary.map_or(0, |item| item.block_count);
    let impl_files = summary
        .into_iter()
        .flat_map(|item| item.files.iter().map(String::as_str))
        .collect::<Vec<_>>();
    let (risk, signals, score_components) =
        type_risk(ty, method_count, impl_block_count, impl_files.len());
    TypeHealthRecord {
        declaration: ty,
        module_id: &fact.module_id,
        package_name: &fact.package_name,
        target_name: &fact.target_name,
        identity_backend: &fact.identity_backend,
        method_count,
        impl_block_count,
        impl_file_count: impl_files.len(),
        impl_files,
        structural_risk: risk,
        structural_score: round2(100.0 - risk),
        signals,
        score_components,
        provenance,
        metadata,
        measurement_confidence: confidence,
        source: "static_type_health",
        mock: false,
    }
}
