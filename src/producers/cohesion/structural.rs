//! Syntax-observed inherent-method relationships, not declaration-count cohesion.
use crate::{
    config::LensConfig,
    facts::RunContext,
    util::{project_relative_path, round2},
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use syn::visit::{self, Visit};

pub(super) fn produce(config: &LensConfig, context: &RunContext) -> Value {
    let mut records = Vec::new();
    let mut reasons = Vec::new();
    for fact in context.source_facts.iter() {
        let path = project_relative_path(&config.project_root, &fact.path);
        match std::fs::read_to_string(&fact.path)
            .ok()
            .and_then(|text| syn::parse_file(&text).ok())
        {
            Some(file) => {
                let (rows, limitations) = analyze(&file);
                reasons.extend(
                    limitations
                        .into_iter()
                        .map(|reason| format!("{path}: {reason}")),
                );
                for mut row in rows {
                    row["path"] = json!(path);
                    row["module_id"] = json!(fact.module_id);
                    row["package_name"] = json!(fact.package_name);
                    row["target_name"] = json!(fact.target_name);
                    records.push(row);
                }
            }
            None => reasons.push(format!("{path}: source unreadable or not fully parseable")),
        }
    }
    if context.source_confidence()["complete"] != true {
        reasons.push("source inventory is incomplete".into());
    }
    json!({"model_version": 1, "calibration": "uncalibrated-advisory",
        "scope": "receiver methods in same-file, same-lexical-module inherent impls of locally declared structs; excludes trait impls, generated methods and other files",
        "compiler_verified": false, "records": records,
        "measurement_confidence": confidence(&reasons)})
}

fn confidence(reasons: &[String]) -> Value {
    json!({"complete": reasons.is_empty(), "partial": !reasons.is_empty(),
        "confidence_scope": "same_file_inherent_method_relationships",
        "required_inputs": ["parseable_rust_source", "direct_receiver_accesses"],
        "observed_inputs": {}, "missing_input": [], "stale_input": [], "unsupported_pattern": reasons})
}

#[derive(Default)]
struct Group {
    fields: BTreeSet<String>,
    declaration_count: usize,
    methods: Vec<Method>,
    excluded_static_methods: usize,
    excluded_trait_methods: usize,
    reasons: BTreeSet<String>,
}

#[derive(Default)]
struct Method {
    name: String,
    line: usize,
    fields: BTreeSet<String>,
    calls: BTreeSet<String>,
    reasons: BTreeSet<String>,
}

fn analyze(file: &syn::File) -> (Vec<Value>, Vec<String>) {
    let mut groups = BTreeMap::new();
    let mut reasons = Vec::new();
    collect_items(
        &file.items,
        "",
        conditional(&file.attrs),
        &mut groups,
        &mut reasons,
    );
    let mut rows = Vec::new();
    for (name, mut group) in groups {
        if group.methods.is_empty() {
            continue;
        }
        if group.declaration_count != 1 {
            group
                .reasons
                .insert("local struct declaration is missing or ambiguous".into());
        }
        let names: BTreeSet<_> = group
            .methods
            .iter()
            .map(|method| method.name.clone())
            .collect();
        if names.len() != group.methods.len() {
            group
                .reasons
                .insert("duplicate inherent method names (possibly cfg-dependent)".into());
        }
        for method in &group.methods {
            group.reasons.extend(method.reasons.iter().cloned());
            if !method.fields.is_subset(&group.fields) {
                group.reasons.insert("receiver field is not a declared field (possible Deref or unresolved ownership)".into());
            }
            if !method.calls.is_subset(&names) {
                group.reasons.insert(
                    "receiver call resolves outside the measured inherent-method group".into(),
                );
            }
        }
        reasons.extend(
            group
                .reasons
                .iter()
                .map(|reason| format!("{name}: {reason}")),
        );
        rows.push(group_report(&name, &group));
    }
    (rows, reasons)
}

fn conditional(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .any(|attr| attr.path().is_ident("cfg") || attr.path().is_ident("cfg_attr"))
}

fn type_name(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Path(path)
            if path.qself.is_none()
                && path.path.leading_colon.is_none()
                && path.path.segments.len() == 1 =>
        {
            path.path
                .segments
                .first()
                .map(|segment| segment.ident.to_string())
        }
        _ => None,
    }
}

fn qualified(module: &str, name: &str) -> String {
    if module.is_empty() {
        name.to_string()
    } else {
        format!("{module}::{name}")
    }
}

