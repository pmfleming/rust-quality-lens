use crate::config::LensConfig;
use crate::semantic::{self, IdentityResolutionSummary};
use crate::tool::MeasureTool;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

mod graph;
mod helpers;
mod test_runner;
mod type_identity;
pub(crate) use type_identity::TypeImplIndex;
#[cfg(test)]
pub(crate) use type_identity::impl_identity;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use graph::module_graph;
pub(crate) use graph::{ModuleGraph, ModuleInfo, resolve_dependency};
pub(crate) use helpers::ast_clone_facts_for_files;
pub(crate) use test_runner::{TestStatus, correctness_paths, run_tests};

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct FileFacts {
    pub(crate) path: String,
    pub(crate) module_key: String,
    #[serde(default)]
    pub(crate) module_id: String,
    #[serde(default)]
    pub(crate) package_name: String,
    #[serde(default)]
    pub(crate) target_name: String,
    #[serde(default)]
    pub(crate) identity_backend: String,
    #[serde(default = "default_target_kind")]
    pub(crate) target_kind: String,
    #[serde(default)]
    pub(crate) entrypoint_kind: Option<String>,
    #[serde(default)]
    pub(crate) is_entrypoint: bool,
    pub(crate) parse_status: String,
    #[serde(default)]
    pub(crate) syntax_backend: String,
    #[serde(default)]
    pub(crate) syntax_error_count: usize,
    #[serde(default = "default_true")]
    pub(crate) source_metrics_available: bool,
    #[serde(flatten)]
    pub(crate) graph: FileGraphFacts,
    #[serde(flatten)]
    pub(crate) source: SourceMetrics,
    #[serde(flatten)]
    pub(crate) items: FileItems,
    #[serde(flatten)]
    pub(crate) escapes: EscapeFacts,
}

#[cfg(test)]
impl FileFacts {
    pub(crate) fn test_fact(path: &str, module_key: &str) -> Self {
        Self {
            path: path.to_string(),
            module_key: module_key.to_string(),
            module_id: format!("test::shared::{module_key}"),
            package_name: "test".to_string(),
            target_name: "shared".to_string(),
            identity_backend: "test".to_string(),
            target_kind: "module".to_string(),
            entrypoint_kind: None,
            is_entrypoint: false,
            parse_status: "ok".to_string(),
            syntax_backend: "syn".to_string(),
            syntax_error_count: 0,
            source_metrics_available: true,
            graph: FileGraphFacts::default(),
            source: SourceMetrics::default(),
            items: FileItems::default(),
            escapes: EscapeFacts::default(),
        }
    }
}

