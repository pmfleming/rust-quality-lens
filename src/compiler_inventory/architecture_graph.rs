//! Per-context compiler graph projection. No syntax fallback or pressure scores.
use super::{
    Origin, Profile,
    architecture::{Module, Reference, Target},
};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct Node {
    pub(crate) id: String,
    pub(crate) selector: String,
    pub(crate) compiler_context_id: Option<String>,
    pub(crate) module_path: String,
    pub(crate) package_name: Option<String>,
    pub(crate) origin: Option<Origin>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct Edge {
    pub(crate) source_id: String,
    pub(crate) target_id: String,
    pub(crate) observations: Vec<Reference>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct Graph {
    pub(crate) compiler_context_id: String,
    pub(crate) modules: BTreeMap<String, Node>,
    pub(crate) edges: Vec<Edge>,
    pub(crate) unresolved_references: Vec<Reference>,
    pub(crate) missing_evidence: Vec<String>,
}

fn node(profile: &Profile, module: &Module) -> Node {
    let context = profile.context_id();
    Node {
        id: format!("{context}::{}", module.id),
        // Compatibility view only. Actual IDs remain compiler/context-qualified.
        selector: format!(
            "{}::shared::{}",
            profile.package_name,
            display_path(&module.path)
        ),
        compiler_context_id: Some(context),
        module_path: module.path.clone(),
        package_name: Some(profile.package_name.clone()),
        origin: Some(module.origin.clone()),
    }
}

fn display_path(path: &str) -> &str {
    if path.is_empty() { "crate" } else { path }
}

fn external_target(
    profiles: &[Profile],
    crate_id: &str,
    name: &str,
    module_path: &str,
    module_id: &str,
    definition_id: &str,
) -> Result<Node, String> {
    let matches = profiles
        .iter()
        .filter(|p| {
            p.architecture
                .as_ref()
                .is_some_and(|a| a.crate_id == crate_id)
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => Ok(Node {
            id: format!("external::{crate_id}::{module_id}"),
            selector: format!("external::{name}::{}", display_path(module_path)),
            compiler_context_id: None,
            module_path: module_path.into(),
            package_name: None,
            origin: None,
        }),
        [profile] => {
            let capture = profile
                .architecture
                .as_ref()
                .ok_or("missing target architecture")?;
            let module = capture
                .modules
                .iter()
                .find(|m| m.id == module_id && m.path == module_path)
                .ok_or("compiler-linked target module is not inventoried")?;
            if capture
                .declarations
                .iter()
                .any(|d| d.id == definition_id && d.module_id != module.id)
            {
                return Err("compiler-linked target declaration and module disagree".into());
            }
            Ok(node(profile, module))
        }
        _ => Err("compiler crate identity matches multiple compilation contexts".into()),
    }
}

pub(super) fn project(profiles: &[Profile]) -> Vec<Graph> {
    profiles
        .iter()
        .map(|profile| project_context(profile, profiles))
        .collect()
}

fn project_context(profile: &Profile, profiles: &[Profile]) -> Graph {
    let mut graph = Graph {
        compiler_context_id: profile.context_id(),
        modules: BTreeMap::new(),
        edges: Vec::new(),
        unresolved_references: Vec::new(),
        missing_evidence: Vec::new(),
    };
    let Some(capture) = &profile.architecture else {
        graph
            .missing_evidence
            .push("adapter did not provide architecture capture".into());
        return graph;
    };
    graph.missing_evidence = capture.missing_evidence();
    let modules = capture
        .modules
        .iter()
        .map(|m| (m.id.as_str(), node(profile, m)))
        .collect::<BTreeMap<_, _>>();
    let declarations = capture
        .declarations
        .iter()
        .map(|d| (d.id.as_str(), d.module_id.as_str()))
        .collect::<BTreeMap<_, _>>();
    graph.modules = modules
        .values()
        .map(|m| (m.id.clone(), m.clone()))
        .collect();
    let owner = |id: &str| {
        declarations
            .get(id)
            .and_then(|module| modules.get(module))
            .cloned()
    };
    let mut edges = BTreeMap::<(String, String), Vec<Reference>>::new();
    for reference in &capture.references {
        let target = match &reference.target {
            Target::Local { declaration_id } => {
                owner(declaration_id).ok_or_else(|| "unknown local endpoint".into())
            }
            Target::External {
                crate_id,
                crate_name,
                definition_id,
                module_path,
                module_id,
            } => external_target(
                profiles,
                crate_id,
                crate_name,
                module_path,
                module_id,
                definition_id,
            ),
            Target::Unknown { reason } => Err(reason.clone()),
        };
        match (owner(&reference.source_id), target) {
            (Some(source), Ok(target)) => {
                graph.modules.insert(target.id.clone(), target.clone());
                if source.id != target.id {
                    edges
                        .entry((source.id, target.id))
                        .or_default()
                        .push(reference.clone());
                }
            }
            (_, target) => {
                graph.missing_evidence.push(
                    target
                        .err()
                        .unwrap_or_else(|| "unknown source declaration owner".into()),
                );
                graph.unresolved_references.push(reference.clone());
            }
        }
    }
    graph.edges = edges
        .into_iter()
        .map(|((source_id, target_id), observations)| Edge {
            source_id,
            target_id,
            observations,
        })
        .collect();
    graph.missing_evidence.sort();
    graph.missing_evidence.dedup();
    graph
}