fn collect_items(
    items: &[syn::Item],
    module: &str,
    conditional_parent: bool,
    groups: &mut BTreeMap<String, Group>,
    reasons: &mut Vec<String>,
) {
    for item in items {
        match item {
            syn::Item::Mod(item) => {
                if let Some((_, items)) = &item.content {
                    collect_items(
                        items,
                        &qualified(module, &item.ident.to_string()),
                        conditional_parent || conditional(&item.attrs),
                        groups,
                        reasons,
                    );
                }
            }
            syn::Item::Struct(item) => {
                let group = groups
                    .entry(qualified(module, &item.ident.to_string()))
                    .or_default();
                group.declaration_count += 1;
                group
                    .fields
                    .extend(item.fields.iter().enumerate().map(|(index, field)| {
                        field
                            .ident
                            .as_ref()
                            .map(ToString::to_string)
                            .unwrap_or_else(|| index.to_string())
                    }));
                if conditional_parent
                    || conditional(&item.attrs)
                    || item.fields.iter().any(|field| conditional(&field.attrs))
                {
                    group.reasons.insert(
                        "cfg-dependent declaration inventory is not compiler-selected".into(),
                    );
                }
            }
            syn::Item::Impl(item) => {
                let Some(name) = type_name(&item.self_ty) else {
                    reasons.push("qualified or non-path impl ownership is outside same-module cohesion scope".into());
                    continue;
                };
                let group = groups.entry(qualified(module, &name)).or_default();
                for method in &item.items {
                    let syn::ImplItem::Fn(method) = method else {
                        if matches!(method, syn::ImplItem::Macro(_)) {
                            group
                                .reasons
                                .insert("unexpanded impl macro may contain methods".into());
                        }
                        continue;
                    };
                    if item.trait_.is_some() {
                        group.excluded_trait_methods += 1;
                        continue;
                    }
                    if method.sig.receiver().is_none() {
                        group.excluded_static_methods += 1;
                        continue;
                    }
                    let mut observation = Method {
                        name: method.sig.ident.to_string(),
                        line: method.sig.ident.span().start().line,
                        ..Default::default()
                    };
                    if conditional_parent || conditional(&item.attrs) || conditional(&method.attrs)
                    {
                        observation
                            .reasons
                            .insert("cfg-dependent methods are not compiler-selected".into());
                    }
                    observation.visit_block(&method.block);
                    group.methods.push(observation);
                }
            }
            syn::Item::Macro(item) if item.ident.is_none() => {
                reasons.push("unexpanded item macro may contain declarations or impls".into());
            }
            _ => {}
        }
    }
}

fn is_self(expression: &syn::Expr) -> bool {
    match expression {
        syn::Expr::Path(path) => path.qself.is_none() && path.path.is_ident("self"),
        syn::Expr::Paren(paren) => is_self(&paren.expr),
        syn::Expr::Group(group) => is_self(&group.expr),
        _ => false,
    }
}

impl<'ast> Visit<'ast> for Method {
    fn visit_expr_field(&mut self, field: &'ast syn::ExprField) {
        if is_self(&field.base) {
            self.fields.insert(match &field.member {
                syn::Member::Named(name) => name.to_string(),
                syn::Member::Unnamed(index) => index.index.to_string(),
            });
        } else {
            visit::visit_expr_field(self, field);
        }
    }
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        if is_self(&call.receiver) {
            self.calls.insert(call.method.to_string());
            for arg in &call.args {
                self.visit_expr(arg);
            }
        } else {
            visit::visit_expr_method_call(self, call);
        }
    }
    fn visit_expr_path(&mut self, path: &'ast syn::ExprPath) {
        if path.path.is_ident("self") {
            self.reasons.insert(
                "whole receiver escape, alias, destructuring or indirect access is not resolved"
                    .into(),
            );
        }
        visit::visit_expr_path(self, path);
    }
    fn visit_macro(&mut self, _: &'ast syn::Macro) {
        self.reasons
            .insert("macro body accesses are not expanded".into());
    }
    fn visit_attribute(&mut self, attr: &'ast syn::Attribute) {
        if conditional(std::slice::from_ref(attr)) {
            self.reasons
                .insert("cfg-dependent method body is not compiler-selected".into());
        }
    }
    // Nested items have their own receiver, not the enclosing method's state.
    fn visit_item(&mut self, _: &'ast syn::Item) {}
}

