use crate::util::{absolutize, bundled_helper_manifest, resolve_config_path, resolve_project_path};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) const CONFIG_FILE_NAME: &str = "rqlens.toml";

#[derive(Debug, Deserialize)]
struct RawConfig {
    project_name: Option<String>,
    project_root: Option<PathBuf>,
    source_roots: Option<Vec<String>>,
    output_dir: Option<PathBuf>,
    rust: Option<RawRustConfig>,
    verification: Option<RawVerificationConfig>,
}

#[derive(Debug, Deserialize)]
struct RawRustConfig {
    helper_manifest: Option<PathBuf>,
    identity_resolution: Option<SemanticIdentityMode>,
    rust_analyzer: Option<PathBuf>,
    identity_timeout_seconds: Option<u64>,
    identity_offline: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
struct RawVerificationConfig {
    timeout_seconds: Option<u64>,
    workspace: Option<bool>,
    all_targets: Option<bool>,
    all_features: Option<bool>,
    no_default_features: Option<bool>,
    features: Option<Vec<String>>,
    exclude: Option<Vec<String>>,
    audit: Option<bool>,
    deny: Option<bool>,
}

#[derive(Clone, Debug)]
pub(crate) struct VerificationConfig {
    pub(crate) timeout_seconds: u64,
    pub(crate) workspace: bool,
    pub(crate) all_targets: bool,
    pub(crate) all_features: bool,
    pub(crate) no_default_features: bool,
    pub(crate) features: Vec<String>,
    pub(crate) exclude: Vec<String>,
    pub(crate) audit: bool,
    pub(crate) deny: bool,
}

impl Default for VerificationConfig {
    fn default() -> Self {
        Self {
            timeout_seconds: 600,
            workspace: true,
            all_targets: true,
            all_features: false,
            no_default_features: false,
            features: Vec::new(),
            exclude: Vec::new(),
            audit: false,
            deny: false,
        }
    }
}

impl VerificationConfig {
    pub(crate) fn cargo_arguments(
        &self,
        command: &str,
        include_targets: bool,
        doc_tests: bool,
    ) -> Vec<String> {
        let mut arguments = vec![command.to_string()];
        if self.workspace {
            arguments.push("--workspace".to_string());
        }
        if include_targets && self.all_targets {
            arguments.push("--all-targets".to_string());
        }
        if self.all_features {
            arguments.push("--all-features".to_string());
        } else {
            if self.no_default_features {
                arguments.push("--no-default-features".to_string());
            }
            if !self.features.is_empty() {
                arguments.extend(["--features".to_string(), self.features.join(",")]);
            }
        }
        if self.workspace {
            for package in &self.exclude {
                arguments.extend(["--exclude".to_string(), package.clone()]);
            }
        }
        if doc_tests {
            arguments.push("--doc".to_string());
        }
        if command == "doc" {
            arguments.push("--no-deps".to_string());
        }
        arguments
    }
}

impl From<Option<RawVerificationConfig>> for VerificationConfig {
    fn from(raw: Option<RawVerificationConfig>) -> Self {
        let raw = raw.unwrap_or_default();
        let defaults = Self::default();
        Self {
            timeout_seconds: raw
                .timeout_seconds
                .unwrap_or(defaults.timeout_seconds)
                .max(1),
            workspace: raw.workspace.unwrap_or(defaults.workspace),
            all_targets: raw.all_targets.unwrap_or(defaults.all_targets),
            all_features: raw.all_features.unwrap_or(defaults.all_features),
            no_default_features: raw
                .no_default_features
                .unwrap_or(defaults.no_default_features),
            features: raw.features.unwrap_or_default(),
            exclude: raw.exclude.unwrap_or_default(),
            audit: raw.audit.unwrap_or_default(),
            deny: raw.deny.unwrap_or_default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SemanticIdentityMode {
    #[default]
    Auto,
    Required,
    Disabled,
}

#[derive(Clone, Debug)]
pub(crate) struct LensConfig {
    pub(crate) project_name: String,
    pub(crate) project_root: PathBuf,
    pub(crate) source_roots: Vec<String>,
    pub(crate) output_dir: PathBuf,
    pub(crate) helper_manifest: PathBuf,
    pub(crate) identity_resolution: SemanticIdentityMode,
    pub(crate) rust_analyzer: PathBuf,
    pub(crate) identity_timeout_seconds: u64,
    pub(crate) identity_offline: bool,
    pub(crate) verification: VerificationConfig,
}

impl LensConfig {
    pub(crate) fn load(path: Option<PathBuf>) -> Result<Self> {
        let config_path = resolve_config_input(path)?;
        let config_dir = config_path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .unwrap_or(std::env::current_dir()?);
        let raw = match &config_path {
            Some(path) => toml::from_str::<RawConfig>(
                &fs::read_to_string(path)
                    .with_context(|| format!("reading config {}", path.display()))?,
            )?,
            _ => RawConfig {
                project_name: None,
                project_root: None,
                source_roots: None,
                output_dir: None,
                rust: None,
                verification: None,
            },
        };
        let project_root =
            resolve_config_path(raw.project_root.unwrap_or_else(|| ".".into()), &config_dir);
        let rust = raw.rust;
        let identity_offline = rust
            .as_ref()
            .and_then(|rust| rust.identity_offline)
            .unwrap_or(true);
        let source_roots = raw.source_roots.map_or_else(
            || discover_rust_source_roots(&project_root, identity_offline),
            |roots| {
                roots
                    .into_iter()
                    .map(|path| {
                        resolve_project_path(PathBuf::from(path), &project_root)
                            .to_string_lossy()
                            .to_string()
                    })
                    .collect()
            },
        );
        let output_dir = resolve_project_path(
            raw.output_dir.unwrap_or_else(|| "target/analysis".into()),
            &project_root,
        );
        let default_helper_manifest = bundled_helper_manifest()?;
        let helper_manifest = resolve_project_path(
            rust.as_ref()
                .and_then(|rust| rust.helper_manifest.clone())
                .unwrap_or(default_helper_manifest),
            &project_root,
        );
        let identity_resolution = rust
            .as_ref()
            .and_then(|rust| rust.identity_resolution)
            .unwrap_or_default();
        let rust_analyzer = rust
            .as_ref()
            .and_then(|rust| rust.rust_analyzer.clone())
            .unwrap_or_else(|| PathBuf::from("rust-analyzer"));
        let identity_timeout_seconds = rust
            .as_ref()
            .and_then(|rust| rust.identity_timeout_seconds)
            .unwrap_or(60)
            .max(1);
        let verification = VerificationConfig::from(raw.verification);
        let project_name = raw.project_name.unwrap_or_else(|| {
            project_root
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });
        Ok(Self {
            project_name,
            project_root,
            source_roots,
            output_dir,
            helper_manifest,
            identity_resolution,
            rust_analyzer,
            identity_timeout_seconds,
            identity_offline,
            verification,
        })
    }
}

fn discover_rust_source_roots(project_root: &Path, offline: bool) -> Vec<String> {
    let roots = cargo_metadata(project_root, offline, false)
        .or_else(|| cargo_metadata(project_root, offline, true))
        .map(|metadata| metadata_source_roots(project_root, &metadata))
        .unwrap_or_default();
    if roots.is_empty() {
        vec![project_root.join("src").to_string_lossy().to_string()]
    } else {
        roots
            .into_iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect()
    }
}

fn cargo_metadata(project_root: &Path, offline: bool, no_dependencies: bool) -> Option<Value> {
    let mut command = Command::new("cargo");
    command
        .args(["metadata", "--format-version", "1"])
        .current_dir(project_root);
    if offline {
        command.arg("--offline");
    }
    if no_dependencies {
        command.arg("--no-deps");
    }
    command
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| serde_json::from_slice(&output.stdout).ok())
}

fn metadata_source_roots(project_root: &Path, metadata: &Value) -> BTreeSet<PathBuf> {
    let project_root = absolutize(project_root);
    metadata["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|package| {
            let manifest = PathBuf::from(package["manifest_path"].as_str()?);
            let package_root = manifest.parent()?.to_path_buf();
            absolutize(&package_root)
                .starts_with(&project_root)
                .then_some((package, package_root))
        })
        .flat_map(|(package, package_root)| {
            let conventional = package_root.join("src");
            let mut roots = BTreeSet::new();
            if conventional.is_dir() {
                roots.insert(conventional);
            }
            for target in package["targets"].as_array().into_iter().flatten() {
                let Some(source) = target["src_path"].as_str().map(PathBuf::from) else {
                    continue;
                };
                if !source.starts_with(package_root.join("src")) {
                    roots.insert(source);
                }
            }
            roots
        })
        .collect()
}

pub(crate) fn discover_config(start: impl AsRef<Path>) -> Option<PathBuf> {
    let mut dir = absolutize(start);
    if dir.is_file() {
        dir.pop();
    }
    loop {
        let candidate = dir.join(CONFIG_FILE_NAME);
        if candidate.exists() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn resolve_config_input(path: Option<PathBuf>) -> Result<Option<PathBuf>> {
    if let Some(path) = path {
        let path = absolutize(path);
        if !path.exists() {
            bail!("config file does not exist: {}", path.display());
        }
        return Ok(Some(path));
    }
    Ok(discover_config(std::env::current_dir()?).map(absolutize))
}

pub(crate) fn write_default_config(path: Option<PathBuf>, force: bool) -> Result<PathBuf> {
    let target = absolutize(path.unwrap_or_else(|| CONFIG_FILE_NAME.into()));
    if target.exists() && !force {
        bail!(
            "config already exists at {}; pass --force to overwrite it",
            target.display()
        );
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let project_root = target
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or(std::env::current_dir()?);
    let project_name = project_root
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    fs::write(
        &target,
        format!(
            r#"project_name = "{project_name}"
project_root = "."
# source_roots is optional; Cargo workspace and local path packages are discovered automatically.
output_dir = "target/analysis"

[rust]
identity_resolution = "auto"
rust_analyzer = "rust-analyzer"
identity_timeout_seconds = 60
identity_offline = true

[verification]
timeout_seconds = 600
workspace = true
all_targets = true
all_features = false
audit = false
deny = false
"#
        ),
    )?;
    Ok(target)
}

pub(crate) fn config_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://example.local/rqlens.schema.json",
        "title": "rust-quality-lens config",
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "project_name": {"type": "string"},
            "project_root": {"type": "string", "default": "."},
            "source_roots": {
                "type": "array",
                "items": {"type": "string"},
                "description": "Optional explicit roots. When omitted, local Cargo package roots are discovered automatically."
            },
            "output_dir": {"type": "string", "default": "target/analysis"},
            "rust": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "helper_manifest": {"type": "string"},
                    "identity_resolution": {"type": "string", "enum": ["auto", "required", "disabled"], "default": "auto"},
                    "rust_analyzer": {"type": "string", "default": "rust-analyzer"},
                    "identity_timeout_seconds": {"type": "integer", "minimum": 1, "default": 60},
                    "identity_offline": {"type": "boolean", "default": true}
                }
            },
            "verification": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "timeout_seconds": {"type": "integer", "minimum": 1, "default": 600},
                    "workspace": {"type": "boolean", "default": true},
                    "all_targets": {"type": "boolean", "default": true},
                    "all_features": {"type": "boolean", "default": false},
                    "no_default_features": {"type": "boolean", "default": false},
                    "features": {"type": "array", "items": {"type": "string"}, "default": []},
                    "exclude": {"type": "array", "items": {"type": "string"}, "default": []},
                    "audit": {"type": "boolean", "default": false},
                    "deny": {"type": "boolean", "default": false}
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::metadata_source_roots;
    use serde_json::json;

    #[test]
    fn cargo_metadata_discovers_local_package_source_roots() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let application = root.path();
        let helper = root.path().join("crates/helper");
        std::fs::create_dir_all(application.join("src"))?;
        std::fs::create_dir_all(helper.join("src"))?;
        let metadata = json!({
            "packages": [
                {
                    "manifest_path": application.join("Cargo.toml"),
                    "targets": [{"src_path": application.join("src/lib.rs")}]
                },
                {
                    "manifest_path": helper.join("Cargo.toml"),
                    "targets": [{"src_path": helper.join("src/lib.rs")}]
                },
                {
                    "manifest_path": "/outside/registry/dependency/Cargo.toml",
                    "targets": [{"src_path": "/outside/registry/dependency/src/lib.rs"}]
                }
            ]
        });
        let roots = metadata_source_roots(application, &metadata);
        assert!(roots.contains(&application.join("src")));
        assert!(roots.contains(&helper.join("src")));
        assert_eq!(roots.len(), 2);
        Ok(())
    }
}
