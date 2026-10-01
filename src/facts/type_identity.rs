//! Conservative syntax-only type/impl joins. Never guess a symbol by suffix.
use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use super::{FileFacts, ImplFact};

type TypeIdentity = (String, String, String);

#[derive(Default)]
pub(crate) struct ImplSummary {
    pub(crate) method_count: usize,
    pub(crate) block_count: usize,
    pub(crate) files: BTreeSet<String>,
}

#[derive(Default)]
pub(crate) struct TypeImplIndex {
    by_type: BTreeMap<TypeIdentity, ImplSummary>,
    unsupported: BTreeSet<String>,
    unresolved_impls: usize,
}

fn identity(fact: &FileFacts, name: &str) -> TypeIdentity {
    (
        fact.package_name.clone(),
        fact.target_name.clone(),
        name.to_string(),
    )
}

impl TypeImplIndex {
    pub(crate) fn new(facts: &[FileFacts]) -> Self {
        let mut definitions = BTreeMap::<(&str, &str), BTreeSet<String>>::new();
        let mut ambiguous = BTreeSet::new();
        for fact in facts {
            let scope = definitions
                .entry((fact.package_name.as_str(), fact.target_name.as_str()))
                .or_default();
            for ty in &fact.items.types {
                if !scope.insert(ty.qualified_name.clone()) {
                    ambiguous.insert(identity(fact, &ty.qualified_name));
                }
            }
        }
        let mut index = Self::default();
        index.unsupported.extend(
            facts
                .iter()
                .flat_map(|fact| fact.items.type_identity_limitations.iter().cloned()),
        );
        for (package, target, name) in &ambiguous {
            index.unsupported.insert(format!("duplicate type identity {package}::{target}::{name}; conditional/local scopes are not resolved"));
        }
        for fact in facts {
            let scope = definitions.get(&(fact.package_name.as_str(), fact.target_name.as_str()));
            for imp in &fact.items.impls {
                index.record_impl(fact, imp, scope, &ambiguous);
            }
        }
        index
    }

    fn record_impl(
        &mut self,
        fact: &FileFacts,
        imp: &ImplFact,
        definitions: Option<&BTreeSet<String>>,
        ambiguous: &BTreeSet<TypeIdentity>,
    ) {
        let owner = definitions
            .and_then(|definitions| impl_identity(imp, definitions))
            .map(|name| identity(fact, &name))
            .filter(|key| !ambiguous.contains(key));
        if let Some(owner) = owner {
            let summary = self.by_type.entry(owner).or_default();
            summary.method_count += imp.method_count;
            summary.block_count += 1;
            summary.files.insert(imp.path.clone());
        } else {
            self.unresolved_impls += 1;
            self.unsupported.insert(format!("{}: unresolved impl owner {} in {}::{}::{}; imports, aliases and external owners require semantic resolution", imp.path, imp.qualified_type_name, fact.package_name, fact.target_name, imp.module_key));
        }
    }

    pub(crate) fn get(&self, fact: &FileFacts, name: &str) -> Option<&ImplSummary> {
        self.by_type.get(&identity(fact, name))
    }

    pub(crate) fn confidence(&self, mut source: Value, fingerprint_complete: bool) -> Value {
        let complete =
            source["complete"] == true && self.unsupported.is_empty() && fingerprint_complete;
        source["complete"] = json!(complete);
        source["partial"] = json!(!complete);
        source["confidence_scope"] = json!("syntax_types_and_impl_identity");
        if let Some(required) = source["required_inputs"].as_array_mut() {
            required.push(json!("unambiguous_impl_owners"));
        }
        source["type_identity"] = json!({
            "resolved_impl_blocks": self.by_type.values().map(|summary| summary.block_count).sum::<usize>(),
            "unresolved_impl_blocks": self.unresolved_impls,
            "count_scope": "explicit source impl observations before cfg/macro expansion, not compiled type totals",
        });
        if let Some(reasons) = source["unsupported_pattern"].as_array_mut() {
            reasons.extend(self.unsupported.iter().map(|reason| json!(reason)));
        }
        if !fingerprint_complete && let Some(missing) = source["missing_input"].as_array_mut() {
            missing.push(json!("type identity input fingerprint is incomplete"));
        }
        source
    }
}

pub(crate) fn impl_identity(imp: &ImplFact, definitions: &BTreeSet<String>) -> Option<String> {
    let raw = if imp.qualified_type_name.is_empty() {
        &imp.type_name
    } else {
        &imp.qualified_type_name
    };
    let candidate = impl_candidate(raw, &imp.module_key)?;
    if definitions.contains(&candidate) {
        return Some(candidate);
    }
    // Extractor root declarations use lib::/main::, unlike inline modules.
    // Only correct that root representation; never search arbitrary suffixes.
    let roots = [format!("lib::{candidate}"), format!("main::{candidate}")];
    let mut matches = roots.into_iter().filter(|name| definitions.contains(name));
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

fn impl_candidate(raw: &str, module: &str) -> Option<String> {
    if let Some(rest) = raw.strip_prefix("crate::") {
        return Some(rest.to_string());
    }
    let mut base = if matches!(module, "lib" | "main") {
        Vec::new()
    } else {
        module.split("::").collect::<Vec<_>>()
    };
    let mut remainder = raw.strip_prefix("self::").unwrap_or(raw);
    while let Some(rest) = remainder.strip_prefix("super::") {
        base.pop()?;
        remainder = rest;
    }
    Some(
        base.into_iter()
            .chain(std::iter::once(remainder))
            .collect::<Vec<_>>()
            .join("::"),
    )
}
