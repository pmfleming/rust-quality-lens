use crate::util::{absolutize, repo_root, resolve_config_path, resolve_project_path};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

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
        let config_path = path.as_ref().map(absolutize);
        let config_dir = config_path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .unwrap_or(std::env::current_dir()?);
        let raw = match &config_path {
            Some(path) if path.exists() => toml::from_str::<RawConfig>(
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
