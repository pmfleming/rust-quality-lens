use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

use crate::facts::{FileFacts, ModuleFileFact};

#[derive(Clone)]
pub(crate) struct ModuleInfo {
    pub(crate) key: String,
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
}

pub(crate) fn module_graph(facts: &[FileFacts]) -> ModuleGraph {
    let modules = module_index(facts);
    let keys = modules.keys().cloned().collect::<Vec<_>>();
    let mut dependencies = HashMap::new();
    let mut containment_dependencies = HashMap::new();
    for fact in facts {
        record_containment(fact, &modules, &mut containment_dependencies);
        record_dependencies(fact, &keys, &mut dependencies);
    }
    let reverse_dependencies = reverse_index(&dependencies);
    ModuleGraph {
        facts: facts.to_vec(),
        modules,
        dependencies,
        containment_dependencies,
        reverse_dependencies,
    }
}

fn module_index(facts: &[FileFacts]) -> BTreeMap<String, ModuleInfo> {
    let mut modules = BTreeMap::new();
    for fact in facts {
        if fact.parse_status == "ok" {
            modules.insert(
                fact.module_key.clone(),
                ModuleInfo {
                    key: fact.module_key.clone(),
                    path: PathBuf::from(&fact.path),
                    target_kind: fact.target_kind.clone(),
                    entrypoint_kind: fact.entrypoint_kind.clone(),
                    is_entrypoint: fact.is_entrypoint,
                    public_api_count: fact.graph.public_api_count,
                    source_nonblank_line_count: fact.source.source_nonblank_line_count,
                },
            );
        }
        for module_file in &fact.graph.module_files {
            modules
                .entry(module_file.module_key.clone())
                .or_insert_with(|| module_file_info(module_file));
        }
    }
    modules
}

fn module_file_info(module_file: &ModuleFileFact) -> ModuleInfo {
    ModuleInfo {
        key: module_file.module_key.clone(),
        path: PathBuf::from(&module_file.path),
        target_kind: "module".to_string(),
        entrypoint_kind: None,
        is_entrypoint: false,
        public_api_count: 0,
        source_nonblank_line_count: 0,
    }
}

fn record_containment(
    fact: &FileFacts,
    modules: &BTreeMap<String, ModuleInfo>,
    containment_dependencies: &mut HashMap<String, HashSet<String>>,
) {
    for child in &fact.graph.child_modules {
        if modules.contains_key(child) && child != &fact.module_key {
            containment_dependencies
                .entry(fact.module_key.clone())
                .or_default()
                .insert(child.clone());
        }
    }
}

fn record_dependencies(
    fact: &FileFacts,
    module_keys: &[String],
    dependencies: &mut HashMap<String, HashSet<String>>,
) {
    let entry = dependencies.entry(fact.module_key.clone()).or_default();
    for raw in &fact.graph.dependencies {
        if let Some(target) = resolve_dependency(raw, &fact.module_key, module_keys)
            && target != fact.module_key
        {
            entry.insert(target);
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
    use super::resolve_dependency;

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
}
