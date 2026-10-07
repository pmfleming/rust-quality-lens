//! Navigation is not ownership: unindexed local/build output must stay unknown.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::config::LensConfig;
use crate::facts::{DependencyReferenceFact, FileFacts, ResolvedDependencyFact};

use super::{
    DefinitionLocation, absolute_path, canonical_key, identifier_at, normalize_slashes,
    reference_symbol, unresolved, uri_path,
};

#[derive(Default)]
pub(super) struct DefinitionIndex {
    pub(super) modules: BTreeMap<String, (String, String)>,
    local_roots: BTreeSet<PathBuf>,
    target_root: Option<PathBuf>,
}

impl DefinitionIndex {
    pub(super) fn new(config: &LensConfig, facts: &[FileFacts]) -> Self {
        let mut local_roots = BTreeSet::from([PathBuf::from(canonical_key(&config.project_root))]);
        if let Some(metadata) = &config.cargo_metadata {
            local_roots.extend(
                metadata["packages"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|package| package["source"].is_null())
                    .filter_map(|package| Path::new(package["manifest_path"].as_str()?).parent())
                    .map(|root| PathBuf::from(canonical_key(root))),
            );
        }
        Self {
            modules: facts
                .iter()
                .map(|fact| {
                    (
                        canonical_key(absolute_path(config, &fact.path)),
                        (fact.module_id.clone(), fact.module_key.clone()),
                    )
                })
                .collect(),
            local_roots,
            target_root: Some(PathBuf::from(canonical_key(config.target_directory()))),
        }
    }

    fn unmapped_status(&self, path: &Path) -> Option<&'static str> {
        let path = PathBuf::from(canonical_key(path));
        if self
            .target_root
            .as_ref()
            .is_some_and(|root| path.starts_with(root))
        {
            // Cargo's target directory can also contain dependency build output.
            // Its directory name alone cannot establish a local or external owner.
            Some("unmapped_generated_definition")
        } else if self.local_roots.iter().any(|root| path.starts_with(root)) {
            Some("unindexed_project_definition")
        } else {
            None
        }
    }

    fn rank(&self, location: &DefinitionLocation) -> u8 {
        match uri_path(&location.uri) {
            Some(path) if self.modules.contains_key(&canonical_key(&path)) => 0,
            Some(path) if self.unmapped_status(&path).is_none() => 2,
            _ => 1,
        }
    }
}

pub(super) fn resolved_definitions(
    reference: &DependencyReferenceFact,
    locations: Vec<DefinitionLocation>,
    index: &DefinitionIndex,
) -> ResolvedDependencyFact {
    // Prefer a known authored origin over the generator. An unmapped expansion
    // is still stronger evidence of uncertainty than an external generator path.
    let multiple = locations.len() > 1;
    let Some(location) = locations
        .into_iter()
        .min_by_key(|location| index.rank(location))
    else {
        return unresolved(reference, "unresolved");
    };
    let Some(path) = uri_path(&location.uri) else {
        return unresolved(reference, "unmapped_generated_definition");
    };
    let target = index.modules.get(&canonical_key(&path));
    let symbol = if multiple && target.is_some() {
        reference_symbol(&reference.raw_path)
    } else {
        identifier_at(&path, location.line, location.character)
            .unwrap_or_else(|| reference_symbol(&reference.raw_path))
    };
    let (status, target_module_id, target_module_key, symbol_identity) = match target {
        Some((module_id, module_key)) => (
            "resolved_local",
            Some(module_id.clone()),
            Some(module_key.clone()),
            Some(format!("{module_id}::{symbol}")),
        ),
        None => (
            index.unmapped_status(&path).unwrap_or("resolved_external"),
            None,
            None,
            None,
        ),
    };
    ResolvedDependencyFact {
        raw_path: reference.raw_path.clone(),
        line: reference.line,
        column: reference.column,
        status: status.to_string(),
        backend: "rust_analyzer".to_string(),
        target_path: Some(normalize_slashes(path)),
        target_module_id,
        target_module_key,
        symbol_identity,
    }
}

