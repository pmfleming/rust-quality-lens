use crate::MeasureTool;
use crate::config::LensConfig;
use crate::util::{dedupe, iter_rust_files, resolve_project_path, tail};
use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::env;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;
use tempfile::NamedTempFile;

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct FileFacts {
    pub(crate) path: String,
    pub(crate) module_key: String,
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
        let needs_correctness_facts = tools
            .iter()
            .any(|tool| matches!(tool, MeasureTool::Correctness | MeasureTool::CorrectnessRun));
        let source_facts = if needs_source_facts {
            rust_facts_for_paths(config, &config.source_roots)?
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
                rust_facts_for_paths(config, &correctness_paths)?
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

fn rust_facts_for_paths(config: &LensConfig, paths: &[String]) -> Result<Vec<FileFacts>> {
    let files: Vec<String> = iter_rust_files(paths)
        .into_iter()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .collect();
    if files.is_empty() {
        return Ok(Vec::new());
    }
    let mut temp = NamedTempFile::new()?;
    for file in &files {
        writeln!(temp, "{file}")?;
    }
    let output = Command::new("cargo")
        .arg("run")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(&config.helper_manifest)
        .arg("--bin")
        .arg("rust_facts")
        .arg("--")
        .arg(temp.path())
        .current_dir(&config.project_root)
        .env(
            "RQLENS_SOURCE_ROOTS",
            env::join_paths(config.source_roots.iter().map(PathBuf::from))?,
        )
        .output()
        .with_context(|| {
            format!(
                "running Rust fact extractor at {}",
                config.helper_manifest.display()
            )
        })?;
    if !output.status.success() {
        return Err(anyhow!(
            "Rust fact extraction failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

pub(crate) fn ast_clone_facts_for_paths(
    config: &LensConfig,
    paths: &[String],
) -> Result<Vec<AstCloneFact>> {
    let files: Vec<String> = iter_rust_files(paths)
        .into_iter()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .collect();
    if files.is_empty() {
        return Ok(Vec::new());
    }
    let mut temp = NamedTempFile::new()?;
    for file in &files {
        writeln!(temp, "{file}")?;
    }
    let output = Command::new("cargo")
        .arg("run")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(&config.helper_manifest)
        .arg("--bin")
        .arg("ast_hasher")
        .arg("--")
        .arg(temp.path())
        .current_dir(&config.project_root)
        .output()
        .with_context(|| {
            format!(
                "running AST clone extractor at {}",
                config.helper_manifest.display()
            )
        })?;
    if !output.status.success() {
        return Err(anyhow!(
            "AST clone extraction failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

#[derive(Clone)]
pub(crate) struct ModuleInfo {
    pub(crate) key: String,
    pub(crate) path: PathBuf,
    pub(crate) public_api_count: usize,
    pub(crate) source_nonblank_line_count: usize,
}

pub(crate) struct ModuleGraph {
    pub(crate) facts: Vec<FileFacts>,
    pub(crate) modules: BTreeMap<String, ModuleInfo>,
    pub(crate) dependencies: HashMap<String, HashSet<String>>,
    pub(crate) reverse_dependencies: HashMap<String, HashSet<String>>,
}

pub(crate) fn module_graph(facts: &[FileFacts]) -> ModuleGraph {
    let mut modules = BTreeMap::new();
    for fact in facts {
        if fact.parse_status == "ok" {
            modules.insert(
                fact.module_key.clone(),
                ModuleInfo {
                    key: fact.module_key.clone(),
                    path: PathBuf::from(&fact.path),
                    public_api_count: fact.graph.public_api_count,
                    source_nonblank_line_count: fact.source.source_nonblank_line_count,
                },
            );
        }
        for module_file in &fact.graph.module_files {
            modules
                .entry(module_file.module_key.clone())
                .or_insert(ModuleInfo {
                    key: module_file.module_key.clone(),
                    path: PathBuf::from(&module_file.path),
                    public_api_count: 0,
                    source_nonblank_line_count: 0,
                });
        }
    }
    let keys: Vec<String> = modules.keys().cloned().collect();
    let mut dependencies: HashMap<String, HashSet<String>> = HashMap::new();
    for fact in facts {
        let entry = dependencies.entry(fact.module_key.clone()).or_default();
        for child in &fact.graph.child_modules {
            if modules.contains_key(child) && child != &fact.module_key {
                entry.insert(child.clone());
            }
        }
        for raw in &fact.graph.dependencies {
            if let Some(target) = resolve_dependency(raw, &fact.module_key, &keys) {
                if target != fact.module_key {
                    entry.insert(target);
                }
            }
        }
    }
    let mut reverse_dependencies: HashMap<String, HashSet<String>> = HashMap::new();
    for (source, targets) in &dependencies {
        for target in targets {
            reverse_dependencies
                .entry(target.clone())
                .or_default()
                .insert(source.clone());
        }
    }
    ModuleGraph {
        facts: facts.to_vec(),
        modules,
        dependencies,
        reverse_dependencies,
    }
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
    let mut relative_base: Option<String> = None;
    if let Some(stripped) = raw.strip_prefix("crate::") {
        raw = stripped.to_string();
    } else if let Some(stripped) = raw.strip_prefix("self::") {
        raw = format!("{source_module}::{stripped}");
    } else if raw.starts_with("super::") {
        let mut base: Vec<&str> = source_module.split("::").collect();
        while raw.starts_with("super::") {
            base.pop();
            raw = raw.trim_start_matches("super::").to_string();
        }
        raw = base
            .into_iter()
            .chain(std::iter::once(raw.as_str()))
            .collect::<Vec<_>>()
            .join("::");
        relative_base = Some(source_module.to_string());
    }
    let parts: Vec<&str> = raw.split("::").filter(|part| !part.is_empty()).collect();
    for end in (1..=parts.len()).rev() {
        let candidate = parts[..end].join("::");
        if relative_base
            .as_ref()
            .is_some_and(|base| base == &candidate || base.starts_with(&format!("{candidate}::")))
        {
            continue;
        }
        if modules.iter().any(|module| module == &candidate) {
            return Some(candidate);
        }
    }
    None
}

pub(crate) fn correctness_paths(config: &LensConfig) -> Vec<String> {
    let mut paths = config.source_roots.clone();
    for extra in ["tests", "benches", "examples"] {
        let path = config.project_root.join(extra);
        paths.extend(top_level_rust_files(&path));
    }
    let cargo = config.project_root.join("Cargo.toml");
    if let Ok(text) = std::fs::read_to_string(cargo) {
        if let Ok(value) = text.parse::<toml::Value>() {
            for target in ["bin", "test", "bench", "example"] {
                for item in value
                    .get(target)
                    .and_then(toml::Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if let Some(path) = item.get("path").and_then(toml::Value::as_str) {
                        paths.push(
                            resolve_project_path(PathBuf::from(path), &config.project_root)
                                .to_string_lossy()
                                .to_string(),
                        );
                    }
                }
            }
        }
    }
    dedupe(paths)
}

fn top_level_rust_files(path: &std::path::Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "rs"))
        .map(|path| path.to_string_lossy().to_string())
        .collect()
}

#[derive(Clone)]
pub(crate) struct TestStatus {
    pub(crate) status: String,
    pub(crate) duration: Option<f64>,
    pub(crate) stdout_tail: Option<String>,
    pub(crate) stderr_tail: Option<String>,
}

pub(crate) fn run_tests(config: &LensConfig) -> Result<HashMap<String, TestStatus>> {
    let started = Instant::now();
    let output = Command::new("cargo")
        .arg("test")
        .current_dir(&config.project_root)
        .output()?;
    let duration = started.elapsed().as_secs_f64();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut statuses = parse_cargo_test_statuses(&stdout);
    statuses.insert(
        "__run__".to_string(),
        TestStatus {
            status: if output.status.success() {
                "passed"
            } else {
                "failed"
            }
            .to_string(),
            duration: Some(duration),
            stdout_tail: Some(tail(&stdout, 40)),
            stderr_tail: Some(tail(&stderr, 40)),
        },
    );
    Ok(statuses)
}

fn parse_cargo_test_statuses(stdout: &str) -> HashMap<String, TestStatus> {
    let running_re =
        regex::Regex::new(r#"\s*Running\s+(?:(?:unittests|tests?)\s+)?(.+?\.rs)(?:\s+\(|$)"#)
            .unwrap();
    let status_re = regex::Regex::new(r"test\s+(.+?)\s+\.\.\.\s+(ok|FAILED|ignored)").unwrap();
    let mut current_path: Option<String> = None;
    let mut statuses = HashMap::new();
    for line in stdout.lines() {
        if let Some(cap) = running_re.captures(line) {
            current_path = Some(cap[1].trim_matches('"').replace('\\', "/"));
            continue;
        }
        if let Some(cap) = status_re.captures(line.trim()) {
            let name = cap[1].to_string();
            let status = match &cap[2] {
                "ok" => "passed",
                "FAILED" => "failed",
                "ignored" => "skipped",
                _ => "unknown",
            }
            .to_string();
            let record = TestStatus {
                status,
                duration: None,
                stdout_tail: None,
                stderr_tail: None,
            };
            statuses.insert(name.clone(), record.clone());
            if let Some(path) = &current_path {
                statuses.insert(format!("{path}::{name}"), record);
            }
        }
    }
    statuses
}

#[cfg(test)]
mod tests {
    use super::{correctness_paths, resolve_dependency};
    use crate::config::LensConfig;
    use crate::util::normalize_slashes;
    use std::path::PathBuf;

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
    fn cargo_target_paths_are_read_from_toml_structure() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("src")).unwrap();
        std::fs::create_dir_all(root.path().join("tests/fixtures")).unwrap();
        std::fs::write(root.path().join("src/lib.rs"), "").unwrap();
        std::fs::write(root.path().join("tests/smoke.rs"), "").unwrap();
        std::fs::write(root.path().join("tests/fixtures/not_a_target.rs"), "").unwrap();
        std::fs::write(
            root.path().join("Cargo.toml"),
            r#"
[package]
name = "target-paths"
version = "0.1.0"

[package.metadata]
path = "not-a-rust-target.rs"

[[bin]]
name = "tool"
path = "src/bin/tool.rs"

[[test]]
name = "integration"
path = "tests/integration.rs"
"#,
        )
        .unwrap();
        let config = LensConfig {
            project_name: "target-paths".to_string(),
            project_root: root.path().to_path_buf(),
            source_roots: vec![root.path().join("src").to_string_lossy().to_string()],
            output_dir: root.path().join("target/analysis"),
            helper_manifest: PathBuf::from("unused"),
        };
        let paths = correctness_paths(&config);
        assert!(
            paths
                .iter()
                .any(|path| normalize_slashes(path).ends_with("src/bin/tool.rs"))
        );
        assert!(
            paths
                .iter()
                .any(|path| normalize_slashes(path).ends_with("tests/integration.rs"))
        );
        assert!(
            paths
                .iter()
                .any(|path| normalize_slashes(path).ends_with("tests/smoke.rs"))
        );
        assert!(
            !paths
                .iter()
                .any(|path| normalize_slashes(path).contains("tests/fixtures"))
        );
        assert!(
            !paths
                .iter()
                .any(|path| normalize_slashes(path).contains("not-a-rust-target.rs"))
        );
    }
}
