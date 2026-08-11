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
    let mut checks = official_checks(config);
    checks.extend([
        audit_check(config),
        deny_check(config),
        semver_check(config),
        feature_matrix_check(config),
        mutation_check(config),
        flaky_test_check(config),
        miri_check(config),
    ]);
    checks.extend(fuzz_checks(config));
    checks.extend(sanitizer_checks(config));
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

fn official_checks(config: &LensConfig) -> Vec<Value> {
    let mut clippy_args = scoped_args(config, "clippy", true, false);
    clippy_args.extend(["--".to_string(), "-D".to_string(), "warnings".to_string()]);
    vec![
        cargo_check(
            config,
            "rust.official.rustfmt",
            "Formatting matches rustfmt",
            ["fmt", "--all", "--", "--check"]
                .map(str::to_string)
                .to_vec(),
            "https://github.com/rust-lang/rustfmt",
            BTreeMap::new(),
        ),
        cargo_check(
            config,
            "rust.official.cargo-check",
            "All configured targets compile",
            scoped_args(config, "check", true, false),
            "https://doc.rust-lang.org/cargo/commands/cargo-check.html",
            BTreeMap::new(),
        ),
        cargo_check(
            config,
            "rust.official.clippy",
            "Clippy reports no warnings",
            clippy_args,
            "https://doc.rust-lang.org/clippy/",
            BTreeMap::new(),
        ),
        cargo_check(
            config,
            "rust.official.tests",
            "Cargo tests pass for all configured targets",
            scoped_args(config, "test", true, false),
            "https://doc.rust-lang.org/cargo/commands/cargo-test.html",
            BTreeMap::new(),
        ),
        cargo_check(
            config,
            "rust.official.doctests",
            "Rust documentation tests pass",
            scoped_args(config, "test", false, true),
            "https://doc.rust-lang.org/rustdoc/write-documentation/documentation-tests.html",
            BTreeMap::new(),
        ),
        cargo_check(
            config,
            "rust.official.rustdoc",
            "Rust documentation builds without warnings",
            scoped_args(config, "doc", false, false),
            "https://doc.rust-lang.org/rustdoc/",
            BTreeMap::from([("RUSTDOCFLAGS".to_string(), "-D warnings".to_string())]),
        ),
    ]
}

fn audit_check(config: &LensConfig) -> Value {
    if config.verification.audit {
        cargo_check(
            config,
            "rust.security.rustsec",
            "Dependencies have no known RustSec advisories",
            vec!["audit".to_string()],
            "https://rustsec.org/",
            BTreeMap::new(),
        )
    } else {
        skipped_check(
            "rust.security.rustsec",
            "Dependency advisory scan",
            "enable verification.audit to require cargo-audit",
            "https://rustsec.org/",
        )
    }
}

fn deny_check(config: &LensConfig) -> Value {
    if config.verification.deny {
        cargo_check(
            config,
            "rust.supply-chain.cargo-deny",
            "Dependency policy passes cargo-deny",
            vec!["deny".to_string(), "check".to_string()],
            "https://embarkstudios.github.io/cargo-deny/",
            BTreeMap::new(),
        )
    } else {
        skipped_check(
            "rust.supply-chain.cargo-deny",
            "Dependency license and source policy",
            "enable verification.deny to require cargo-deny",
            "https://embarkstudios.github.io/cargo-deny/",
        )
    }
}

fn semver_check(config: &LensConfig) -> Value {
    if !config.verification.semver {
        return skipped_check(
            "rust.api.semver",
            "Public API compatibility",
            "enable verification.semver for release-oriented libraries",
            "https://github.com/obi1kenobi/cargo-semver-checks",
        );
    }
    let mut arguments = vec!["semver-checks".to_string(), "check-release".to_string()];
    if let Some(revision) = &config.verification.semver_baseline_rev {
        arguments.extend(["--baseline-rev".to_string(), revision.clone()]);
    }
    cargo_check(
        config,
        "rust.api.semver",
        "Public API is compatible with the configured baseline",
        arguments,
        "https://github.com/obi1kenobi/cargo-semver-checks",
        BTreeMap::new(),
    )
}

