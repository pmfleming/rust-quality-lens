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
        fact.package_name = metadata.package_name;
        fact.target_name = metadata.target_name;
        fact.module_id = format!(
            "{}::{}::{}",
            fact.package_name, fact.target_name, fact.module_key
        );
        fact.identity_backend = metadata.identity_backend;
    }
    mark_test_module_findings(&mut facts);
    Ok(facts)
}

fn mark_test_module_findings(facts: &mut [FileFacts]) {
    let test_modules = facts
        .iter()
        .flat_map(|fact| {
            fact.graph
                .module_files
                .iter()
                .filter(|module| module.test_code)
                .map(|module| (fact.package_name.clone(), module.module_key.clone()))
        })
        .collect::<Vec<_>>();
    for fact in facts {
        let is_test_module = test_modules.iter().any(|(package, module)| {
            fact.package_name == *package
                && (fact.module_key == *module
                    || fact
                        .module_key
                        .strip_prefix(module)
                        .is_some_and(|suffix| suffix.starts_with("::")))
        });
        if is_test_module {
            for finding in &mut fact.items.quality_findings {
                finding.test_code = true;
            }
        }
    }
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
    package_name: String,
    target_name: String,
    identity_backend: String,
}

struct CargoTarget {
    src_path: PathBuf,
    manifest_dir: PathBuf,
    package_name: String,
    target_name: String,
    kind: String,
    identity_backend: String,
}

fn target_metadata(
    config: &LensConfig,
    path: &str,
    cargo_targets: &[CargoTarget],
) -> TargetMetadata {
    let absolute = path_key(resolve_project_path(
        PathBuf::from(path),
        &config.project_root,
    ));
    if let Some(target) = cargo_targets
        .iter()
        .find(|target| path_key(&target.src_path) == absolute)
    {
        return target_metadata_for_target(target, &target.kind);
    }
    let absolute_path = canonical_or_original(&resolve_project_path(
        PathBuf::from(path),
        &config.project_root,
    ));
    if let Some(package) = cargo_targets
        .iter()
        .filter(|target| absolute_path.starts_with(&target.manifest_dir))
        .max_by_key(|target| target.manifest_dir.components().count())
    {
        let candidate_targets = cargo_targets
            .iter()
            .filter(|target| target.package_name == package.package_name)
            .filter(|target| {
                target
                    .src_path
                    .parent()
                    .is_some_and(|parent| absolute_path.starts_with(parent))
            })
            .collect::<Vec<_>>();
        let target_name = if candidate_targets.len() == 1 {
            candidate_targets[0].target_name.clone()
        } else {
            "shared".to_string()
        };
        let mut metadata = target_metadata_for_kind("module");
        metadata.package_name = package.package_name.clone();
        metadata.target_name = target_name;
        metadata.identity_backend = package.identity_backend.clone();
        return metadata;
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
        package_name: "unknown".to_string(),
        target_name: "shared".to_string(),
        identity_backend: "path_fallback".to_string(),
    }
}

fn target_metadata_for_target(target: &CargoTarget, kind: &str) -> TargetMetadata {
    let entrypoint_kind =
        matches!(kind, "bin" | "test" | "bench" | "example").then(|| kind.to_string());
    TargetMetadata {
        target_kind: kind.to_string(),
        entrypoint_kind,
        package_name: target.package_name.clone(),
        target_name: target.target_name.clone(),
        identity_backend: target.identity_backend.clone(),
    }
}

