use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::facts::ModuleGraph;
use crate::measurement::classify_module;

pub(crate) fn cycle_modules(dependencies: &HashMap<String, HashSet<String>>) -> BTreeSet<String> {
    dependencies
        .keys()
        .filter(|module| {
            dependencies
                .get(*module)
                .into_iter()
                .flatten()
                .any(|target| path_exists(target, module, dependencies, &mut HashSet::new()))
        })
        .cloned()
        .collect()
}

fn path_exists(
    current: &str,
    target: &str,
    dependencies: &HashMap<String, HashSet<String>>,
    seen: &mut HashSet<String>,
) -> bool {
    if current == target {
        return true;
    }
    seen.insert(current.to_string())
        && dependencies
            .get(current)
            .into_iter()
            .flatten()
            .any(|next| path_exists(next, target, dependencies, seen))
}

pub(crate) fn layer_violations(graph: &ModuleGraph) -> BTreeMap<String, usize> {
    graph
        .dependencies
        .iter()
        .filter_map(|(source, targets)| {
            let count = targets
                .iter()
                .filter(|target| is_layer_violation(source, target))
                .count();
            (count > 0).then(|| (source.clone(), count))
        })
        .collect()
}

pub(crate) fn is_layer_violation(source: &str, target: &str) -> bool {
    let Some(source_rank) = layer_rank(classify_module(source)) else {
        return false;
    };
    let Some(target_rank) = layer_rank(classify_module(target)) else {
        return false;
    };
    source_rank < target_rank
}

fn layer_rank(layer: &str) -> Option<u8> {
    match layer {
        "Tests" => None,
        "Interface" => Some(4),
        "Application" | "Infrastructure" => Some(3),
        "Domain" => Some(2),
        _ => None,
    }
}
