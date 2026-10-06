use super::{COMPILER_VERSION, Profile};
use crate::{command_runner::checked_output, config::LensConfig};
use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
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

// Decode Cargo's required fields once. Additional Cargo metadata is intentionally
// ignored, unlike the version-pinned compiler wire format.
#[derive(Deserialize)]
struct CargoInventory {
    packages: Vec<Package>,
    workspace_members: BTreeSet<String>,
    workspace_default_members: BTreeSet<String>,
    resolve: Resolution,
}

#[derive(Deserialize)]
struct Resolution {
    nodes: Vec<Features>,
}

#[derive(Deserialize)]
struct Features {
    id: String,
    features: BTreeSet<String>,
}

#[derive(Deserialize)]
struct Package {
    id: String,
    name: String,
    manifest_path: PathBuf,
    targets: Vec<Target>,
}

#[derive(Deserialize)]
struct Target {
    kind: Vec<String>,
    src_path: PathBuf,
    test: bool,
    #[serde(default, rename = "required-features")]
    required_features: BTreeSet<String>,
}

impl Target {
    fn is(&self, kind: &str) -> bool {
        self.kind.iter().any(|value| value == kind)
    }

    fn modes(&self, all_targets: bool) -> Result<&'static [bool]> {
        if self.is("custom-build") {
            return Ok(&[]);
        }
        if self.kind.iter().any(|kind| {
            [
                "lib",
                "bin",
                "proc-macro",
                "rlib",
                "dylib",
                "staticlib",
                "cdylib",
            ]
            .contains(&kind.as_str())
        }) {
            return Ok(if all_targets && self.test {
                &[false, true]
            } else {
                &[false]
            });
        }
        if !all_targets {
            return Ok(&[]);
        }
        ensure!(
            self.is("test"),
            "compiler inventory coverage for target kinds {:?} is not implemented; no completeness is certified",
            self.kind
        );
        Ok(&[true])
    }
}

#[derive(Default)]
struct Selection {
    units: BTreeSet<Unit>,
    // Canonicalize selected package ownership once, then reuse it for capture
    // validation instead of scanning/canonicalizing every dependency per profile.
    packages: BTreeMap<PathBuf, (String, BTreeSet<String>)>,
}

impl Selection {
    fn select_targets(
        &mut self,
        package: &Package,
        manifest: PathBuf,
        enabled: &BTreeSet<String>,
        all_targets: bool,
    ) -> Result<()> {
        for target in package
            .targets
            .iter()
            .filter(|target| target.required_features.is_subset(enabled))
        {
            let source = fs::canonicalize(&target.src_path)?;
            for &test in target.modes(all_targets)? {
                self.units.insert((manifest.clone(), source.clone(), test));
            }
        }
        Ok(())
    }

    fn new(metadata: &CargoInventory, config: &LensConfig) -> Result<Self> {
        let scope = &config.verification;
        let members = if scope.workspace {
            &metadata.workspace_members
        } else {
            &metadata.workspace_default_members
        };
        let mut selected = Self::default();
        for package in metadata.packages.iter().filter(|package| {
            members.contains(&package.id)
                && !(scope.workspace && scope.exclude.contains(&package.name))
        }) {
            let manifest = fs::canonicalize(
                package
                    .manifest_path
                    .parent()
                    .context("manifest has no directory")?,
            )?;
            let enabled = &metadata
                .resolve
                .nodes
                .iter()
                .find(|node| node.id == package.id)
                .context("selected package has no feature resolution")?
                .features;
            let feature_cfg = enabled
                .iter()
                .map(|feature| format!("feature={feature:?}"))
                .collect();
            ensure!(
                selected
                    .packages
                    .insert(manifest.clone(), (package.name.clone(), feature_cfg))
                    .is_none(),
                "ambiguous selected package"
            );
            selected.select_targets(package, manifest, enabled, scope.all_targets)?;
        }
        ensure!(
            !selected.units.is_empty(),
            "no supported selected Cargo compilation contexts"
        );
        Ok(selected)
    }

    fn validate_profile(&self, profile: &Profile, target: &str) -> Result<()> {
        profile.validate()?;
        ensure!(
            profile.target_triple == target,
            "unexpected compiler target"
        );
        let (name, expected) = self
            .packages
            .get(&profile.manifest_dir)
            .context("compiler context is not a selected metadata package")?;
        ensure!(
            name == &profile.package_name,
            "compiler package identity differs from Cargo metadata"
        );
        let actual = profile
            .cfg
            .iter()
            .filter(|cfg| cfg.starts_with("feature="))
            .cloned()
            .collect();
        ensure!(
            expected == &actual,
            "effective compiler feature cfg differs from requested Cargo resolution"
        );
        Ok(())
    }
}

