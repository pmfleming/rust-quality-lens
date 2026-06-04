use crate::MeasureTool;
use crate::config::LensConfig;
use anyhow::Result;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

static HELPER_BINARIES: OnceLock<Mutex<HashMap<HelperBinaryKey, PathBuf>>> = OnceLock::new();

use std::collections::HashMap;
use std::path::PathBuf;

mod graph;
mod helpers;
mod test_runner;

pub(crate) use graph::{ModuleGraph, ModuleInfo, module_graph};
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
    #[serde(default = "default_target_kind")]
    pub(crate) target_kind: String,
    #[serde(default)]
    pub(crate) entrypoint_kind: Option<String>,
    #[serde(default)]
    pub(crate) is_entrypoint: bool,
    pub(crate) parse_status: String,
    #[serde(flatten)]
    pub(crate) graph: FileGraphFacts,
    #[serde(flatten)]
    pub(crate) source: SourceMetrics,
    #[serde(flatten)]
    pub(crate) items: FileItems,
    #[serde(flatten)]
    pub(crate) escapes: EscapeFacts,
}

fn default_target_kind() -> String {
    "module".to_string()
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct FileGraphFacts {
    pub(crate) dependencies: Vec<String>,
    pub(crate) child_modules: Vec<String>,
    pub(crate) module_files: Vec<ModuleFileFact>,
    pub(crate) unsupported_patterns: Vec<String>,
    pub(crate) public_api_count: usize,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct SourceMetrics {
    pub(crate) source_line_count: usize,
    pub(crate) source_nonblank_line_count: usize,
    pub(crate) source_comment_line_count: usize,
    pub(crate) function_count: usize,
    pub(crate) cognitive_complexity: usize,
    pub(crate) cyclomatic_complexity: usize,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct FileItems {
    pub(crate) types: Vec<TypeFact>,
    pub(crate) impls: Vec<ImplFact>,
    pub(crate) tests: Vec<TestFact>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct EscapeFacts {
    pub(crate) escape_counts: BTreeMap<String, usize>,
    pub(crate) escape_locations: Vec<LocationFact>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ModuleFileFact {
    pub(crate) module_key: String,
    pub(crate) path: String,
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
}

impl RunContext {
    pub(crate) fn new(config: &LensConfig, tools: &[MeasureTool]) -> Result<Self> {
        let needs_source_facts = tools.iter().any(|tool| {
            matches!(
                tool,
                MeasureTool::Hotspots
                    | MeasureTool::Clones
                    | MeasureTool::EscapeHatches
                    | MeasureTool::TypeHealth
                    | MeasureTool::Locality
                    | MeasureTool::Leverage
                    | MeasureTool::Map
            )
        });
        let needs_correctness_facts = tools.iter().any(|tool| {
            matches!(
                tool,
                MeasureTool::Clones | MeasureTool::Correctness | MeasureTool::CorrectnessRun
            )
        });
        let source_facts = if needs_source_facts {
            helpers::rust_facts_for_paths(config, &config.source_roots)?
        } else {
            Vec::new()
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
        })
    }
}
