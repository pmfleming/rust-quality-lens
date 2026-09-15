use anyhow::{Context, Result, anyhow};
use serde_json::Value;
use std::env;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::NamedTempFile;

use crate::config::LensConfig;
use crate::facts::{AstCloneFact, FileFacts, HELPER_BINARIES, HelperBinaryKey};
use crate::util::{absolutize, normalize_slashes, resolve_project_path};

pub(super) fn rust_facts_for_files(
    config: &LensConfig,
    files: &[PathBuf],
    targets: &[CargoTarget],
) -> Result<Vec<FileFacts>> {
    let mut facts: Vec<FileFacts> =
        helper_json_for_files(config, files, "rust_facts", "Rust fact extraction")?;
    for fact in &mut facts {
        let metadata = target_metadata(config, &fact.path, targets);
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

pub(crate) fn ast_clone_facts_for_files(
    config: &LensConfig,
    files: &[PathBuf],
) -> Result<Vec<AstCloneFact>> {
    helper_json_for_files(config, files, "ast_hasher", "AST clone extraction")
}

fn helper_json_for_files<T: serde::de::DeserializeOwned>(
    config: &LensConfig,
    files: &[PathBuf],
    bin: &str,
    label: &str,
) -> Result<Vec<T>> {
    if files.is_empty() {
        return Ok(Vec::new());
    }
    let mut temp = NamedTempFile::new()?;
    for file in files {
        writeln!(temp, "{}", normalize_slashes(file))?;
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
    let output = crate::command_runner::checked_output(
        &mut command,
        std::time::Duration::from_secs(config.verification.timeout_seconds),
    )
    .with_context(|| format!("running {label} helper {}", helper.display()))?;
    Ok(serde_json::from_str(&output.stdout)?)
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

    let target = helper_target_dir(&manifest);
    build_helper(config, &manifest, bin, &target)?;
    let binary = target
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

fn build_helper(config: &LensConfig, manifest: &Path, bin: &str, target: &Path) -> Result<()> {
    crate::command_runner::checked_output(
        Command::new("cargo")
            .args(["build", "--quiet", "--manifest-path"])
            .arg(manifest)
            .args(["--locked", "--bin", bin, "--target-dir"])
            .arg(target)
            .current_dir(&config.project_root),
        std::time::Duration::from_secs(config.verification.timeout_seconds),
    )
    .with_context(|| format!("building helper binary {bin}"))?;
    Ok(())
}

// Ordinary project builds can contain same-named helper executables from another
// checkout. Keep each manifest's Cargo cache and executable paths isolated.
fn helper_target_dir(manifest: &Path) -> PathBuf {
    crate::util::helper_cache_root()
        .join("builds")
        .join(crate::util::stable_hash(&normalize_slashes(
            canonical_or_original(manifest),
        )))
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

pub(super) struct CargoTarget {
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

pub(super) fn cargo_targets(config: &LensConfig) -> Vec<CargoTarget> {
    let Some(metadata) = &config.cargo_metadata else {
        return manifest_targets(config);
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
    use super::{helper_binary, helper_target_dir, mark_test_module_findings};
    use crate::facts::{FileFacts, ModuleFileFact, QualityFindingFact};

    fn probe_helper(
        root: &std::path::Path,
        name: &str,
    ) -> anyhow::Result<crate::config::LensConfig> {
        let project = root.join(name);
        std::fs::create_dir_all(project.join("src"))?;
        std::fs::write(
            project.join("Cargo.toml"),
            "[package]\nname = 'helper-probe'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
        )?;
        std::fs::write(
            project.join("src/main.rs"),
            format!("fn main() {{ println!(\"{name}\"); }}"),
        )?;
        let path = project.join("rqlens.toml");
        std::fs::write(
            &path,
            format!(
                "project_root = '../project'\n[rust]\nhelper_manifest = '../{name}/Cargo.toml'\n"
            ),
        )?;
        crate::config::LensConfig::load(Some(path))
    }

    #[test]
    fn same_named_helpers_from_different_manifests_have_isolated_executables() -> anyhow::Result<()>
    {
        let root = tempfile::tempdir()?;
        std::fs::create_dir_all(root.path().join("project/src"))?;
        let mut binaries = Vec::new();
        for name in ["left", "right"] {
            let config = probe_helper(root.path(), name)?;
            let binary = helper_binary(&config, "helper-probe")?;
            let before = config.input_fingerprint();
            assert_eq!(before["complete"], true);
            crate::util::write_preserving_mtime(
                &root.path().join(name).join("src/main.rs"),
                format!("fn main() {{ println!(\"{name}\"); }}\n"),
            )?;
            let after = config.input_fingerprint();
            assert_ne!(before["source_digest"], after["source_digest"]);
            assert_ne!(before["helper_digest"], after["helper_digest"]);
            assert_eq!(config.source_roots.len(), 1);
            let mut unresolved = config.clone();
            unresolved.helper_metadata = None;
            assert_eq!(unresolved.source_fingerprint()["complete"], false);
            assert!(!binary.starts_with(config.target_directory()));
            binaries.push((binary, name, helper_target_dir(&config.helper_manifest)));
        }
        assert_ne!(binaries[0].0, binaries[1].0);
        for (binary, expected, cache) in binaries {
            let output = crate::command_runner::checked_output(
                &mut std::process::Command::new(binary),
                std::time::Duration::from_secs(10),
            )?;
            assert_eq!(output.stdout.trim(), expected);
            std::fs::remove_dir_all(cache)?;
        }
        Ok(())
    }

    #[test]
    fn external_cfg_test_modules_mark_nested_findings_as_test_code() {
        let mut parent = FileFacts::test_fact("src/lib.rs", "lib");
        parent.package_name = "demo".to_string();
        parent.graph.module_files.push(ModuleFileFact {
            module_key: "test_support".to_string(),
            path: "src/test_support.rs".to_string(),
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
