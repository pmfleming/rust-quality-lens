use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::PathBuf;

use crate::facts::FileFacts;

#[derive(Clone)]
pub(crate) struct ModuleInfo {
    /// Collision-safe graph key. This is the Cargo-qualified module identity.
    pub(crate) key: String,
    /// Human-readable Rust module path within its package.
    pub(crate) module_key: String,
    pub(crate) id: String,
    pub(crate) package_name: String,
    pub(crate) target_name: String,
    pub(crate) identity_backend: String,
    pub(crate) path: PathBuf,
    pub(crate) target_kind: String,
    pub(crate) entrypoint_kind: Option<String>,
    pub(crate) is_entrypoint: bool,
    pub(crate) public_api_count: usize,
    pub(crate) source_nonblank_line_count: usize,
}

pub(crate) struct ModuleGraph {
    pub(crate) facts: Vec<FileFacts>,
    pub(crate) modules: BTreeMap<String, ModuleInfo>,
    pub(crate) dependencies: HashMap<String, HashSet<String>>,
    pub(crate) containment_dependencies: HashMap<String, HashSet<String>>,
    pub(crate) reverse_dependencies: HashMap<String, HashSet<String>>,
    pub(crate) dependency_provenance: HashMap<(String, String), BTreeSet<String>>,
    pub(crate) dependency_symbols: HashMap<(String, String), BTreeSet<String>>,
}

pub(crate) fn module_graph(facts: &[FileFacts]) -> ModuleGraph {
    let modules = module_index(facts);
    let mut dependencies = HashMap::new();
    let mut containment_dependencies = HashMap::new();
    let mut dependency_provenance = HashMap::new();
    let mut dependency_symbols = HashMap::new();
    for fact in facts {
        record_containment(fact, &modules, &mut containment_dependencies);
        record_dependencies(
            fact,
            facts,
            &mut dependencies,
            &mut dependency_provenance,
            &mut dependency_symbols,
        );
    }
    let reverse_dependencies = reverse_index(&dependencies);
    ModuleGraph {
        facts: facts.to_vec(),
        modules,
        dependencies,
        containment_dependencies,
        reverse_dependencies,
        dependency_provenance,
        dependency_symbols,
    }
}

fn module_index(facts: &[FileFacts]) -> BTreeMap<String, ModuleInfo> {
    let mut modules = BTreeMap::new();
    for fact in facts {
        if fact.parse_status == "ok" {
            modules.insert(
                fact.module_id.clone(),
                ModuleInfo {
                    key: fact.module_id.clone(),
                    module_key: fact.module_key.clone(),
                    id: fact.module_id.clone(),
                    package_name: fact.package_name.clone(),
                    target_name: fact.target_name.clone(),
                    identity_backend: fact.identity_backend.clone(),
                    path: PathBuf::from(&fact.path),
                    target_kind: fact.target_kind.clone(),
                    entrypoint_kind: fact.entrypoint_kind.clone(),
                    is_entrypoint: fact.is_entrypoint,
                    public_api_count: fact.graph.public_api_count,
                    source_nonblank_line_count: fact.source.source_nonblank_line_count,
                },
            );
        }
        // Source facts already cover declared module files. Keeping declaration-only
        // placeholders would reintroduce unqualified keys in multi-package workspaces.
    }
    modules
}

fn record_containment(
    fact: &FileFacts,
    modules: &BTreeMap<String, ModuleInfo>,
    containment_dependencies: &mut HashMap<String, HashSet<String>>,
) {
    for child in &fact.graph.child_modules {
        let child_id = modules
            .values()
            .find(|module| module.package_name == fact.package_name && module.module_key == *child);
        if let Some(child) = child_id.filter(|child| child.key != fact.module_id) {
            containment_dependencies
                .entry(fact.module_id.clone())
                .or_default()
                .insert(child.key.clone());
        }
    }
}