fn feature_matrix_check(config: &LensConfig) -> Value {
    if !config.verification.feature_matrix {
        return skipped_check(
            "rust.features.matrix",
            "Cargo feature compatibility matrix",
            "enable verification.feature_matrix to require cargo-hack",
            "https://github.com/taiki-e/cargo-hack",
        );
    }
    let mut arguments = vec![
        "hack".to_string(),
        "check".to_string(),
        "--feature-powerset".to_string(),
    ];
    if config.verification.workspace {
        arguments.push("--workspace".to_string());
    }
    if config.verification.all_targets {
        arguments.push("--all-targets".to_string());
    }
    if config.verification.locked {
        arguments.push("--locked".to_string());
    }
    if config.verification.workspace {
        for package in &config.verification.exclude {
            arguments.extend(["--exclude".to_string(), package.clone()]);
        }
    }
    cargo_check(
        config,
        "rust.features.matrix",
        "Configured Cargo feature combinations compile",
        arguments,
        "https://github.com/taiki-e/cargo-hack",
        BTreeMap::new(),
    )
}

fn mutation_check(config: &LensConfig) -> Value {
    if !config.verification.mutation {
        return skipped_check(
            "rust.tests.mutation",
            "Tests reject generated code mutations",
            "enable verification.mutation to require cargo-mutants",
            "https://mutants.rs/",
        );
    }
    let mut arguments = vec!["mutants".to_string(), "--no-times".to_string()];
    if config.verification.workspace {
        arguments.push("--workspace".to_string());
        for package in &config.verification.exclude {
            arguments.extend(["--exclude".to_string(), package.clone()]);
        }
    }
    if config.verification.all_features {
        arguments.push("--all-features".to_string());
    } else {
        if config.verification.no_default_features {
            arguments.push("--no-default-features".to_string());
        }
        if !config.verification.features.is_empty() {
            arguments.extend([
                "--features".to_string(),
                config.verification.features.join(","),
            ]);
        }
    }
    cargo_check(
        config,
        "rust.tests.mutation",
        "Tests reject generated code mutations",
        arguments,
        "https://mutants.rs/",
        BTreeMap::new(),
    )
}

fn flaky_test_check(config: &LensConfig) -> Value {
    let runs = config.verification.flaky_test_runs;
    if runs <= 1 {
        return skipped_check(
            "rust.tests.flaky-repeat",
            "Repeated test runs are stable",
            "set verification.flaky_test_runs above 1 to detect intermittent failures",
            "https://doc.rust-lang.org/cargo/commands/cargo-test.html",
        );
    }
    let arguments = config.verification.cargo_arguments("test", true, false);
    let mut outcomes = Vec::new();
    for _ in 0..runs {
        let mut request = CommandRequest::new("cargo", &arguments, &config.project_root);
        request.timeout = Duration::from_secs(config.verification.timeout_seconds);
        outcomes.push(run(request));
    }
    let passed = outcomes
        .iter()
        .filter(|outcome| outcome.status == CommandStatus::Passed)
        .count();
    let unavailable = outcomes.iter().any(|outcome| {
        outcome.status == CommandStatus::Unavailable
            || (outcome.status == CommandStatus::Failed
                && cargo_tool_is_unavailable(&outcome.stderr))
    });
    let timed_out = outcomes
        .iter()
        .any(|outcome| outcome.status == CommandStatus::TimedOut);
    let status = if unavailable {
        "unavailable"
    } else if timed_out {
        "timed-out"
    } else if passed == runs {
        "passed"
    } else {
        "failed"
    };
    json!({
        "rule_id": "rust.tests.flaky-repeat",
        "title": "Repeated test runs are stable",
        "category": "verified-gate",
        "severity": "error",
        "status": status,
        "source": "https://doc.rust-lang.org/cargo/commands/cargo-test.html",
        "tool": "cargo",
        "tool_version": cargo_version(),
        "evidence": {
            "requested_runs": runs,
            "passed_runs": passed,
            "failed_runs": runs.saturating_sub(passed),
            "inconsistent": passed > 0 && passed < runs,
            "runs": outcomes,
        },
    })
}