fn group_report(name: &str, group: &Group) -> Value {
    let n = group.methods.len();
    let possible = n * n.saturating_sub(1) / 2;
    let mut shared_pairs = 0usize;
    let mut adjacency = vec![BTreeSet::new(); n];
    for a in 0..n {
        for b in a + 1..n {
            let left = &group.methods[a];
            let right = &group.methods[b];
            let shared = !left.fields.is_disjoint(&right.fields);
            shared_pairs += usize::from(shared);
            if shared || left.calls.contains(&right.name) || right.calls.contains(&left.name) {
                adjacency[a].insert(b);
                adjacency[b].insert(a);
            }
        }
    }
    let components = components(&adjacency);
    let connected_pairs: usize = components
        .iter()
        .map(|component| component.len() * component.len().saturating_sub(1) / 2)
        .sum();
    let has_state = group.methods.iter().any(|method| !method.fields.is_empty());
    let applicable = possible > 0 && has_state;
    let complete = group.reasons.is_empty();
    let methods: Vec<_> = group.methods.iter().map(|method| json!({"name": method.name, "line": method.line,
        "accessed_fields": method.fields, "receiver_calls": method.calls, "limitations": method.reasons})).collect();
    let clusters: Vec<_> = components
        .iter()
        .map(|indices| {
            indices
                .iter()
                .map(|index| group.methods[*index].name.as_str())
                .collect::<Vec<_>>()
        })
        .collect();
    json!({"qualified_type_name": name, "receiver_method_count": n,
        "excluded_static_method_count": group.excluded_static_methods, "excluded_trait_method_count": group.excluded_trait_methods,
        "possible_method_pairs": possible, "observed_shared_field_pairs": shared_pairs,
        "observed_connected_method_pairs": connected_pairs, "observed_components": clusters,
        "field_sharing_percent": (complete && applicable).then(|| round2(100.0 * shared_pairs as f64 / possible as f64)),
        "connected_pair_percent": (complete && applicable).then(|| round2(100.0 * connected_pairs as f64 / possible as f64)),
        "component_count": (complete && applicable).then_some(components.len()),
        "applicable": applicable, "not_applicable_reason": if possible == 0 { Some("fewer than two receiver methods") }
            else if !has_state { Some("no directly observed receiver state") } else { None },
        "methods": methods, "measurement_confidence": confidence(&group.reasons.iter().cloned().collect::<Vec<_>>())})
}

fn components(adjacency: &[BTreeSet<usize>]) -> Vec<Vec<usize>> {
    let mut unseen: BTreeSet<_> = (0..adjacency.len()).collect();
    let mut groups = Vec::new();
    while let Some(start) = unseen.pop_first() {
        let mut stack = vec![start];
        let mut group = Vec::new();
        while let Some(index) = stack.pop() {
            group.push(index);
            for neighbor in &adjacency[index] {
                if unseen.remove(neighbor) {
                    stack.push(*neighbor);
                }
            }
        }
        group.sort();
        groups.push(group);
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rows(source: &str) -> anyhow::Result<Vec<Value>> {
        Ok(analyze(&syn::parse_file(source)?).0)
    }
    #[test]
    fn shared_fields_delegation_and_disjoint_state_have_distinct_metrics() -> anyhow::Result<()> {
        let records = rows(
            "struct S { a: i32, b: i32 } impl S {
            fn new() -> Self { loop {} }
            fn a(&self) -> i32 { self.a }
            fn a2(&mut self) { self.a = 2; }
            fn delegate(&self) -> i32 { self.a() }
            fn b(&self) -> i32 { self.b }
        } impl Clone for S { fn clone(&self) -> Self { loop {} } }",
        )?;
        let row = &records[0];
        assert_eq!(row["receiver_method_count"], 4);
        assert_eq!(row["possible_method_pairs"], 6);
        assert_eq!(row["field_sharing_percent"], 16.67);
        assert_eq!(row["connected_pair_percent"], 50.0);
        assert_eq!(row["component_count"], 2);
        assert_eq!(row["excluded_static_method_count"], 1);
        assert_eq!(row["excluded_trait_method_count"], 1);
        Ok(())
    }
    #[test]
    fn incomplete_and_stateless_cohorts_abstain_from_ratios() -> anyhow::Result<()> {
        for body in [
            "let alias = self; alias.x",
            "unknown!(self.x)",
            "self.outside()",
        ] {
            let source = format!(
                "struct S {{ x: i32 }} impl S {{ fn a(&self) -> i32 {{ {body} }} fn b(&self) -> i32 {{ self.x }} }}"
            );
            let records = rows(&source)?;
            assert_eq!(records[0]["measurement_confidence"]["complete"], false);
            assert!(records[0]["field_sharing_percent"].is_null());
        }
        for source in [
            "struct S; impl S { fn a(&self) {} fn b(&self) {} }",
            "struct S(i32); impl S { fn a(&self) -> i32 { self.0 } }",
        ] {
            let records = rows(source)?;
            assert_eq!(records[0]["applicable"], false);
            assert!(records[0]["component_count"].is_null());
        }
        Ok(())
    }
    #[test]
    fn lexical_identity_multiple_impls_tuple_fields_and_cfg_are_preserved() -> anyhow::Result<()> {
        let records = rows("mod a { struct S(i32); impl S { fn a(&self) -> i32 { self.0 } }
            impl S { fn b(&self) -> i32 { self.0 } } }
            mod b { struct S(i32); #[cfg(unix)] impl S { fn a(&self) -> i32 { self.0 } fn b(&self) -> i32 { self.0 } } }")?;
        assert_eq!(records.len(), 2);
        assert_eq!(records[0]["qualified_type_name"], "a::S");
        assert_eq!(records[0]["field_sharing_percent"], 100.0);
        assert_eq!(records[1]["measurement_confidence"]["complete"], false);
        assert!(records[1]["field_sharing_percent"].is_null());
        Ok(())
    }
}