pub(super) fn collect(config: &LensConfig) -> Result<(Vec<Profile>, BTreeSet<Unit>)> {
    let driver = config.compiler_inventory_driver.as_ref()
        .context("compiler inventory is not enabled; configure rust.compiler_inventory_driver for a trusted build")?;
    let info = driver_info(driver, config)?;
    let target = config
        .compiler_inventory_target
        .as_deref()
        .unwrap_or(&info.host);
    ensure!(
        !target.is_empty() && !target.starts_with('-') && !target.ends_with(".json"),
        "compiler inventory requires a named target triple, not a custom target JSON"
    );
    let metadata = cargo_inventory(config, &info, target)?;
    let selected = Selection::new(&metadata, config)?;
    let build_scripts = metadata
        .packages
        .iter()
        .flat_map(|package| &package.targets)
        .filter(|target| target.is("custom-build"))
        .map(|target| fs::canonicalize(&target.src_path))
        .collect::<std::io::Result<Vec<_>>>()?;

    // A fresh directory prevents cache hits from omitting wrapper invocations.
    fs::create_dir_all(config.target_directory())?;
    let run = tempfile::Builder::new()
        .prefix("rqlens-compiler-")
        .tempdir_in(config.target_directory())?;
    let captures = run.path().join("captures");
    fs::create_dir(&captures)?;
    let mut command = cargo(
        config,
        &info,
        config.verification.cargo_arguments("check", true, false),
    );
    command
        .args(["--target", target, "--message-format=json"])
        .arg("--target-dir")
        .arg(run.path().join("build"))
        .env("RUSTC_WORKSPACE_WRAPPER", driver)
        .env("RQLENS_COMPILER_CAPTURE", &captures)
        .env(
            "RQLENS_COMPILER_BUILD_SCRIPTS",
            serde_json::to_string(&build_scripts)?,
        );
    checked_output(
        &mut command,
        Duration::from_secs(config.verification.timeout_seconds),
    )
    .context("compiler inventory build failed; captured fragments were not promoted")?;
    let profiles = read_captures(&captures, &selected, target)?;
    Ok((profiles, selected.units))
}

fn cargo(config: &LensConfig, info: &DriverInfo, args: Vec<String>) -> Command {
    let mut command = Command::new("cargo");
    command
        .args(args)
        .current_dir(&config.project_root)
        .env("RUSTC", &info.rustc);
    if config.identity_offline {
        command.arg("--offline");
    }
    command
}

fn cargo_inventory(config: &LensConfig, info: &DriverInfo, target: &str) -> Result<CargoInventory> {
    let mut args = vec![
        "metadata".into(),
        "--format-version=1".into(),
        "--filter-platform".into(),
        target.into(),
    ];
    args.extend(config.verification.cargo_feature_arguments());
    if config.verification.locked {
        args.push("--locked".into());
    }
    let output = checked_output(
        &mut cargo(config, info, args),
        Duration::from_secs(config.verification.timeout_seconds),
    )?;
    Ok(serde_json::from_str(&output.stdout)?)
}

fn driver_info(driver: &Path, config: &LensConfig) -> Result<DriverInfo> {
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
    Ok(info)
}

fn read_captures(directory: &Path, selected: &Selection, target: &str) -> Result<Vec<Profile>> {
    let mut profiles = Vec::new();
    let mut total_bytes = 0;
    for entry in fs::read_dir(directory)? {
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
        selected.validate_profile(&profile, target)?;
        profiles.push(profile);
    }
    validate_coverage(&profiles, &selected.units)?;
    profiles.sort_by_key(|profile| {
        (
            profile.manifest_dir.clone(),
            profile.source_root.clone(),
            profile.test,
        )
    });
    Ok(profiles)
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
    if expected.is_empty() || &actual != expected {
        bail!(
            "missing or unexpected compiler contexts: missing {:?}; unexpected {:?}",
            expected.difference(&actual).collect::<Vec<_>>(),
            actual.difference(expected).collect::<Vec<_>>()
        );
    }
    Ok(())
}
