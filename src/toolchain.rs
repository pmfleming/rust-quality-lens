use serde::Serialize;
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ToolchainSnapshot {
    pub(crate) rustc_version: Option<String>,
    pub(crate) rustc_release: Option<String>,
    pub(crate) cargo_version: Option<String>,
    pub(crate) host: Option<String>,
    pub(crate) llvm_version: Option<String>,
    pub(crate) declared_rust_version: Option<String>,
    pub(crate) pinned_channel: Option<String>,
}

pub(crate) fn snapshot(project_root: &Path) -> ToolchainSnapshot {
    let rustc = command_lines(project_root, "rustc", &["--version", "--verbose"]);
    let cargo = command_lines(project_root, "cargo", &["--version"]);
    ToolchainSnapshot {
        rustc_version: rustc.first().cloned(),
        rustc_release: verbose_field(&rustc, "release"),
        cargo_version: cargo.first().cloned(),
        host: verbose_field(&rustc, "host"),
        llvm_version: verbose_field(&rustc, "LLVM version"),
        declared_rust_version: declared_rust_version(project_root),
        pinned_channel: pinned_channel(project_root),
    }
}

pub(crate) fn as_json(snapshot: &ToolchainSnapshot) -> Value {
    serde_json::to_value(snapshot).unwrap_or_else(|_| json!({}))
}

pub(crate) fn quality_checks(snapshot: &ToolchainSnapshot) -> Vec<Value> {
    vec![
        version_check(
            "rust.project.active-toolchain-compatible",
            "Active Rust compiler satisfies the declared minimum",
            snapshot.declared_rust_version.as_deref(),
            snapshot.rustc_release.as_deref(),
            "active_rustc_release",
            true,
        ),
        version_check(
            "rust.project.pinned-toolchain-compatible",
            "Pinned Rust toolchain satisfies the declared minimum",
            snapshot.declared_rust_version.as_deref(),
            snapshot.pinned_channel.as_deref(),
            "pinned_channel",
            false,
        ),
    ]
}

fn version_check(
    rule_id: &str,
    title: &str,
    required: Option<&str>,
    observed: Option<&str>,
    observed_key: &str,
    active: bool,
) -> Value {
    let source = "https://doc.rust-lang.org/cargo/reference/rust-version.html";
    let (Some(required), Some(observed)) = (required, observed) else {
        return json!({
            "rule_id": rule_id,
            "title": title,
            "category": "verified-gate",
            "severity": if active { "error" } else { "warning" },
            "status": "skipped",
            "source": source,
            "evidence": {
                "reason": "both a declared rust-version and an inspectable toolchain version are required",
                "declared_rust_version": required,
                (observed_key): observed,
            },
        });
    };
    let required_version = parse_version(required);
    let observed_version = parse_version(observed);
    let compatible = required_version
        .zip(observed_version)
        .is_some_and(|(required, observed)| observed >= required);
    json!({
        "rule_id": rule_id,
        "title": title,
        "category": "verified-gate",
        "severity": if active { "error" } else { "warning" },
        "status": if compatible { "passed" } else { "failed" },
        "source": source,
        "evidence": {
            "declared_rust_version": required,
            (observed_key): observed,
            "version_parse_complete": required_version.is_some() && observed_version.is_some(),
            "exact_match": required_version == observed_version,
        },
    })
}

pub(crate) fn parse_version(value: &str) -> Option<(u64, u64, u64)> {
    let numeric = value
        .trim()
        .strip_prefix("rustc ")
        .unwrap_or(value.trim())
        .split(['-', ' '])
        .next()?;
    let mut parts = numeric.split('.').map(|part| part.parse::<u64>().ok());
    Some((
        parts.next()??,
        parts.next().flatten().unwrap_or(0),
        parts.next().flatten().unwrap_or(0),
    ))
}

fn command_lines(current_dir: &Path, program: &str, args: &[&str]) -> Vec<String> {
    Command::new(program)
        .args(args)
        .current_dir(current_dir)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn verbose_field(lines: &[String], field: &str) -> Option<String> {
    lines
        .iter()
        .find_map(|line| line.strip_prefix(&format!("{field}: ")).map(str::to_string))
}

fn declared_rust_version(project_root: &Path) -> Option<String> {
    let contents = fs::read_to_string(project_root.join("Cargo.toml")).ok()?;
    let manifest = toml::from_str::<toml::Value>(&contents).ok()?;
    manifest
        .get("package")
        .and_then(|package| package.get("rust-version"))
        .and_then(toml::Value::as_str)
        .or_else(|| {
            manifest
                .get("workspace")
                .and_then(|workspace| workspace.get("package"))
                .and_then(|package| package.get("rust-version"))
                .and_then(toml::Value::as_str)
        })
        .map(str::to_string)
}

fn pinned_channel(project_root: &Path) -> Option<String> {
    for directory in project_root.ancestors() {
        match fs::read_to_string(directory.join("rust-toolchain.toml")) {
            Ok(contents) if let Ok(document) = toml::from_str::<toml::Value>(&contents) => {
                if let Some(channel) = document
                    .get("toolchain")
                    .and_then(|toolchain| toolchain.get("channel"))
                    .and_then(toml::Value::as_str)
                {
                    return Some(channel.to_string());
                }
            }
            _ => {}
        }
        if let Some(channel) = fs::read_to_string(directory.join("rust-toolchain"))
            .ok()
            .map(|contents| contents.trim().to_string())
            .filter(|channel| !channel.is_empty())
        {
            return Some(channel);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{parse_version, pinned_channel, quality_checks, snapshot};

    #[test]
    fn versions_accept_release_and_channel_forms() {
        assert_eq!(parse_version("1.95"), Some((1, 95, 0)));
        assert_eq!(
            parse_version("1.95.0-x86_64-unknown-linux-gnu"),
            Some((1, 95, 0))
        );
        assert_eq!(
            parse_version("rustc 1.95.0 (abc 2026-04-16)"),
            Some((1, 95, 0))
        );
        assert_eq!(parse_version("stable"), None);
    }

    #[test]
    fn reads_toml_toolchain_with_an_if_let_guard() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        std::fs::write(
            root.path().join("rust-toolchain.toml"),
            "[toolchain]\nchannel = \"1.95.0\"\n",
        )?;
        assert_eq!(pinned_channel(root.path()).as_deref(), Some("1.95.0"));
        Ok(())
    }

    #[test]
    fn current_project_toolchain_is_compatible() {
        let snapshot = snapshot(std::path::Path::new(env!("CARGO_MANIFEST_DIR")));
        let checks = quality_checks(&snapshot);
        assert!(checks.iter().all(|check| check["status"] == "passed"));
    }
}