fn fuzz_checks(config: &LensConfig) -> Vec<Value> {
    if config.verification.fuzz_targets.is_empty() {
        return vec![skipped_check(
            "rust.safety.fuzz",
            "Configured fuzz targets execute without crashes",
            "configure verification.fuzz_targets to require cargo-fuzz",
            "https://rust-fuzz.github.io/book/cargo-fuzz.html",
        )];
    }
    config
        .verification
        .fuzz_targets
        .iter()
        .map(|target| {
            cargo_check(
                config,
                "rust.safety.fuzz",
                &format!("Fuzz target {target} executes without crashes"),
                vec![
                    "fuzz".to_string(),
                    "run".to_string(),
                    target.clone(),
                    "--".to_string(),
                    format!("-max_total_time={}", config.verification.fuzz_seconds),
                ],
                "https://rust-fuzz.github.io/book/cargo-fuzz.html",
                BTreeMap::new(),
            )
        })
        .collect()
}

fn sanitizer_checks(config: &LensConfig) -> Vec<Value> {
    if config.verification.sanitizers.is_empty() {
        return vec![skipped_check(
            "rust.safety.sanitizers",
            "Tests pass under configured compiler sanitizers",
            "configure verification.sanitizers on a supported nightly toolchain",
            "https://doc.rust-lang.org/beta/unstable-book/compiler-flags/sanitizer.html",
        )];
    }
    config
        .verification
        .sanitizers
        .iter()
        .map(|sanitizer| {
            cargo_check(
                config,
                &format!("rust.safety.sanitizer.{sanitizer}"),
                &format!("Tests pass under the {sanitizer} sanitizer"),
                config.verification.cargo_arguments("test", true, false),
                "https://doc.rust-lang.org/beta/unstable-book/compiler-flags/sanitizer.html",
                BTreeMap::from([("RUSTFLAGS".to_string(), format!("-Zsanitizer={sanitizer}"))]),
            )
        })
        .collect()
}

fn miri_check(config: &LensConfig) -> Value {
    if !config.verification.miri {
        return skipped_check(
            "rust.safety.miri",
            "Miri undefined-behavior checks",
            "enable verification.miri on a toolchain with the Miri component",
            "https://github.com/rust-lang/miri",
        );
    }
    let mut arguments = vec!["miri".to_string()];
    arguments.extend(config.verification.cargo_arguments("test", true, false));
    cargo_check(
        config,
        "rust.safety.miri",
        "Miri finds no undefined behavior in executed tests",
        arguments,
        "https://github.com/rust-lang/miri",
        BTreeMap::new(),
    )
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
    let mut outcome = run(request);
    if outcome.status == CommandStatus::Failed && cargo_tool_is_unavailable(&outcome.stderr) {
        outcome.status = CommandStatus::Unavailable;
        outcome.reason =
            Some("the requested Cargo command or toolchain component is unavailable".to_string());
    }
    command_check(rule_id, title, source, outcome)
}

fn cargo_tool_is_unavailable(stderr: &str) -> bool {
    let stderr = stderr.to_ascii_lowercase();
    [
        "no such command:",
        "is not installed for the toolchain",
        "component is unavailable",
        "component 'rustfmt' is unavailable",
        "component 'clippy' is unavailable",
        "toolchain is not installed",
        "unknown proxy name",
    ]
    .iter()
    .any(|message| stderr.contains(message))
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
    let mut evidence = vec![
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
    ];
    evidence.insert(1, msrv_dependency_check(config, manifest.as_ref(), package));
    evidence
}

