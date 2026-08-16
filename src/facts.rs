use crate::config::LensConfig;
use crate::semantic::{self, IdentityResolutionSummary};
use crate::tool::MeasureTool;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

static HELPER_BINARIES: OnceLock<Mutex<HashMap<HelperBinaryKey, PathBuf>>> = OnceLock::new();

use std::collections::HashMap;
use std::path::PathBuf;

mod graph;
mod helpers;
mod test_runner;

pub(crate) use graph::{ModuleGraph, ModuleInfo, module_graph, resolve_dependency};
pub(crate) use helpers::ast_clone_facts_for_paths;
pub(crate) use test_runner::{TestStatus, correctness_paths, run_tests};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct HelperBinaryKey {
    pub(super) manifest: PathBuf,
    pub(super) bin: String,
}

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
    pub(crate) unsupported_patterns: Vec<String>,
    pub(crate) public_api_count: usize,
    #[serde(default)]
    pub(crate) documented_public_api_count: usize,
    #[serde(default)]
    pub(crate) has_crate_docs: bool,
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
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct FileItems {
    pub(crate) types: Vec<TypeFact>,
    pub(crate) impls: Vec<ImplFact>,
    pub(crate) tests: Vec<TestFact>,
    #[serde(default)]
    pub(crate) functions: Vec<FunctionFact>,
    #[serde(default)]
    pub(crate) quality_findings: Vec<QualityFindingFact>,
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
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct EscapeFacts {
    pub(crate) escape_counts: BTreeMap<String, usize>,
    pub(crate) escape_locations: Vec<LocationFact>,
}

#[derive(Clone, Debug, Deserialize)]
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
    pub(crate) source_facts: Vec<FileFacts>,
    pub(crate) correctness_facts: Vec<FileFacts>,
    pub(crate) correctness_paths: Vec<String>,
    pub(crate) identity_resolution: IdentityResolutionSummary,
}

impl RunContext {
    pub(crate) fn new(config: &LensConfig, tools: &[MeasureTool]) -> Result<Self> {
        let needs_source_facts = tools.iter().any(|tool| {
            matches!(
                tool,
                MeasureTool::Hotspots
                    | MeasureTool::Clones
                    | MeasureTool::EscapeHatches
                    | MeasureTool::Reliability
                    | MeasureTool::ApiHealth
                    | MeasureTool::TypeHealth
                    | MeasureTool::Correctness
                    | MeasureTool::CorrectnessRun
                    | MeasureTool::Locality
                    | MeasureTool::Leverage
                    | MeasureTool::Map
                    | MeasureTool::Coverage
            )
        });
        let needs_correctness_facts = tools.iter().any(|tool| {
            matches!(
                tool,
                MeasureTool::Clones | MeasureTool::Correctness | MeasureTool::CorrectnessRun
            )
        });
        let mut source_facts = if needs_source_facts {
            helpers::rust_facts_for_paths(config, &config.source_roots)?
        } else {
            Vec::new()
        };
        let needs_semantic_identity = tools.iter().any(|tool| {
            matches!(
                tool,
                MeasureTool::Locality | MeasureTool::Leverage | MeasureTool::Map
            )
        });
        let reference_count = source_facts
            .iter()
            .map(|fact| fact.graph.dependency_references.len())
            .sum();
        let identity_resolution = if needs_semantic_identity {
            semantic::resolve(config, &mut source_facts)?
        } else {
            IdentityResolutionSummary::disabled(config.identity_resolution, reference_count)
        };
        let correctness_paths = if needs_correctness_facts {
            correctness_paths(config)
        } else {
            Vec::new()
        };
        let correctness_facts = if needs_correctness_facts {
            if correctness_paths == config.source_roots && needs_source_facts {
                source_facts.clone()
            } else {
                helpers::rust_facts_for_paths(config, &correctness_paths)?
            }
        } else {
            Vec::new()
        };
        Ok(Self {
            source_facts,
            correctness_facts,
            correctness_paths,
            identity_resolution,
        })
    }
}