fn cargo_targets(config: &LensConfig) -> Vec<CargoTarget> {
    let output = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(&config.project_root)
        .output();
    let Ok(output) = output else {
        return manifest_targets(config);
    };
    if !output.status.success() {
        return manifest_targets(config);
    }
    let Ok(metadata) = serde_json::from_slice::<Value>(&output.stdout) else {
        return Vec::new();
    };
    metadata["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|package| {
            let package_name = package["name"].as_str().unwrap_or("unknown").to_string();
            let manifest_dir = package["manifest_path"]
                .as_str()
                .and_then(|path| Path::new(path).parent())
                .map(Path::to_path_buf)
                .unwrap_or_else(|| config.project_root.clone());
            package["targets"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(move |target| {
                    Some(CargoTarget {
                        src_path: PathBuf::from(target["src_path"].as_str()?),
                        manifest_dir: manifest_dir.clone(),
                        package_name: package_name.clone(),
                        target_name: target["name"].as_str()?.to_string(),
                        kind: cargo_target_kind(&target["kind"]),
                        identity_backend: "cargo_metadata".to_string(),
                    })
                })
        })
        .collect()
}

fn manifest_targets(config: &LensConfig) -> Vec<CargoTarget> {
    let manifest_path = config.project_root.join("Cargo.toml");
    let Ok(contents) = std::fs::read_to_string(&manifest_path) else {
        return Vec::new();
    };
    let Ok(manifest) = toml::from_str::<toml::Value>(&contents) else {
        return Vec::new();
    };
    let package_name = manifest["package"]["name"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();
    let mut targets = Vec::new();
    for (kind, default_path) in [("lib", "src/lib.rs"), ("bin", "src/main.rs")] {
        let path = config.project_root.join(default_path);
        if path.is_file() {
            targets.push(CargoTarget {
                src_path: path,
                manifest_dir: config.project_root.clone(),
                package_name: package_name.clone(),
                target_name: package_name.clone(),
                kind: kind.to_string(),
                identity_backend: "cargo_manifest".to_string(),
            });
        }
    }
    for kind in ["bin", "test", "bench", "example"] {
        for target in manifest
            .get(kind)
            .and_then(toml::Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(path) = target.get("path").and_then(toml::Value::as_str) else {
                continue;
            };
            targets.push(CargoTarget {
                src_path: config.project_root.join(path),
                manifest_dir: config.project_root.clone(),
                package_name: package_name.clone(),
                target_name: target
                    .get("name")
                    .and_then(toml::Value::as_str)
                    .unwrap_or(kind)
                    .to_string(),
                kind: kind.to_string(),
                identity_backend: "cargo_manifest".to_string(),
            });
        }
    }
    targets
}

fn cargo_target_kind(kinds: &Value) -> String {
    for candidate in ["lib", "bin", "test", "bench", "example"] {
        if kinds
            .as_array()
            .into_iter()
            .flatten()
            .any(|kind| kind == candidate)
        {
            return candidate.to_string();
        }
    }
    if kinds.as_array().into_iter().flatten().any(|kind| {
        matches!(
            kind.as_str(),
            Some("proc-macro" | "rlib" | "cdylib" | "dylib" | "staticlib")
        )
    }) {
        return "lib".to_string();
    }
    "module".to_string()
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

#[cfg(test)]
mod tests {
    use super::mark_test_module_findings;
    use crate::facts::{FileFacts, ModuleFileFact, QualityFindingFact};

    #[test]
    fn external_cfg_test_modules_mark_nested_findings_as_test_code() {
        let mut parent = FileFacts::test_fact("src/lib.rs", "lib");
        parent.package_name = "demo".to_string();
        parent.graph.module_files.push(ModuleFileFact {
            module_key: "test_support".to_string(),
            test_code: true,
        });
        let mut child = FileFacts::test_fact("src/test_support/nested.rs", "test_support::nested");
        child.package_name = "demo".to_string();
        child.items.quality_findings.push(QualityFindingFact {
            rule_id: "rust.reliability.expect".to_string(),
            kind: "panic-path".to_string(),
            line: 1,
            message: "expect may panic".to_string(),
            test_code: false,
        });

        let mut facts = vec![parent, child];
        mark_test_module_findings(&mut facts);

        assert!(facts[1].items.quality_findings[0].test_code);
    }
}
