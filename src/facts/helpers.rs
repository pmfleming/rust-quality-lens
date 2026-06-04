use anyhow::{Context, Result, anyhow};
use serde_json::Value;
use std::env;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::NamedTempFile;

use crate::config::LensConfig;
use crate::facts::{AstCloneFact, FileFacts, HELPER_BINARIES, HelperBinaryKey};
use crate::util::{absolutize, iter_rust_files, normalize_slashes, resolve_project_path};

pub(super) fn rust_facts_for_paths(
    config: &LensConfig,
    paths: &[String],
) -> Result<Vec<FileFacts>> {
    let mut facts: Vec<FileFacts> =
        helper_json_for_paths(config, paths, "rust_facts", "Rust fact extraction")?;
    let targets = cargo_targets(config);
    for fact in &mut facts {
        let metadata = target_metadata(config, &fact.path, &targets);
        fact.target_kind = metadata.target_kind;
        fact.entrypoint_kind = metadata.entrypoint_kind;
        fact.is_entrypoint = fact.entrypoint_kind.is_some();
    }
    Ok(facts)
}

pub(crate) fn ast_clone_facts_for_paths(
    config: &LensConfig,
    paths: &[String],
) -> Result<Vec<AstCloneFact>> {
    helper_json_for_paths(config, paths, "ast_hasher", "AST clone extraction")
}