fn msrv_dependency_check(
    config: &LensConfig,
    manifest: Option<&toml::Value>,
    package: Option<&toml::Value>,
) -> Value {
    let declared = package
        .and_then(|package| package.get("rust-version"))
        .and_then(toml::Value::as_str)
        .or_else(|| {
            manifest
                .and_then(|manifest| manifest.get("workspace"))
                .and_then(|workspace| workspace.get("package"))
                .and_then(|package| package.get("rust-version"))
                .and_then(toml::Value::as_str)
        });
    let Some(declared) = declared else {
        return skipped_check(
            "rust.project.msrv-compatible",
            "Selected dependencies support the declared Rust version",
            "rust-version is not declared",
            "https://doc.rust-lang.org/cargo/reference/rust-version.html",
        );
    };
    let arguments = ["metadata", "--format-version", "1", "--offline"]
        .map(str::to_string)
        .to_vec();
    let mut request = CommandRequest::new("cargo", &arguments, &config.project_root);
    request.timeout = Duration::from_secs(config.verification.timeout_seconds);
    let outcome = run(request);
    if outcome.status != CommandStatus::Passed {
        return json!({
            "rule_id": "rust.project.msrv-compatible",
            "title": "Selected dependencies support the declared Rust version",
            "category": "verified-gate",
            "severity": "warning",
            "status": "unavailable",
            "source": "https://doc.rust-lang.org/cargo/reference/rust-version.html",
            "evidence": outcome,
        });
    }
    let metadata = serde_json::from_str::<Value>(&outcome.stdout).unwrap_or_else(|_| json!({}));
    let maximum = metadata["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|package| {
            Some((
                parse_rust_version(package["rust_version"].as_str()?)?,
                package["name"].as_str()?.to_string(),
            ))
        })
        .max_by_key(|(version, _)| *version);
    let declared_version = parse_rust_version(declared);
    let compatible = declared_version
        .zip(maximum.as_ref().map(|(version, _)| *version))
        .is_some_and(|(declared, required)| declared >= required);
    json!({
        "rule_id": "rust.project.msrv-compatible",
        "title": "Selected dependencies support the declared Rust version",
        "category": "static-finding",
        "severity": "warning",
        "status": if compatible { "passed" } else { "failed" },
        "source": "https://doc.rust-lang.org/cargo/reference/rust-version.html",
        "evidence": {
            "declared_rust_version": declared,
            "maximum_selected_dependency_rust_version": maximum.as_ref().map(|(version, _)| format_rust_version(*version)),
            "maximum_dependency": maximum.map(|(_, package)| package),
        },
    })
}

fn parse_rust_version(value: &str) -> Option<(u64, u64, u64)> {
    let mut parts = value.split('.').map(|part| part.parse::<u64>().ok());
    Some((
        parts.next()??,
        parts.next().flatten().unwrap_or(0),
        parts.next().flatten().unwrap_or(0),
    ))
}

fn format_rust_version(version: (u64, u64, u64)) -> String {
    format!("{}.{}.{}", version.0, version.1, version.2)
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

#[cfg(test)]
mod tests {
    use super::{cargo_tool_is_unavailable, format_rust_version, parse_rust_version};

    #[test]
    fn rust_versions_compare_as_numeric_components() {
        assert!(parse_rust_version("1.100") > parse_rust_version("1.99.9"));
        assert_eq!(parse_rust_version("1.95"), Some((1, 95, 0)));
        assert_eq!(format_rust_version((1, 95, 0)), "1.95.0");
        assert_eq!(parse_rust_version("stable"), None);
    }

    #[test]
    fn missing_cargo_components_are_not_reported_as_code_failures() {
        assert!(cargo_tool_is_unavailable("error: no such command: `audit`"));
        assert!(cargo_tool_is_unavailable(
            "error: component 'rustfmt' is unavailable for download"
        ));
        assert!(!cargo_tool_is_unavailable(
            "error: could not compile `demo` due to 1 previous error"
        ));
    }
}
