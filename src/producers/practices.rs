use anyhow::Result;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

use crate::command_runner::{CommandOutcome, CommandRequest, CommandStatus, run};
use crate::config::LensConfig;

pub(super) fn produce(config: &LensConfig) -> Result<Value> {
    let mut checks = Vec::new();
    checks.push(cargo_check(
        config,
        "rust.official.rustfmt",
        "Formatting matches rustfmt",
        ["fmt", "--all", "--", "--check"]
            .map(str::to_string)
            .to_vec(),
        "https://github.com/rust-lang/rustfmt",
        BTreeMap::new(),
    ));
    checks.push(cargo_check(
        config,
        "rust.official.cargo-check",
        "All configured targets compile",
        scoped_args(config, "check", true, false),
        "https://doc.rust-lang.org/cargo/commands/cargo-check.html",
        BTreeMap::new(),
    ));
    let mut clippy_args = scoped_args(config, "clippy", true, false);
    clippy_args.extend(["--".to_string(), "-D".to_string(), "warnings".to_string()]);
    checks.push(cargo_check(
        config,
        "rust.official.clippy",
        "Clippy reports no warnings",
        clippy_args,
        "https://doc.rust-lang.org/clippy/",
        BTreeMap::new(),
    ));
    checks.push(cargo_check(
        config,
        "rust.official.tests",
        "Cargo tests pass",
        scoped_args(config, "test", false, false),
        "https://doc.rust-lang.org/cargo/commands/cargo-test.html",
        BTreeMap::new(),
    ));
    checks.push(cargo_check(
        config,
        "rust.official.doctests",
        "Rust documentation tests pass",
        scoped_args(config, "test", false, true),
        "https://doc.rust-lang.org/rustdoc/write-documentation/documentation-tests.html",
        BTreeMap::new(),
    ));
    checks.push(cargo_check(
        config,
        "rust.official.rustdoc",
        "Rust documentation builds without warnings",
        scoped_args(config, "doc", false, false),
        "https://doc.rust-lang.org/rustdoc/",
        BTreeMap::from([("RUSTDOCFLAGS".to_string(), "-D warnings".to_string())]),
    ));

    if config.verification.audit {
        checks.push(cargo_check(
            config,
            "rust.security.rustsec",
            "Dependencies have no known RustSec advisories",
            vec!["audit".to_string()],
            "https://rustsec.org/",
            BTreeMap::new(),
        ));
    } else {
        checks.push(skipped_check(
            "rust.security.rustsec",
            "Dependency advisory scan",
            "enable verification.audit to require cargo-audit",
            "https://rustsec.org/",
        ));
    }
    if config.verification.deny {
        checks.push(cargo_check(
            config,
            "rust.supply-chain.cargo-deny",
            "Dependency policy passes cargo-deny",
            vec!["deny".to_string(), "check".to_string()],
            "https://embarkstudios.github.io/cargo-deny/",
            BTreeMap::new(),
        ));
    } else {
        checks.push(skipped_check(
            "rust.supply-chain.cargo-deny",
            "Dependency license and source policy",
            "enable verification.deny to require cargo-deny",
            "https://embarkstudios.github.io/cargo-deny/",
        ));
    }

    checks.extend(project_evidence(config));
    let failed_errors = count(&checks, "failed", "error");
    let failed_warnings = count(&checks, "failed", "warning");
    let unavailable = checks
        .iter()
        .filter(|check| matches!(check["status"].as_str(), Some("unavailable" | "timed-out")))
        .count();
    let passed = checks
        .iter()
        .filter(|check| check["status"] == "passed")
        .count();
    let skipped = checks
        .iter()
        .filter(|check| check["status"] == "skipped")
        .count();
    Ok(json!({
        "version": 1,
        "profile": "baseline",
        "summary": {
            "check_count": checks.len(),
            "passed": passed,
            "failed_errors": failed_errors,
            "failed_warnings": failed_warnings,
            "unavailable": unavailable,
            "skipped": skipped,
            "conformant": failed_errors == 0 && unavailable == 0,
        },
        "checks": checks,
        "measurement_confidence": {
            "complete": unavailable == 0,
            "partial": unavailable > 0,
            "confidence_scope": "verified_practices",
            "required_inputs": ["cargo", "rustfmt", "clippy", "rustdoc"],
            "observed_inputs": {"completed_checks": passed + failed_errors + failed_warnings},
            "missing_input": checks.iter().filter(|check| check["status"] == "unavailable").filter_map(|check| check["rule_id"].as_str()).collect::<Vec<_>>(),
            "stale_input": [],
            "unsupported_pattern": checks.iter().filter(|check| check["status"] == "timed-out").filter_map(|check| check["rule_id"].as_str()).collect::<Vec<_>>(),
        }
    }))
}

