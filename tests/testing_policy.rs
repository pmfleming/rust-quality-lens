//! Primary public-workflow owner: real CLI, manifest, Cargo execution and artifacts.
use anyhow::{Context, Result};
use serde_json::Value;
use std::{fs, path::Path, process::Command};

fn invoke(root: &Path, args: &[&str]) -> Result<std::process::Output> {
    let output = Command::new(env!("CARGO_BIN_EXE_rqlens"))
        .args(args)
        .arg("--config")
        .arg(root.join("rqlens.toml"))
        .current_dir(root)
        .output()?;
    Ok(output)
}

fn run(root: &Path, args: &[&str]) -> Result<()> {
    let output = invoke(root, args)?;
    anyhow::ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn document(root: &Path, file: &str) -> Result<Value> {
    Ok(serde_json::from_str(&fs::read_to_string(
        root.join("target/analysis").join(file),
    )?)?)
}

fn setup(root: &Path) -> Result<()> {
    fs::create_dir_all(root.join("src"))?;
    fs::create_dir_all(root.join("tests"))?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = 'ownership_fixture'\nversion = '0.1.0'\nedition = '2024'\n",
    )?;
    fs::write(root.join("src/lib.rs"), "pub fn value() -> u8 { 42 }\n")?;
    fs::write(
        root.join("tests/journey.rs"),
        "#[test]\nfn journey() { assert_eq!(ownership_fixture::value(), 42); }\n",
    )?;
    fs::write(
        root.join("rqlens.toml"),
        "[rust]\nidentity_resolution = 'disabled'\n[testing_policy]\nmanifest = 'testing.toml'\n",
    )?;
    fs::write(root.join("testing.toml"), manifest())?;
    Ok(())
}

fn manifest() -> &'static str {
    r#"version = 1
[[behaviors]]
id = "answer"
description = "External answer remains correct"
risks = ["public_contract"]
requirement = "fixture public answer contract"
scope = "default Cargo host tests"
expectations = ["answer is 42"]
real_boundaries = ["public library implementation"]
reviewer = "fixture-maintainer"
review_revision = "fixture-v1"
required = true
[behaviors.owner]
role = "integration"
rationale = "Real public call, no replaced implementation"
tests = [{package = "ownership_fixture", target = "journey", path = "tests/journey.rs", name = "journey"}]
"#
}

#[test]
fn ownership_review_uses_real_execution_and_rejects_stale_claims() -> Result<()> {
    let root = tempfile::tempdir()?;
    setup(root.path())?;
    run(root.path(), &["measure", "test-quality"])?;
    let before = document(root.path(), "test_quality.json")?;
    assert_eq!(
        before["data"]["testing_policy"]["behaviors"][0]["status"],
        "unknown"
    );
    run(root.path(), &["measure", "correctness-run"])?;
    run(root.path(), &["measure", "test-quality"])?;
    let after = document(root.path(), "test_quality.json")?;
    let policy = &after["data"]["testing_policy"];
    assert_eq!(
        policy["behaviors"][0]["status"], "observed_pass",
        "{policy:#}"
    );
    assert_eq!(policy["execution_evidence"], "passed");
    assert!(
        after["data"]["findings"]
            .as_array()
            .context("findings")?
            .is_empty()
    );

    // Non-Rust policy input changes invalidate execution; no inferred re-pass.
    fs::write(
        root.path().join("testing.toml"),
        format!("{}\n# reviewed anew\n", manifest()),
    )?;
    run(root.path(), &["measure", "test-quality"])?;
    let stale = document(root.path(), "test_quality.json")?;
    assert_ne!(
        after["input_fingerprint"]["digest"],
        stale["input_fingerprint"]["digest"]
    );
    assert_eq!(
        stale["data"]["testing_policy"]["execution_evidence"],
        "stale"
    );
    assert_eq!(
        stale["data"]["testing_policy"]["behaviors"][0]["status"],
        "unknown"
    );

    // Exact target selection: the same test name cannot satisfy another target.
    fs::write(
        root.path().join("testing.toml"),
        manifest().replace("target = \"journey\"", "target = \"other\""),
    )?;
    run(root.path(), &["measure", "test-quality"])?;
    assert_eq!(
        document(root.path(), "test_quality.json")?["data"]["testing_policy"]["behaviors"][0]["status"],
        "unresolved"
    );

    fs::write(
        root.path().join("testing.toml"),
        "version = 999\nbehaviors = []\n",
    )?;
    run(root.path(), &["measure", "test-quality"])?;
    let invalid = document(root.path(), "test_quality.json")?;
    assert_eq!(invalid["data"]["testing_policy"]["status"], "unavailable");
    assert_eq!(invalid["measurement_confidence"]["complete"], false);
    Ok(())
}