fn default_target_kind() -> String {
    "module".to_string()
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct FileGraphFacts {
    pub(crate) dependencies: Vec<String>,
    #[serde(default)]
    pub(crate) dependency_references: Vec<DependencyReferenceFact>,
    #[serde(default)]
    pub(crate) resolved_dependencies: Vec<ResolvedDependencyFact>,
    pub(crate) child_modules: Vec<String>,
    #[serde(default)]
    pub(crate) module_files: Vec<ModuleFileFact>,
    pub(crate) unsupported_patterns: Vec<String>,
    pub(crate) public_api_count: usize,
    #[serde(default)]
    pub(crate) documented_public_api_count: usize,
    #[serde(default)]
    pub(crate) has_crate_docs: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ModuleFileFact {
    pub(crate) module_key: String,
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) test_code: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct DependencyReferenceFact {
    pub(crate) raw_path: String,
    pub(crate) line: usize,
    pub(crate) column: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ResolvedDependencyFact {
    pub(crate) raw_path: String,
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) status: String,
    pub(crate) backend: String,
    pub(crate) target_path: Option<String>,
    pub(crate) target_module_id: Option<String>,
    pub(crate) target_module_key: Option<String>,
    pub(crate) symbol_identity: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct SourceMetrics {
    pub(crate) source_line_count: usize,
    pub(crate) source_nonblank_line_count: usize,
    pub(crate) source_comment_line_count: usize,
    pub(crate) function_count: usize,
    #[serde(default)]
    pub(crate) halstead: Option<HalsteadMetrics>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct HalsteadMetrics {
    pub(crate) distinct_operators: usize,
    pub(crate) total_operators: usize,
    pub(crate) distinct_operands: usize,
    pub(crate) total_operands: usize,
    pub(crate) vocabulary: usize,
    pub(crate) length: usize,
    pub(crate) volume: f64,
    pub(crate) difficulty: Option<f64>,
    pub(crate) effort: Option<f64>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct FileItems {
    pub(crate) type_identity_limitations: Vec<String>,
    #[serde(default)]
    pub(crate) macro_invocations: Option<Vec<MacroInvocation>>,
    pub(crate) types: Vec<TypeFact>,
    pub(crate) impls: Vec<ImplFact>,
    pub(crate) tests: Vec<TestFact>,
    #[serde(default)]
    pub(crate) functions: Vec<FunctionFact>,
    #[serde(default)]
    pub(crate) quality_findings: Vec<QualityFindingFact>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum MacroContext {
    Item,
    Impl,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct MacroInvocation {
    pub(crate) kind: MacroContext,
    pub(crate) macro_path: String,
    pub(crate) module_key: String,
    pub(crate) line: usize,
    pub(crate) column_utf16: usize,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct FunctionFact {
    pub(crate) name: String,
    pub(crate) qualified_name: String,
    pub(crate) module_key: String,
    pub(crate) path: String,
    pub(crate) start_line: usize,
    pub(crate) end_line: usize,
    pub(crate) source_line_count: usize,
    pub(crate) branch_pressure: usize,
    pub(crate) path_pressure: usize,
    pub(crate) max_nesting_depth: usize,
    #[serde(default)]
    pub(crate) cyclomatic_complexity: usize,
    #[serde(default)]
    pub(crate) cognitive_complexity: usize,
    #[serde(default)]
    pub(crate) halstead: Option<HalsteadMetrics>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct EscapeFacts {
    pub(crate) escape_counts: BTreeMap<String, usize>,
    pub(crate) escape_locations: Vec<LocationFact>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct TypeFact {
    pub(crate) type_name: String,
    pub(crate) qualified_name: String,
    pub(crate) module_key: String,
    pub(crate) path: String,
    pub(crate) line: usize,
    pub(crate) kind: String,
    pub(crate) shape: String,
    pub(crate) field_count: usize,
    pub(crate) variant_count: usize,
    pub(crate) variant_field_count: usize,
    pub(crate) declaration_span: usize,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ImplFact {
    pub(crate) type_name: String,
    #[serde(default)]
    pub(crate) qualified_type_name: String,
    pub(crate) module_key: String,
    pub(crate) path: String,
    pub(crate) method_count: usize,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct TestFact {
    pub(crate) name: String,
    pub(crate) qualified_name: String,
    pub(crate) path: String,
    pub(crate) line: usize,
    #[serde(default)]
    pub(crate) module_key: String,
    #[serde(default)]
    pub(crate) assertion_count: usize,
    #[serde(default)]
    pub(crate) sut_call_count: usize,
    #[serde(default)]
    pub(crate) ignored: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct LocationFact {
    pub(crate) kind: String,
    pub(crate) line: usize,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct QualityFindingFact {
    pub(crate) rule_id: String,
    pub(crate) kind: String,
    pub(crate) line: usize,
    pub(crate) message: String,
    pub(crate) test_code: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct AstCloneFact {
    pub(crate) name: String,
    pub(crate) qualified_name: String,
    pub(crate) file: String,
    pub(crate) start_line: usize,
    pub(crate) end_line: usize,
    pub(crate) node_count: usize,
    pub(crate) ast_hash: String,
}

pub(crate) struct RunContext {
    pub(crate) snapshot: crate::measurement::InputSnapshot,
    pub(crate) source_files: Vec<PathBuf>,
    pub(crate) correctness_files: Vec<PathBuf>,
    graph: OnceLock<ModuleGraph>,
    type_impls: OnceLock<TypeImplIndex>,
    pub(crate) source_facts: Arc<[FileFacts]>,
    pub(crate) correctness_facts: Arc<[FileFacts]>,
    pub(crate) correctness_paths: Vec<String>,
    pub(crate) identity_resolution: IdentityResolutionSummary,
    pub(crate) compiler_inventory: Option<Arc<crate::compiler_inventory::Inventory>>,
    pub(crate) compiler_architecture: Option<Arc<crate::compiler_inventory::Inventory>>,
}

impl RunContext {
    pub(crate) fn type_impl_index(&self) -> &TypeImplIndex {
        self.type_impls
            .get_or_init(|| TypeImplIndex::new(&self.source_facts))
    }

    pub(crate) fn type_confidence(&self) -> serde_json::Value {
        if let Some(inventory) = &self.compiler_inventory {
            return inventory.confidence(self.snapshot.fingerprint["complete"] == true);
        }
        self.type_impl_index().confidence(
            self.source_confidence(),
            self.snapshot.fingerprint["complete"] == true,
        )
    }

    pub(crate) fn semantic_confidence(
        &self,
        source: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let mut confidence: crate::contracts::MeasurementConfidence =
            serde_json::from_value(source)?;
        self.identity_resolution.apply_confidence(&mut confidence);
        Ok(serde_json::to_value(confidence)?)
    }

    pub(crate) fn module_graph(&self) -> &ModuleGraph {
        self.graph
            .get_or_init(|| graph::module_graph(&self.source_facts))
    }

    pub(crate) fn source_confidence(&self) -> serde_json::Value {
        crate::measurement::source_confidence_from_inventory(&self.source_files, &self.source_facts)
    }

    pub(crate) fn correctness_confidence(&self) -> serde_json::Value {
        crate::measurement::source_confidence_from_inventory(
            &self.correctness_files,
            &self.correctness_facts,
        )
    }

    pub(crate) fn new(config: &LensConfig, tools: &[MeasureTool]) -> Result<Self> {
        // Configuration loading prepares Cargo metadata (and Cargo.lock) before
        // snapshots. Reuse the target identities and inventories for both scans.
        let targets = helpers::cargo_targets(config);
        let snapshot = crate::measurement::InputSnapshot::capture(config);
        let source_files = crate::util::iter_rust_files(&config.source_roots);
        let needs_source_facts = tools
            .iter()
            .any(|tool| tool.requires_source_facts() && !config.compiler_architecture_for(tool));
        let needs_correctness_facts = tools.iter().any(MeasureTool::requires_correctness_facts);
        let mut source_facts = if needs_source_facts {
            helpers::rust_facts_for_files(config, &source_files, &targets)?
        } else {
            Vec::new()
        };
        let identity_resolution = resolve_identities(config, tools, &mut source_facts)?;
        let source_facts: Arc<[FileFacts]> = source_facts.into();
        let correctness_paths = if needs_correctness_facts {
            correctness_paths(config, &source_facts)
        } else {
            Vec::new()
        };
        let correctness_files = crate::util::iter_rust_files(&correctness_paths);
        let correctness_facts = if !needs_correctness_facts {
            Arc::default()
        } else if correctness_files == source_files && needs_source_facts {
            Arc::clone(&source_facts)
        } else {
            helpers::rust_facts_for_files(config, &correctness_files, &targets)?.into()
        };
        let explicit_inventory = tools.contains(&MeasureTool::CompilerInventory);
        let compiler_requested = explicit_inventory
            || (config.compiler_inventory_driver.is_some()
                && tools.contains(&MeasureTool::TypeHealth));
        let architecture_requested = tools
            .iter()
            .any(|tool| config.compiler_architecture_for(tool));
        let capture_architecture = architecture_requested || explicit_inventory;
        let inventory = (compiler_requested || architecture_requested).then(|| {
            Arc::new(crate::compiler_inventory::Inventory::collect(
                config,
                capture_architecture,
            ))
        });
        let compiler_inventory = inventory.as_ref().filter(|_| compiler_requested).cloned();
        let compiler_architecture = inventory.filter(|_| architecture_requested);
        Ok(Self {
            snapshot,
            source_files,
            correctness_files,
            graph: OnceLock::new(),
            type_impls: OnceLock::new(),
            source_facts,
            correctness_facts,
            correctness_paths,
            identity_resolution,
            compiler_inventory,
            compiler_architecture,
        })
    }
}

fn resolve_identities(
    config: &LensConfig,
    tools: &[MeasureTool],
    source_facts: &mut [FileFacts],
) -> Result<IdentityResolutionSummary> {
    if tools
        .iter()
        .any(|tool| tool.requires_semantic_identity() && !config.compiler_architecture_for(tool))
    {
        return semantic::resolve(config, source_facts);
    }
    let reference_count = source_facts
        .iter()
        .map(|fact| fact.graph.dependency_references.len())
        .sum();
    Ok(IdentityResolutionSummary::disabled(
        config.identity_resolution,
        reference_count,
    ))
}
