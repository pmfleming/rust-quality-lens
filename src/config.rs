use crate::util::{
    absolutize, normalize_slashes, repo_root, resolve_config_path, resolve_project_path,
};
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) const CONFIG_FILE_NAME: &str = "rqlens.toml";

#[derive(Debug, Deserialize)]
struct RawConfig {
    project_name: Option<String>,
    project_root: Option<PathBuf>,
    source_roots: Option<Vec<String>>,
    output_dir: Option<PathBuf>,
    rust: Option<RawRustConfig>,
}

#[derive(Debug, Deserialize)]
struct RawRustConfig {
    helper_manifest: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub(crate) struct LensConfig {
    pub(crate) project_name: String,
    pub(crate) project_root: PathBuf,
    pub(crate) source_roots: Vec<String>,
    pub(crate) output_dir: PathBuf,
    pub(crate) helper_manifest: PathBuf,
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
            },
        };
        let project_root =
            resolve_config_path(raw.project_root.unwrap_or_else(|| ".".into()), &config_dir);
        let source_roots = raw
            .source_roots
            .unwrap_or_else(|| vec!["src".to_string()])
            .into_iter()
            .map(|path| {
                resolve_project_path(PathBuf::from(path), &project_root)
                    .to_string_lossy()
                    .to_string()
            })
            .collect();
        let output_dir = resolve_project_path(
            raw.output_dir.unwrap_or_else(|| "target/analysis".into()),
            &project_root,
        );
        let default_helper_manifest = repo_root()?.join("rust_helpers").join("Cargo.toml");
        let helper_manifest = resolve_project_path(
            raw.rust
                .and_then(|rust| rust.helper_manifest)
                .unwrap_or(default_helper_manifest),
            &project_root,
        );
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
        })
    }
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
    let helper_manifest = normalize_slashes(repo_root()?.join("rust_helpers").join("Cargo.toml"));
    fs::write(
        &target,
        format!(
            r#"project_name = "{project_name}"
project_root = "."
source_roots = ["src"]
output_dir = "target/analysis"

[rust]
helper_manifest = "{helper_manifest}"
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
                "default": ["src"]
            },
            "output_dir": {"type": "string", "default": "target/analysis"},
            "rust": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "helper_manifest": {"type": "string"}
                }
            }
        }
    })
}
