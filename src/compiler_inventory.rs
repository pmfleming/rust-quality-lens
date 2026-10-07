//! Compiler-analyzed declarations, kept separate from authored syntax metrics.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::PathBuf};

use crate::config::LensConfig;
pub(crate) mod architecture;
pub(crate) mod architecture_graph;
mod architecture_schema;
mod capture;
mod projection;
mod schema;
pub(crate) use projection::validate_type_projection;
pub(crate) use schema::{context_schema, origin_schema, profiles_schema};
#[cfg(test)]
mod architecture_graph_tests;
#[cfg(test)]
mod tests;

pub(crate) const COMPILER_VERSION: &str = "rustc 1.95.0-nightly (a423f68a0 2026-02-13)";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Location {
    pub(crate) path: PathBuf,
    pub(crate) line: usize,
    pub(crate) column: usize,
    end_line: usize,
    end_column: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Origin {
    generated: bool,
    location_kind: String,
    location: Option<Location>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct TypeRecord {
    id: String,
    qualified_name: String,
    name: String,
    kind: String,
    field_count: usize,
    variant_count: usize,
    variant_field_count: usize,
    origin: Origin,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ImplRecord {
    id: String,
    owner_id: Option<String>,
    owner_display: String,
    owner_kind: String,
    method_ids: Vec<String>,
    origin: Origin,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct FunctionRecord {
    id: String,
    qualified_name: String,
    origin: Origin,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Profile {
    schema_version: u64,
    backend: String,
    compiler_version: String,
    package_name: String,
    manifest_dir: PathBuf,
    source_root: PathBuf,
    crate_name: String,
    target_triple: String,
    test: bool,
    cfg: Vec<String>,
    types: Vec<TypeRecord>,
    impls: Vec<ImplRecord>,
    functions: Vec<FunctionRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    architecture: Option<architecture::Capture>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CompilerContext {
    manifest_dir: PathBuf,
    source_root: PathBuf,
    target_triple: String,
    test: bool,
    cfg: Vec<String>,
    compiler_version: String,
}

impl CompilerContext {
    fn id(&self) -> String {
        crate::util::stable_hash(&json!(self).to_string())
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            self.source_root.is_absolute()
                && self.manifest_dir.is_absolute()
                && !self.target_triple.is_empty()
                && self.compiler_version == COMPILER_VERSION,
            "missing or unsupported compiler context identity"
        );
        ensure!(
            !self.cfg.is_empty()
                && self.cfg.windows(2).all(|pair| pair[0] < pair[1])
                && self.test == self.cfg.iter().any(|cfg| cfg == "test"),
            "invalid effective cfg evidence"
        );
        Ok(())
    }
}

impl Profile {
    fn context(&self) -> CompilerContext {
        CompilerContext {
            manifest_dir: self.manifest_dir.clone(),
            source_root: self.source_root.clone(),
            target_triple: self.target_triple.clone(),
            test: self.test,
            cfg: self.cfg.clone(),
            compiler_version: self.compiler_version.clone(),
        }
    }

    fn context_id(&self) -> String {
        self.context().id()
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1
                && self.backend == "rustc-hir"
                && self.compiler_version == COMPILER_VERSION,
            "unsupported compiler inventory producer"
        );
        self.context().validate()?;
        if let Some(architecture) = &self.architecture {
            architecture.validate()?;
        }
        ensure!(
            !self.package_name.is_empty() && !self.crate_name.is_empty(),
            "missing compiler package/crate identity"
        );
        let mut definitions = BTreeSet::new();
        let mut types = BTreeSet::new();
        let mut functions = BTreeSet::new();
        for ty in &self.types {
            definition(&mut definitions, &ty.id, &ty.origin)?;
            ensure!(
                matches!(ty.kind.as_str(), "struct" | "enum" | "union")
                    && !ty.name.is_empty()
                    && !ty.qualified_name.is_empty(),
                "invalid compiler type"
            );
            types.insert(&ty.id);
        }
        for function in &self.functions {
            definition(&mut definitions, &function.id, &function.origin)?;
            functions.insert(&function.id);
        }
        let mut assigned_methods = BTreeSet::new();
        for imp in &self.impls {
            definition(&mut definitions, &imp.id, &imp.origin)?;
            ensure!(
                match imp.owner_id.as_ref() {
                    Some(id) => imp.owner_kind == "local_adt" && types.contains(id),
                    None => imp.owner_kind == "external_or_non_nominal",
                },
                "compiler impl references an unknown or contradictory owner"
            );
            for method in &imp.method_ids {
                ensure!(
                    functions.contains(method) && assigned_methods.insert(method),
                    "missing or multiply assigned compiler method"
                );
            }
        }
        Ok(())
    }
}

fn definition<'a>(ids: &mut BTreeSet<&'a str>, id: &'a str, origin: &Origin) -> Result<()> {
    ensure!(
        !id.is_empty() && ids.insert(id),
        "duplicate or empty compiler definition identity"
    );
    if let Some(location) = &origin.location {
        ensure!(
            location.path.is_absolute()
                && location.line > 0
                && location.column > 0
                && location.end_column > 0
                && (location.end_line, location.end_column) >= (location.line, location.column),
            "invalid compiler origin"
        );
        ensure!(
            origin.location_kind
                == if origin.generated {
                    "invocation_anchor"
                } else {
                    "authored"
                },
            "generated coordinates cannot masquerade as authored source"
        );
    } else {
        ensure!(
            origin.location_kind == "unavailable",
            "missing source origin must be explicit"
        );
    }
    Ok(())
}

#[derive(Default)]
pub(crate) struct Inventory {
    profiles: Vec<Profile>,
    expected_contexts: BTreeSet<(PathBuf, PathBuf, bool)>,
    reasons: Vec<String>,
    requested_scope: Value,
}

impl Inventory {
    pub(crate) fn collect(config: &LensConfig) -> Self {
        let requested_scope = json!({
            "workspace": config.verification.workspace, "all_targets": config.verification.all_targets,
            "all_features": config.verification.all_features, "features": config.verification.features,
            "no_default_features": config.verification.no_default_features, "exclude": config.verification.exclude,
            "target": config.compiler_inventory_target,
        });
        match capture::collect(config) {
            Ok((profiles, expected_contexts)) => Self {
                profiles,
                expected_contexts,
                requested_scope,
                reasons: Vec::new(),
            },
            Err(error) => Self {
                profiles: Vec::new(),
                expected_contexts: BTreeSet::new(),
                requested_scope,
                reasons: vec![format!("{error:#}")],
            },
        }
    }

    pub(crate) fn confidence(&self, fingerprint_complete: bool) -> Value {
        let mut missing = self.reasons.clone();
        if !fingerprint_complete {
            missing.push("compiler inventory input fingerprint is incomplete".into());
        }
        if self.profiles.iter().any(|profile| {
            profile.types.iter().any(|ty| ty.origin.location.is_none())
                || profile
                    .impls
                    .iter()
                    .any(|imp| imp.owner_id.is_some() && imp.origin.location.is_none())
        }) {
            missing.push(
                "compiler type or local impl has no attributable source/invocation anchor".into(),
            );
        }
        let complete = missing.is_empty() && !self.profiles.is_empty();
        json!({
            "complete": complete, "partial": !complete,
            "confidence_scope": "compiler_type_inventory",
            "required_inputs": ["successful_compilation", "complete_selected_target_contexts", "compiler_resolved_owners", "input_fingerprint"],
            "observed_inputs": {"compilation_contexts": self.profiles.len()},
            "missing_input": missing, "stale_input": [], "unsupported_pattern": [],
            "count_scope": "Local ADTs and locally defined impl methods in each selected compiled context; not inherited methods or dependency implementations. Source-only rule coverage is separate.",
        })
    }

    pub(crate) fn architecture_graphs(&self) -> Vec<architecture_graph::Graph> {
        architecture_graph::project(&self.profiles)
    }

    pub(crate) fn report(&self, fingerprint_complete: bool) -> Value {
        let confidence = self.confidence(fingerprint_complete);
        json!({
            "backend": "rustc-hir", "inventory_model_version": 1,
            "requested_scope": self.requested_scope,
            "summary": {"complete": confidence["complete"], "context_count": self.profiles.len(),
                "type_count_across_contexts": self.profiles.iter().map(|p| p.types.len()).sum::<usize>(),
                "architecture_contexts_with_complete_capture": self.profiles.iter().filter(|p| p.architecture.as_ref().is_some_and(|a| a.missing_evidence().is_empty())).count()},
            "profiles": self.profiles, "expected_contexts": self.expected_contexts,
            "architecture_graphs": self.architecture_graphs(),
            "measurement_confidence": confidence,
            "scope": "Compiler-analyzed declarations after cfg selection and all macro expansion. Per-context identities and invocation anchors; no merged cross-profile totals or source-calibrated pressure scores.",
        })
    }

    pub(crate) fn has_profiles(&self) -> bool {
        !self.profiles.is_empty()
    }
}

pub(crate) fn validate_report(value: &Value) -> Result<()> {
    ensure!(
        value["backend"] == "rustc-hir" && value["inventory_model_version"] == 1,
        "invalid compiler inventory contract"
    );
    let profiles: Vec<Profile> = serde_json::from_value(value["profiles"].clone())?;
    let expected_rows: Vec<(PathBuf, PathBuf, bool)> =
        serde_json::from_value(value["expected_contexts"].clone())?;
    let expected = expected_rows.iter().cloned().collect::<BTreeSet<_>>();
    ensure!(
        expected.len() == expected_rows.len(),
        "duplicate expected compilation context"
    );
    if !profiles.is_empty() || !expected.is_empty() {
        capture::validate_coverage(&profiles, &expected)?;
    }
    let mut contexts = BTreeSet::new();
    for profile in &profiles {
        profile.validate()?;
        ensure!(
            contexts.insert(profile.context_id()),
            "duplicate compiler context"
        );
    }
    if let Some(graphs) = value.get("architecture_graphs") {
        ensure!(
            *graphs == serde_json::to_value(architecture_graph::project(&profiles))?,
            "compiler architecture projection differs from capture"
        );
    }
    let confidence: crate::contracts::MeasurementConfidence =
        serde_json::from_value(value["measurement_confidence"].clone())?;
    if confidence.is_complete() {
        ensure!(
            profiles.iter().all(|profile| profile
                .types
                .iter()
                .all(|ty| ty.origin.location.is_some())
                && profile
                    .impls
                    .iter()
                    .all(|imp| imp.owner_id.is_none() || imp.origin.location.is_some())),
            "complete compiler type inventory requires attributable origins"
        );
    }
    ensure!(
        value["summary"]["type_count_across_contexts"]
            == profiles
                .iter()
                .map(|profile| profile.types.len())
                .sum::<usize>(),
        "compiler type count mismatch"
    );
    if let Some(count) = value["summary"].get("architecture_contexts_with_complete_capture") {
        ensure!(
            count
                == profiles
                    .iter()
                    .filter(|p| p
                        .architecture
                        .as_ref()
                        .is_some_and(|a| a.missing_evidence().is_empty()))
                    .count(),
            "compiler architecture capture count mismatch"
        );
    }
    ensure!(
        value["summary"]["context_count"] == profiles.len(),
        "compiler context count mismatch"
    );
    ensure!(
        value["summary"]["complete"] == value["measurement_confidence"]["complete"],
        "contradictory compiler inventory confidence"
    );
    ensure!(
        value["measurement_confidence"]["complete"] != true || !profiles.is_empty(),
        "empty compiler captures are not complete"
    );
    Ok(())
}