fn helper_json_for_paths<T: serde::de::DeserializeOwned>(
    config: &LensConfig,
    paths: &[String],
    bin: &str,
    label: &str,
) -> Result<Vec<T>> {
    let files = rust_file_args(paths);
    if files.is_empty() {
        return Ok(Vec::new());
    }
    let mut temp = NamedTempFile::new()?;
    for file in &files {
        writeln!(temp, "{file}")?;
    }
    let helper = helper_binary(config, bin)?;
    let mut command = Command::new(&helper);
    command.arg(temp.path()).current_dir(&config.project_root);
    if bin == "rust_facts" {
        command.env(
            "RQLENS_SOURCE_ROOTS",
            env::join_paths(config.source_roots.iter().map(PathBuf::from))?,
        );
    }
    let output = command
        .output()
        .with_context(|| format!("running {label} helper {}", helper.display()))?;
    if !output.status.success() {
        return Err(anyhow!(
            "{label} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn rust_file_args(paths: &[String]) -> Vec<String> {
    iter_rust_files(paths)
        .into_iter()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .collect()
}

fn helper_binary(config: &LensConfig, bin: &str) -> Result<PathBuf> {
    let manifest = canonical_or_original(&config.helper_manifest);
    let key = HelperBinaryKey {
        manifest: manifest.clone(),
        bin: bin.to_string(),
    };
    let cache =
        HELPER_BINARIES.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    if let Some(path) = cache
        .lock()
        .map_err(|_| anyhow!("helper binary cache is poisoned"))?
        .get(&key)
        .cloned()
    {
        return Ok(path);
    }

    build_helper(config, &manifest, bin)?;
    let binary = helper_target_dir(&manifest, &config.project_root)?
        .join("debug")
        .join(format!("{bin}{}", env::consts::EXE_SUFFIX));
    if !binary.exists() {
        return Err(anyhow!(
            "helper binary {bin} was built but not found at {}",
            binary.display()
        ));
    }

    cache
        .lock()
        .map_err(|_| anyhow!("helper binary cache is poisoned"))?
        .insert(key, binary.clone());
    Ok(binary)
}

fn build_helper(config: &LensConfig, manifest: &Path, bin: &str) -> Result<()> {
    let build = Command::new("cargo")
        .arg("build")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(manifest)
        .arg("--bin")
        .arg(bin)
        .current_dir(&config.project_root)
        .output()
        .with_context(|| format!("building helper binary {bin}"))?;
    if build.status.success() {
        Ok(())
    } else {
        Err(anyhow!(
            "helper binary build failed for {bin}: {}",
            String::from_utf8_lossy(&build.stderr)
        ))
    }
}

fn helper_target_dir(manifest: &Path, current_dir: &Path) -> Result<PathBuf> {
    if let Some(target_dir) = env::var_os("CARGO_TARGET_DIR") {
        return Ok(PathBuf::from(target_dir));
    }
    let output = Command::new("cargo")
        .arg("metadata")
        .arg("--no-deps")
        .arg("--format-version")
        .arg("1")
        .arg("--manifest-path")
        .arg(manifest)
        .current_dir(current_dir)
        .output()
        .context("reading helper cargo metadata")?;
    if !output.status.success() {
        return Err(anyhow!(
            "helper cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let metadata: Value = serde_json::from_slice(&output.stdout)?;
    metadata["target_directory"]
        .as_str()
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("helper cargo metadata did not include target_directory"))
}

fn canonical_or_original(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

struct TargetMetadata {
    target_kind: String,
    entrypoint_kind: Option<String>,
}

fn target_metadata(
    config: &LensConfig,
    path: &str,
    cargo_targets: &[(PathBuf, &'static str)],
) -> TargetMetadata {
    let absolute = path_key(resolve_project_path(
        PathBuf::from(path),
        &config.project_root,
    ));
    if let Some((_, kind)) = cargo_targets
        .iter()
        .find(|(target_path, _)| path_key(target_path) == absolute)
    {
        return target_metadata_for_kind(kind);
    }

    let relative = relative_project_path(config, path);
    let parts = relative.split('/').collect::<Vec<_>>();
    match parts.as_slice() {
        ["src", "lib.rs"] => target_metadata_for_kind("lib"),
        ["src", "main.rs"] | ["src", "bin", ..] => target_metadata_for_kind("bin"),
        ["tests", file] if file.ends_with(".rs") => target_metadata_for_kind("test"),
        ["benches", file] if file.ends_with(".rs") => target_metadata_for_kind("bench"),
        ["examples", file] if file.ends_with(".rs") => target_metadata_for_kind("example"),
        _ => target_metadata_for_kind("module"),
    }
}

fn target_metadata_for_kind(kind: &'static str) -> TargetMetadata {
    let entrypoint_kind = match kind {
        "bin" | "test" | "bench" | "example" => Some(kind.to_string()),
        _ => None,
    };
    TargetMetadata {
        target_kind: kind.to_string(),
        entrypoint_kind,
    }
}

fn cargo_targets(config: &LensConfig) -> Vec<(PathBuf, &'static str)> {
    let cargo = config.project_root.join("Cargo.toml");
    let Ok(text) = std::fs::read_to_string(cargo) else {
        return Vec::new();
    };
    let Ok(value) = text.parse::<toml::Value>() else {
        return Vec::new();
    };
    let mut targets = Vec::new();
    if let Some(path) = value
        .get("lib")
        .and_then(|lib| lib.get("path"))
        .and_then(toml::Value::as_str)
    {
        targets.push((
            resolve_project_path(PathBuf::from(path), &config.project_root),
            "lib",
        ));
    }
    for (key, kind) in [
        ("bin", "bin"),
        ("test", "test"),
        ("bench", "bench"),
        ("example", "example"),
    ] {
        targets.extend(cargo_target_paths(&value, key, kind, &config.project_root));
    }
    targets
}

fn cargo_target_paths(
    value: &toml::Value,
    key: &str,
    kind: &'static str,
    project_root: &Path,
) -> Vec<(PathBuf, &'static str)> {
    value
        .get(key)
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("path").and_then(toml::Value::as_str))
        .map(|path| {
            (
                resolve_project_path(PathBuf::from(path), project_root),
                kind,
            )
        })
        .collect()
}

fn relative_project_path(config: &LensConfig, path: &str) -> String {
    let path = resolve_project_path(PathBuf::from(path), &config.project_root);
    path.strip_prefix(&config.project_root)
        .map(normalize_slashes)
        .unwrap_or_else(|_| normalize_slashes(path))
}

fn path_key(path: impl AsRef<Path>) -> String {
    normalize_slashes(canonical_or_original(&absolutize(path))).to_lowercase()
}
