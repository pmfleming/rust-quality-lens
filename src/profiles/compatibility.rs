use super::{
    confidence,
    config::{CompatibilityProfile, Operation},
};
use crate::{
    command_runner::{CommandOutcome, CommandRequest, CommandStatus, run},
    config::LensConfig,
    toolchain::ToolchainSnapshot,
};
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::time::Duration;

pub(super) fn measure(
    config: &LensConfig,
    profile: &CompatibilityProfile,
    toolchain: &ToolchainSnapshot,
) -> Value {
    if profile
        .host_os
        .as_deref()
        .is_some_and(|os| os != std::env::consts::OS)
    {
        return json!({"id": profile.id, "profile": profile, "status": "skipped", "executed": false,
            "compilation_confirmed": false, "measurement_confidence": confidence("compatibility_profile", &["declared host OS is not this runner".into()])});
    }
    match execute(config, profile, toolchain) {
        Ok(row) => row,
        Err(error) => {
            json!({"id": profile.id, "profile": profile, "status": "unavailable", "executed": false,
            "compilation_confirmed": false, "measurement_confidence": confidence("compatibility_profile", &[error.to_string()])})
        }
    }
}

fn execute(
    config: &LensConfig,
    profile: &CompatibilityProfile,
    snapshot: &ToolchainSnapshot,
) -> Result<Value> {
    let selected = selected_toolchain(profile, snapshot)?;
    let direct =
        selected == "active" || snapshot.rustc_release.as_deref() == Some(selected.as_str());
    let rustc = invoke(
        config,
        &selected,
        direct,
        "rustc",
        &["--version".into(), "--verbose".into()],
        None,
    );
    let cargo = invoke(
        config,
        &selected,
        direct,
        "cargo",
        &["--version".into()],
        None,
    );
    if rustc.status != CommandStatus::Passed || cargo.status != CommandStatus::Passed {
        return Ok(
            json!({"id": profile.id, "profile": profile, "resolved_toolchain": selected,
            "status": "unavailable", "executed": false, "compilation_confirmed": false,
            "commands": [rustc, cargo], "measurement_confidence": confidence("compatibility_profile", &["selected compiler or Cargo is unavailable; no toolchains are automatically installed".into()])}),
        );
    }
    let directory = tempfile::tempdir()?;
    let args = arguments(config, profile);
    let outcome = invoke(
        config,
        &selected,
        direct,
        "cargo",
        &args,
        Some(directory.path()),
    );
    let missing_target = outcome.stderr.contains("rustup target add")
        || outcome.stdout.contains("target may not be installed");
    let complete = !missing_target && outcome.completed();
    let reasons = if complete {
        Vec::new()
    } else {
        vec![if missing_target {
            "target support is not installed".into()
        } else {
            "compatibility command did not finish normally".into()
        }]
    };
    let compiled = outcome
        .stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .any(|message| message["reason"] == "build-finished" && message["success"] == true);
    Ok(
        json!({"id": profile.id, "profile": profile, "resolved_toolchain": selected,
        "status": if missing_target { json!("unavailable") } else { json!(outcome.status) }, "executed": outcome.status != CommandStatus::Unavailable,
        "compilation_confirmed": compiled, "compiler": rustc.stdout.trim(), "cargo": cargo.stdout.trim(),
        "command": outcome, "measurement_confidence": confidence("compatibility_profile", &reasons)}),
    )
}

fn selected_toolchain(
    profile: &CompatibilityProfile,
    snapshot: &ToolchainSnapshot,
) -> Result<String> {
    if profile.toolchain != "msrv" {
        return Ok(profile.toolchain.clone());
    }
    let declared = snapshot
        .declared_rust_version
        .as_deref()
        .context("msrv profile requires a declared root/workspace rust-version")?;
    let (major, minor, patch) =
        crate::toolchain::parse_version(declared).context("invalid declared MSRV")?;
    Ok(format!("{major}.{minor}.{patch}"))
}

fn arguments(config: &LensConfig, profile: &CompatibilityProfile) -> Vec<String> {
    let mut scope = config.verification.clone();
    scope.no_default_features = profile.no_default_features;
    scope.all_features = profile.all_features;
    scope.features = profile.features.clone();
    let mut args = scope.cargo_arguments(
        if profile.operation == Operation::Check {
            "check"
        } else {
            "test"
        },
        true,
        false,
    );
    if let Some(target) = &profile.target {
        args.extend(["--target".into(), target.clone()]);
    }
    args.push("--message-format=json".into());
    args
}

fn invoke(
    config: &LensConfig,
    selected: &str,
    direct: bool,
    executable: &str,
    args: &[String],
    target_dir: Option<&std::path::Path>,
) -> CommandOutcome {
    let forwarded = if direct {
        args.to_vec()
    } else {
        [
            vec!["run".into(), selected.into(), executable.into()],
            args.to_vec(),
        ]
        .concat()
    };
    let mut request = CommandRequest::new(
        if direct { executable } else { "rustup" },
        &forwarded,
        &config.project_root,
    );
    request.timeout = Duration::from_secs(config.verification.timeout_seconds);
    if let Some(directory) = target_dir {
        request.environment.insert(
            "CARGO_TARGET_DIR".into(),
            directory.to_string_lossy().into_owned(),
        );
    }
    run(request)
}

#[cfg(test)]
mod tests {
    use super::{arguments, selected_toolchain};
    use crate::{config::LensConfig, profiles::config::CompatibilityProfile};
    use anyhow::Result;
    #[test]
    fn explicit_features_override_global_scope_and_msrv_is_resolved() -> Result<()> {
        let mut config = LensConfig::test_config("profile", std::path::Path::new("/project"));
        config.verification.all_features = true;
        config.verification.locked = true;
        let profile: CompatibilityProfile = toml::from_str(
            "id='minimal'\ntoolchain='msrv'\nno_default_features=true\ntarget='wasm32-unknown-unknown'\n",
        )?;
        let args = arguments(&config, &profile);
        assert!(args.contains(&"--no-default-features".into()));
        assert!(!args.contains(&"--all-features".into()));
        assert!(args.contains(&"--locked".into()));
        let mut snapshot = crate::toolchain::snapshot(std::path::Path::new("."));
        snapshot.declared_rust_version = Some("1.95".into());
        assert_eq!(selected_toolchain(&profile, &snapshot)?, "1.95.0");
        snapshot.declared_rust_version = None;
        assert!(selected_toolchain(&profile, &snapshot).is_err());
        Ok(())
    }
}
