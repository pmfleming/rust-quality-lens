use serde_json::{Value, json};
use std::time::Duration;

use crate::command_runner::{CommandRequest, CommandStatus, run};
use crate::config::LensConfig;
use crate::util::stable_hash;

pub(crate) fn project(config: &LensConfig) -> Value {
    let toolchain = crate::toolchain::snapshot(&config.project_root);
    project_with_toolchain(config, &toolchain)
}

pub(crate) fn project_with_toolchain(
    config: &LensConfig,
    toolchain: &crate::toolchain::ToolchainSnapshot,
) -> Value {
    let source = config.source_fingerprint();
    let helper = config.helper_fingerprint();
    let mut errors = [&source, &helper]
        .into_iter()
        .flat_map(|input| input["read_errors"].as_array().into_iter().flatten())
        .cloned()
        .collect::<Vec<_>>();
    let git = git_state(config, &mut errors);
    if toolchain.rustc_version.is_none() || toolchain.cargo_version.is_none() {
        errors.push(json!("active Rust toolchain version is unavailable"));
    }
    let inputs = json!({
        "source": source["digest"],
        "helper": helper["digest"],
        "config": config,
        "git": git,
        "toolchain": toolchain,
        "generator_version": env!("CARGO_PKG_VERSION"),
        "compiler_environment": config.compiler_inventory_driver.as_ref().map(|_| {
            std::env::vars_os().filter_map(|(key, value)| {
                let key = key.to_str()?;
                (matches!(key, "RUSTFLAGS" | "CARGO_ENCODED_RUSTFLAGS" | "CARGO_BUILD_RUSTFLAGS"
                    | "RUSTC_BOOTSTRAP" | "RUSTUP_TOOLCHAIN" | "RUSTC_WRAPPER" | "RUSTC_WORKSPACE_WRAPPER"
                    | "CARGO_BUILD_RUSTC_WRAPPER" | "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER" | "RQLENS_COMPILER_CAPTURE")
                    || (key.starts_with("CARGO_TARGET_") && key.ends_with("_RUSTFLAGS")))
                    .then(|| (key.to_owned(), value.to_string_lossy().into_owned()))
            }).collect::<std::collections::BTreeMap<_, _>>()
        }),
    });
    json!({
        "algorithm": "fnv1a64-quality-inputs-v5",
        "digest": stable_hash(&inputs.to_string()),
        "file_count": source["file_count"],
        "read_errors": errors,
        "complete": errors.is_empty(),
        "source_digest": source["digest"],
        "helper_digest": helper["digest"],
        "config_digest": stable_hash(&inputs["config"].to_string()),
        "git": git,
        "toolchain": toolchain,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Freshness {
    Current,
    Stale,
    Incomplete,
}

pub(crate) fn freshness(document: &Value, current: &Value) -> Freshness {
    let recorded = &document["input_fingerprint"];
    if current["complete"] != true || recorded["complete"] != true {
        return Freshness::Incomplete;
    }
    if !current["digest"].is_string()
        || !current["algorithm"].is_string()
        || recorded["digest"] != current["digest"]
        || recorded["algorithm"] != current["algorithm"]
    {
        return Freshness::Stale;
    }
    Freshness::Current
}

fn git_state(config: &LensConfig, errors: &mut Vec<Value>) -> Value {
    let probe = git(config, &["rev-parse", "--is-inside-work-tree"]);
    if probe.status != CommandStatus::Passed {
        // A source archive need not contain Git metadata. Other failures must
        // not certify a complete fingerprint (e.g. missing git or timeouts).
        if probe.stderr.contains("not a git repository") {
            return json!({"status": "not-a-repository"});
        }
        errors.push(json!(format!(
            "cannot inspect Git state: {}",
            probe.reason.as_deref().unwrap_or(&probe.stderr_tail)
        )));
        return json!({"status": "unavailable"});
    }
    let mut state = serde_json::Map::new();
    for (key, args) in [
        ("head", vec!["rev-parse", "--verify", "HEAD"]),
        ("shallow", vec!["rev-parse", "--is-shallow-repository"]),
        (
            "replacements",
            vec![
                "for-each-ref",
                "--format=%(refname):%(objectname)",
                "refs/replace",
            ],
        ),
    ] {
        let output = git(config, &args);
        if output.status == CommandStatus::Passed {
            state.insert(key.to_string(), json!(output.stdout.trim()));
        } else {
            errors.push(json!(format!(
                "cannot inspect Git {key}: {}",
                output.stderr_tail
            )));
        }
    }
    if state.get("shallow").and_then(Value::as_str) == Some("true") {
        let output = git(config, &["rev-parse", "--git-path", "shallow"]);
        let path = config.project_root.join(output.stdout.trim());
        match std::fs::read_to_string(path) {
            Ok(text) if output.status == CommandStatus::Passed => {
                state.insert("shallow_boundary".to_string(), json!(stable_hash(&text)));
            }
            _ => errors.push(json!("cannot inspect Git shallow boundary")),
        }
    }
    Value::Object(state)
}

fn git(config: &LensConfig, args: &[&str]) -> crate::command_runner::CommandOutcome {
    let args = args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
    let mut request = CommandRequest::new("git", &args, &config.project_root);
    request.timeout = Duration::from_secs(3);
    request
        .environment
        .insert("LC_ALL".to_string(), "C".to_string());
    run(request)
}

#[cfg(test)]
mod tests {
    use super::{Freshness, freshness, git, project};
    use crate::{command_runner::CommandStatus, config::LensConfig};
    use serde_json::{Value, json};
    use std::fs;

    #[test]
    fn custom_effective_configuration_and_git_history_change_fingerprints() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        fs::create_dir(root.path().join("src"))?;
        fs::write(root.path().join("src/lib.rs"), "pub fn value() {}")?;
        let config_path = root.path().join("custom-settings.toml");
        fs::write(&config_path, "source_roots = [\"src\"]\n")?;
        let config = LensConfig::load(Some(config_path.clone()))?;
        let initial = project(&config);
        assert_eq!(initial["complete"], true);
        fs::write(
            &config_path,
            "source_roots = [\"src\"]\n[verification]\nall_features = true\n",
        )?;
        let changed = project(&LensConfig::load(Some(config_path))?);
        assert_ne!(initial["digest"], changed["digest"]);
        assert_eq!(initial["source_digest"], changed["source_digest"]);
        for args in [
            vec!["init", "-q"],
            vec![
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "--allow-empty",
                "-qm",
                "first",
            ],
        ] {
            assert_eq!(git(&config, &args).status, CommandStatus::Passed);
        }
        let first_commit = project(&config);
        assert_eq!(
            git(
                &config,
                &[
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@example.invalid",
                    "commit",
                    "--allow-empty",
                    "-qm",
                    "second"
                ]
            )
            .status,
            CommandStatus::Passed
        );
        let second_commit = project(&config);
        assert_ne!(first_commit["digest"], second_commit["digest"]);
        assert_eq!(
            first_commit["source_digest"],
            second_commit["source_digest"]
        );
        Ok(())
    }

    #[test]
    fn sibling_dependency_and_inherited_workspace_inputs_invalidate_evidence() -> anyhow::Result<()>
    {
        let root = tempfile::tempdir()?;
        let app = root.path().join("app");
        let shared = root.path().join("framework/core");
        fs::create_dir_all(app.join("src"))?;
        fs::create_dir_all(shared.join("src"))?;
        fs::write(
            app.join("Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n[dependencies]\nshared = { path = \"../framework/core\" }\n",
        )?;
        let workspace = root.path().join("framework/Cargo.toml");
        fs::write(
            &workspace,
            "[workspace]\nmembers = [\"core\"]\n[workspace.package]\nversion = \"0.1.0\"\n",
        )?;
        fs::write(
            shared.join("Cargo.toml"),
            "[package]\nname = \"shared\"\nversion.workspace = true\n",
        )?;
        fs::write(app.join("src/lib.rs"), "pub fn app() {}")?;
        let source = shared.join("src/lib.rs");
        fs::write(&source, "pub fn before() {}")?;
        let config_path = app.join("rqlens.toml");
        fs::write(&config_path, "source_roots = [\"src\"]\n")?;
        let config = LensConfig::load(Some(config_path))?;
        assert_eq!(
            config.source_roots.len(),
            1,
            "dependency code must not inflate daemon metrics"
        );
        let initial = config.source_fingerprint();
        assert_eq!(initial["complete"], true);
        crate::util::write_preserving_mtime(&source, "pub fn after_() {}")?;
        let changed = config.source_fingerprint();
        assert_ne!(initial["digest"], changed["digest"]);
        fs::write(
            &workspace,
            fs::read_to_string(&workspace)?.replace("0.1.0", "0.2.0"),
        )?;
        assert_ne!(changed["digest"], config.source_fingerprint()["digest"]);
        let mut unresolved = config.clone();
        let metadata = unresolved
            .cargo_metadata
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("missing fixture metadata"))?;
        metadata["resolve"] = Value::Null;
        assert_eq!(unresolved.source_fingerprint()["complete"], false);
        Ok(())
    }

    #[test]
    fn incomplete_or_legacy_fingerprints_are_never_current() {
        let current = json!({"algorithm": "v2", "digest": "same", "complete": true});
        assert_eq!(
            freshness(&json!({"input_fingerprint": current}), &current),
            Freshness::Current
        );
        for recorded in [
            json!({"digest": "same"}),
            json!({"algorithm": "v2", "digest": "same", "complete": false}),
        ] {
            assert_eq!(
                freshness(&json!({"input_fingerprint": recorded}), &current),
                Freshness::Incomplete
            );
        }
        assert_eq!(
            freshness(
                &json!({"input_fingerprint": {"algorithm": "v1", "digest": "same", "complete": true}}),
                &current
            ),
            Freshness::Stale
        );
    }
}