fn record_dependencies(
    fact: &FileFacts,
    facts: &[FileFacts],
    dependencies: &mut HashMap<String, HashSet<String>>,
    provenance: &mut HashMap<(String, String), BTreeSet<String>>,
    symbols: &mut HashMap<(String, String), BTreeSet<String>>,
) {
    let scoped_facts = facts
        .iter()
        .filter(|candidate| candidate.package_name == fact.package_name)
        .collect::<Vec<_>>();
    let module_keys = scoped_facts
        .iter()
        .map(|candidate| candidate.module_key.clone())
        .collect::<Vec<_>>();
    let entry = dependencies.entry(fact.module_id.clone()).or_default();
    let mut semantically_resolved = HashSet::new();
    for resolved in &fact.graph.resolved_dependencies {
        let Some(target) = resolved.target_module_id.as_ref() else {
            continue;
        };
        semantically_resolved.insert(resolved.raw_path.clone());
        if target == &fact.module_id {
            continue;
        }
        entry.insert(target.clone());
        let edge = (fact.module_id.clone(), target.clone());
        provenance
            .entry(edge.clone())
            .or_default()
            .insert(resolved.backend.clone());
        if let Some(symbol) = &resolved.symbol_identity {
            symbols.entry(edge).or_default().insert(symbol.clone());
        }
    }
    for raw in &fact.graph.dependencies {
        if semantically_resolved.contains(raw) {
            continue;
        }
        if let Some(target_key) = resolve_dependency(raw, &fact.module_key, &module_keys)
            && target_key != fact.module_key
            && let Some(target) = scoped_facts
                .iter()
                .find(|candidate| candidate.module_key == target_key)
        {
            entry.insert(target.module_id.clone());
            provenance
                .entry((fact.module_id.clone(), target.module_id.clone()))
                .or_default()
                .insert("syntax_fallback".to_string());
        }
    }
}

fn reverse_index(
    dependencies: &HashMap<String, HashSet<String>>,
) -> HashMap<String, HashSet<String>> {
    let mut reverse_dependencies = HashMap::new();
    for (source, targets) in dependencies {
        for target in targets {
            reverse_dependencies
                .entry(target.clone())
                .or_insert_with(HashSet::new)
                .insert(source.clone());
        }
    }
    reverse_dependencies
}

pub(crate) fn resolve_dependency(
    raw: &str,
    source_module: &str,
    modules: &[String],
) -> Option<String> {
    let mut raw = raw.trim().trim_end_matches("::*").to_string();
    if raw.is_empty() {
        return None;
    }
    let mut relative_base = None;
    if let Some(stripped) = raw.strip_prefix("crate::") {
        raw = stripped.to_string();
    } else if let Some(stripped) = raw.strip_prefix("self::") {
        raw = format!("{source_module}::{stripped}");
    } else if raw.starts_with("super::") {
        relative_base = Some(source_module.to_string());
        raw = resolve_super_path(raw, source_module);
    }
    dependency_prefix(&raw, modules, relative_base.as_deref())
}

fn resolve_super_path(mut raw: String, source_module: &str) -> String {
    let mut base = source_module.split("::").collect::<Vec<_>>();
    while raw.starts_with("super::") {
        base.pop();
        raw = raw.trim_start_matches("super::").to_string();
    }
    base.into_iter()
        .chain(std::iter::once(raw.as_str()))
        .collect::<Vec<_>>()
        .join("::")
}

fn dependency_prefix(raw: &str, modules: &[String], relative_base: Option<&str>) -> Option<String> {
    let parts = raw
        .split("::")
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    for end in (1..=parts.len()).rev() {
        let candidate = parts[..end].join("::");
        if relative_base
            .is_some_and(|base| base == candidate || base.starts_with(&format!("{candidate}::")))
        {
            continue;
        }
        if modules.iter().any(|module| module == &candidate) {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{module_graph, resolve_dependency};
    use crate::facts::FileFacts;

    #[test]
    fn dependency_resolution_handles_prefix_forms() {
        let modules = vec![
            "domain".to_string(),
            "domain::model".to_string(),
            "service".to_string(),
            "service::worker".to_string(),
        ];
        assert_eq!(
            resolve_dependency("crate::domain::model::Thing", "service", &modules),
            Some("domain::model".to_string())
        );
        assert_eq!(
            resolve_dependency("self::worker::run", "service", &modules),
            Some("service::worker".to_string())
        );
        assert_eq!(
            resolve_dependency("super::domain::model", "service::worker", &modules),
            None
        );
    }

    #[test]
    fn cargo_qualified_ids_prevent_workspace_module_collisions() {
        let mut first = FileFacts::test_fact("/workspace/a/src/lib.rs", "lib");
        first.package_name = "a".to_string();
        first.target_name = "a".to_string();
        first.module_id = "a::a::lib".to_string();
        let mut second = FileFacts::test_fact("/workspace/b/src/lib.rs", "lib");
        second.package_name = "b".to_string();
        second.target_name = "b".to_string();
        second.module_id = "b::b::lib".to_string();

        let graph = module_graph(&[first, second]);
        assert_eq!(graph.modules.len(), 2);
        assert!(graph.modules.contains_key("a::a::lib"));
        assert!(graph.modules.contains_key("b::b::lib"));
    }
}