#[cfg(test)]
mod tests {
    use super::{DefinitionIndex, resolved_definitions};
    use crate::config::LensConfig;
    use crate::facts::{DependencyReferenceFact, FileFacts};
    use crate::semantic::{DefinitionLocation, ResolutionCounts, file_uri};
    use anyhow::Context;
    use serde_json::json;
    use std::fs;

    #[test]
    fn unindexed_project_and_relocated_build_files_are_not_external() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let project = root.path().join("project");
        let sibling = root.path().join("sibling");
        let target = root.path().join("shared-target");
        let mut config = LensConfig::test_config("project", &project);
        config.cargo_metadata = Some(json!({
            "target_directory": target,
            "packages": [{"manifest_path": sibling.join("Cargo.toml"), "source": null}]
        }));
        let index = DefinitionIndex::new(&config, &[]);
        let reference = DependencyReferenceFact {
            raw_path: "Alias".into(),
            line: 1,
            column: 0,
        };
        let cases = [
            (
                project.join("src/omitted.rs"),
                "unindexed_project_definition",
            ),
            (
                sibling.join("src/omitted.rs"),
                "unindexed_project_definition",
            ),
            (
                target.join("debug/build/package-hash/out/config.rs"),
                "unmapped_generated_definition",
            ),
            (
                root.path().join("project-other/src/lib.rs"),
                "resolved_external",
            ),
        ];
        for (path, status) in cases {
            fs::create_dir_all(path.parent().context("fixture path has a parent")?)?;
            fs::write(&path, "pub const VALUE: u32 = 1;")?;
            let location = DefinitionLocation {
                uri: file_uri(&path),
                line: 0,
                character: 10,
            };
            let resolved = resolved_definitions(&reference, vec![location], &index);
            assert_eq!(resolved.status, status);
            assert_eq!(resolved.target_path.as_deref(), path.to_str());
            assert!(resolved.target_module_id.is_none());
            let mut counts = ResolutionCounts::default();
            counts.record(&resolved);
            assert_eq!(
                counts.unresolved,
                usize::from(status != "resolved_external")
            );
            if status != "resolved_external" {
                assert_eq!(counts.unresolved_by_status[status], 1);
            }
        }
        Ok(())
    }

    #[test]
    fn indexed_origin_wins_but_unknown_build_output_never_uses_external_generator()
    -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let project = root.path().join("project");
        let source = project.join("src/lib.rs");
        let generated = project.join("target/debug/build/pkg/out/config.rs");
        let external = root.path().join("registry/generator.rs");
        for path in [&source, &generated, &external] {
            fs::create_dir_all(path.parent().context("fixture path has a parent")?)?;
            fs::write(path, "include!(\"config.rs\");")?;
        }
        let config = LensConfig::test_config("project", &project);
        let index = DefinitionIndex::new(
            &config,
            &[FileFacts::test_fact(&source.to_string_lossy(), "lib")],
        );
        let reference = DependencyReferenceFact {
            raw_path: "crate::VALUE".into(),
            line: 1,
            column: 0,
        };
        for reversed in [false, true] {
            let mut locations = vec![&external, &generated];
            if reversed {
                locations.reverse();
            }
            let locations = locations
                .into_iter()
                .map(|path| DefinitionLocation {
                    uri: file_uri(path),
                    line: 0,
                    character: 0,
                })
                .collect::<Vec<_>>();
            let result = resolved_definitions(&reference, locations, &index);
            assert_eq!(result.status, "unmapped_generated_definition");
            let locations = [&generated, &source, &external]
                .into_iter()
                .map(|path| DefinitionLocation {
                    uri: file_uri(path),
                    line: 0,
                    character: 0,
                })
                .collect();
            let result = resolved_definitions(&reference, locations, &index);
            assert_eq!(result.status, "resolved_local");
            assert_eq!(
                result.symbol_identity.as_deref(),
                Some("test::shared::lib::VALUE")
            );
        }
        Ok(())
    }
}