fn cargo_check(
    config: &LensConfig,
    rule_id: &str,
    title: &str,
    arguments: Vec<String>,
    source: &str,
    environment: BTreeMap<String, String>,
) -> Value {
    let mut request = CommandRequest::new("cargo", &arguments, &config.project_root);
    request.timeout = Duration::from_secs(config.verification.timeout_seconds);
    request.environment = environment;
    let outcome = run(request);
    command_check(rule_id, title, source, outcome)
}

fn command_check(rule_id: &str, title: &str, source: &str, outcome: CommandOutcome) -> Value {
    let status = match outcome.status {
        CommandStatus::Passed => "passed",
        CommandStatus::Failed => "failed",
        CommandStatus::Unavailable => "unavailable",
        CommandStatus::TimedOut => "timed-out",
    };
    json!({
        "rule_id": rule_id,
        "title": title,
        "category": "verified-gate",
        "severity": "error",
        "status": status,
        "source": source,
        "tool": "cargo",
        "tool_version": cargo_version(),
        "evidence": outcome,
    })
}

fn cargo_version() -> &'static str {
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION
        .get_or_init(|| {
            Command::new("cargo")
                .arg("--version")
                .output()
                .ok()
                .filter(|output| output.status.success())
                .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
                .unwrap_or_else(|| "unavailable".to_string())
        })
        .as_str()
}

fn scoped_args(
    config: &LensConfig,
    command: &str,
    include_targets: bool,
    doc_tests: bool,
) -> Vec<String> {
    config
        .verification
        .cargo_arguments(command, include_targets, doc_tests)
}

fn project_evidence(config: &LensConfig) -> Vec<Value> {
    let manifest = fs::read_to_string(config.project_root.join("Cargo.toml"))
        .ok()
        .and_then(|contents| contents.parse::<toml::Value>().ok());
    let package = manifest
        .as_ref()
        .and_then(|manifest| manifest.get("package"))
        .or_else(|| {
            manifest
                .as_ref()
                .and_then(|manifest| manifest.get("workspace"))
                .and_then(|workspace| workspace.get("package"))
        });
    let has = |key: &str| package.and_then(|package| package.get(key)).is_some();
    let files = |names: &[&str]| {
        names
            .iter()
            .any(|name| config.project_root.join(name).is_file())
    };
    vec![
        evidence_check(
            "rust.project.msrv-declared",
            "Minimum supported Rust version is declared",
            has("rust-version"),
            "https://doc.rust-lang.org/cargo/reference/rust-version.html",
        ),
        evidence_check(
            "rust.project.license",
            "License metadata and text are present",
            (has("license") || has("license-file"))
                && files(&["LICENSE", "LICENSE.md", "LICENSE-MIT", "LICENSE-APACHE"]),
            "https://doc.rust-lang.org/cargo/reference/manifest.html#the-license-and-license-file-fields",
        ),
        evidence_check(
            "rust.project.repository",
            "Cargo package repository metadata is present",
            has("repository"),
            "https://doc.rust-lang.org/cargo/reference/manifest.html#the-repository-field",
        ),
        evidence_check(
            "rust.project.contributing",
            "Contribution instructions are present",
            files(&["CONTRIBUTING.md", ".github/CONTRIBUTING.md"]),
            "https://rustc-dev-guide.rust-lang.org/getting-started.html",
        ),
        evidence_check(
            "rust.project.code-of-conduct",
            "A code of conduct is present",
            files(&["CODE_OF_CONDUCT.md", ".github/CODE_OF_CONDUCT.md"]),
            "https://www.rust-lang.org/policies/code-of-conduct",
        ),
        evidence_check(
            "rust.project.toolchain-policy",
            "A Rust toolchain file is present",
            files(&["rust-toolchain.toml", "rust-toolchain"]),
            "https://rust-lang.github.io/rustup/overrides.html#the-toolchain-file",
        ),
        evidence_check(
            "rust.project.security-policy",
            "Security reporting instructions are present",
            files(&["SECURITY.md", ".github/SECURITY.md"]),
            "https://www.rust-lang.org/policies/security",
        ),
        evidence_check(
            "rust.project.changelog",
            "A changelog is present",
            files(&["CHANGELOG.md", "CHANGES.md"]),
            "https://keepachangelog.com/",
        ),
    ]
}

fn evidence_check(rule_id: &str, title: &str, present: bool, source: &str) -> Value {
    json!({
        "rule_id": rule_id,
        "title": title,
        "category": "static-finding",
        "severity": "warning",
        "status": if present { "passed" } else { "failed" },
        "source": source,
        "evidence": {"present": present},
    })
}

fn skipped_check(rule_id: &str, title: &str, reason: &str, source: &str) -> Value {
    json!({
        "rule_id": rule_id,
        "title": title,
        "category": "verified-gate",
        "severity": "warning",
        "status": "skipped",
        "source": source,
        "evidence": {"reason": reason},
    })
}

fn count(checks: &[Value], status: &str, severity: &str) -> usize {
    checks
        .iter()
        .filter(|check| check["status"] == status && check["severity"] == severity)
        .count()
}
