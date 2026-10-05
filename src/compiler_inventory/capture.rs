use super::{COMPILER_VERSION, Profile};
use crate::{command_runner::checked_output, config::LensConfig};
use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::PathBuf,
    process::Command,
    time::Duration,
};

type Unit = (PathBuf, PathBuf, bool);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DriverInfo {
    schema_version: u64,
    backend: String,
    compiler_version: String,
    rustc: PathBuf,
    host: String,
}

pub(super) fn collect(config: &LensConfig) -> Result<(Vec<Profile>, BTreeSet<Unit>)> {
    let driver = config.compiler_inventory_driver.as_ref()
        .context("compiler inventory is not enabled; configure rust.compiler_inventory_driver for a trusted build")?;
    for variable in [
        "RUSTC_BOOTSTRAP",
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
        "CARGO_BUILD_RUSTC_WRAPPER",
        "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
        "RQLENS_COMPILER_CAPTURE",
    ] {
        ensure!(
            env::var_os(variable).is_none_or(|value| value.is_empty()),
            "compiler inventory does not support inherited {variable}"
        );
    }
    let info: DriverInfo = serde_json::from_str(
        &checked_output(
            Command::new(driver)
                .arg("--compiler-info")
                .current_dir(&config.project_root),
            Duration::from_secs(10),
        )?
        .stdout,
    )?;
    ensure!(
        info.schema_version == 1
            && info.backend == "rustc-hir"
            && info.compiler_version == COMPILER_VERSION
            && !info.host.is_empty(),
        "incompatible compiler adapter"
    );
    let version = checked_output(
        Command::new(&info.rustc).arg("--version"),
        Duration::from_secs(10),
    )?;
    ensure!(
        version.stdout.trim() == info.compiler_version,
        "adapter and compiler versions differ"
    );
    let target = config
        .compiler_inventory_target
        .as_deref()
        .unwrap_or(&info.host);
    ensure!(
        !target.is_empty() && !target.starts_with('-') && !target.ends_with(".json"),
        "compiler inventory requires a named target triple, not a custom target JSON"
    );
    let timeout = Duration::from_secs(config.verification.timeout_seconds);
    let mut metadata = Command::new("cargo");
    metadata
        .args([
            "metadata",
            "--format-version=1",
            "--filter-platform",
            target,
        ])
        .args(config.verification.cargo_feature_arguments())
        .current_dir(&config.project_root)
        .env("RUSTC", &info.rustc);
    if config.verification.locked {
        metadata.arg("--locked");
    }
    if config.identity_offline {
        metadata.arg("--offline");
    }
    let metadata: Value = serde_json::from_str(&checked_output(&mut metadata, timeout)?.stdout)?;
    let build_scripts = metadata["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|package| package["targets"].as_array().into_iter().flatten())
        .filter(|target| {
            target["kind"]
                .as_array()
                .is_some_and(|kinds| kinds.iter().any(|kind| kind == "custom-build"))
        })
        .filter_map(|target| target["src_path"].as_str())
        .map(fs::canonicalize)
        .collect::<std::io::Result<Vec<_>>>()?;
    let expected = expected_units(&metadata, config)?;
    ensure!(
        !expected.is_empty(),
        "no supported selected Cargo compilation contexts"
    );

    // A fresh directory prevents Cargo's cache from omitting wrapper invocations.
    // Under Cargo's target root, so measurement fingerprints exclude build output.
    fs::create_dir_all(config.target_directory())?;
    let run = tempfile::Builder::new()
        .prefix("rqlens-compiler-")
        .tempdir_in(config.target_directory())?;
    let captures = run.path().join("captures");
    fs::create_dir(&captures)?;
    let mut command = Command::new("cargo");
    command
        .args(config.verification.cargo_arguments("check", true, false))
        .args(["--target", target, "--message-format=json"])
        .arg("--target-dir")
        .arg(run.path().join("build"))
        .current_dir(&config.project_root)
        .env("RUSTC", &info.rustc)
        .env("RUSTC_WORKSPACE_WRAPPER", driver)
        .env("RQLENS_COMPILER_CAPTURE", &captures)
        .env(
            "RQLENS_COMPILER_BUILD_SCRIPTS",
            serde_json::to_string(&build_scripts)?,
        );
    if config.identity_offline {
        command.arg("--offline");
    }
    checked_output(&mut command, timeout)
        .context("compiler inventory build failed; captured fragments were not promoted")?;
    let mut profiles = Vec::new();
    let mut total_bytes = 0;
    for entry in fs::read_dir(&captures)? {
        let path = entry?.path();
        ensure!(
            path.extension()
                .is_some_and(|extension| extension == "json"),
            "unfinished compiler capture"
        );
        let bytes = fs::metadata(&path)?.len();
        total_bytes += bytes;
        ensure!(
            bytes <= 64 * 1024 * 1024 && total_bytes <= 256 * 1024 * 1024,
            "compiler inventory exceeds capture budget"
        );
        let profile: Profile = serde_json::from_slice(&fs::read(path)?)?;
        profile.validate()?;
        ensure!(
            profile.target_triple == target,
            "unexpected compiler target"
        );
        validate_package_features(&profile, &metadata)?;
        profiles.push(profile);
    }
    validate_coverage(&profiles, &expected)?;
    profiles.sort_by_key(|profile| {
        (
            profile.manifest_dir.clone(),
            profile.source_root.clone(),
            profile.test,
        )
    });
    Ok((profiles, expected))
}

fn validate_package_features(profile: &Profile, metadata: &Value) -> Result<()> {
    let package = metadata["packages"]
        .as_array()
        .context("missing packages")?
        .iter()
        .find(|package| {
            package["manifest_path"]
                .as_str()
                .and_then(|path| PathBuf::from(path).parent().map(PathBuf::from))
                .is_some_and(|manifest| {
                    fs::canonicalize(manifest).ok().as_ref() == Some(&profile.manifest_dir)
                })
        })
        .context("compiler context is not a metadata package")?;
    ensure!(
        package["name"] == profile.package_name,
        "compiler package identity differs from Cargo metadata"
    );
    let node = metadata["resolve"]["nodes"]
        .as_array()
        .context("missing resolved features")?
        .iter()
        .find(|node| node["id"] == package["id"])
        .context("compiler package has no resolved feature node")?;
    let expected = node["features"]
        .as_array()
        .context("missing package features")?
        .iter()
        .map(|feature| {
            feature
                .as_str()
                .map(|feature| format!("feature={feature:?}"))
                .context("invalid feature name")
        })
        .collect::<Result<BTreeSet<_>>>()?;
    let actual = profile
        .cfg
        .iter()
        .filter(|cfg| cfg.starts_with("feature="))
        .cloned()
        .collect::<BTreeSet<_>>();
    ensure!(
        expected == actual,
        "effective compiler feature cfg differs from requested Cargo resolution"
    );
    Ok(())
}

pub(super) fn validate_coverage(profiles: &[Profile], expected: &BTreeSet<Unit>) -> Result<()> {
    let mut actual = BTreeSet::new();
    for profile in profiles {
        ensure!(
            actual.insert((
                profile.manifest_dir.clone(),
                profile.source_root.clone(),
                profile.test
            )),
            "duplicate compilation context; cannot deduplicate distinct compiler invocations by name"
        );
    }
    ensure!(
        !expected.is_empty() && &actual == expected,
        "missing or unexpected compiler contexts: missing {:?}; unexpected {:?}",
        expected.difference(&actual).collect::<Vec<_>>(),
        actual.difference(expected).collect::<Vec<_>>()
    );
    Ok(())
}

fn expected_units(metadata: &Value, config: &LensConfig) -> Result<BTreeSet<Unit>> {
    let selected = metadata[if config.verification.workspace {
        "workspace_members"
    } else {
        "workspace_default_members"
    }]
    .as_array()
    .context("missing workspace selection metadata")?;
    let features = metadata["resolve"]["nodes"]
        .as_array()
        .context("missing resolved feature metadata")?
        .iter()
        .map(|node| (node["id"].clone(), node["features"].clone()))
        .collect::<Vec<_>>();
    let mut expected = BTreeSet::new();
    let mut package_names = BTreeMap::new();
    for package in metadata["packages"]
        .as_array()
        .context("missing Cargo packages")?
    {
        let name = package["name"].as_str().context("missing package name")?;
        if !selected.contains(&package["id"])
            || (config.verification.workspace
                && config
                    .verification
                    .exclude
                    .iter()
                    .any(|excluded| excluded == name))
        {
            continue;
        }
        let manifest = PathBuf::from(
            package["manifest_path"]
                .as_str()
                .context("missing manifest")?,
        );
        let manifest = fs::canonicalize(manifest.parent().context("manifest has no directory")?)?;
        ensure!(
            package_names.insert(manifest.clone(), name).is_none(),
            "ambiguous selected package"
        );
        let enabled = features
            .iter()
            .find(|(id, _)| id == &package["id"])
            .and_then(|(_, value)| value.as_array())
            .context("selected package has no feature resolution")?;
        for target in package["targets"]
            .as_array()
            .context("missing Cargo targets")?
        {
            if target["required-features"]
                .as_array()
                .is_some_and(|required| required.iter().any(|feature| !enabled.contains(feature)))
            {
                continue;
            }
            let kinds = target["kind"].as_array().context("missing target kind")?;
            let source = fs::canonicalize(
                target["src_path"]
                    .as_str()
                    .context("missing target source")?,
            )?;
            if kinds.iter().any(|kind| kind == "custom-build") {
                continue;
            }
            if kinds.iter().any(|kind| {
                [
                    "lib",
                    "bin",
                    "proc-macro",
                    "rlib",
                    "dylib",
                    "staticlib",
                    "cdylib",
                ]
                .contains(&kind.as_str().unwrap_or_default())
            }) {
                expected.insert((manifest.clone(), source.clone(), false));
                if config.verification.all_targets && target["test"] == true {
                    expected.insert((manifest.clone(), source, true));
                }
            } else if config.verification.all_targets {
                if kinds.iter().any(|kind| kind == "test") {
                    expected.insert((manifest.clone(), source, true));
                } else {
                    bail!(
                        "compiler inventory coverage for target kinds {kinds:?} is not implemented; no completeness is certified"
                    );
                }
            }
        }
    }
    Ok(expected)
}
