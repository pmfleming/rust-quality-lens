use super::{fixture, measure};
use anyhow::{Context, Result};
use std::{fs, path::Path, process::Command};

fn configure(root: &Path, driver: &Path) -> Result<()> {
    fs::write(
        root.join("rqlens.toml"),
        format!(
            "source_roots=['src']\n[rust]\narchitecture_backend='compiler'\ncompiler_inventory_driver={:?}\nidentity_resolution='required'\nrust_analyzer='/missing-analyzer-not-required-by-compiler'\n[verification]\nall_targets=true\n[[architecture.rules]]\nid='no-denied'\nfrom=['architecture-fixture::shared::isolated']\ndeny=['architecture-fixture::shared::denied']\n",
            driver.to_string_lossy()
        ),
    )?;
    Ok(())
}

fn check(root: &Path) -> Result<bool> {
    Ok(Command::new(env!("CARGO_BIN_EXE_rqlens"))
        .args(["check", "--config"])
        .arg(root.join("rqlens.toml"))
        .output()?
        .status
        .success())
}

#[test]
fn missing_architecture_backend_is_partial_even_with_zero_observed_violations() -> Result<()> {
    let root = fixture()?;
    configure(root.path(), &root.path().join("missing-driver"))?;
    let report = measure(root.path(), "architecture-rules", "architecture_rules.json")?;
    assert_eq!(report["measurement_confidence"]["partial"], true);
    assert_eq!(report["summary"]["violation_count"], 0);
    assert_eq!(report["capabilities"][0]["status"], "partial");
    assert!(!check(root.path())?);
    Ok(())
}

#[cfg(unix)]
#[test]
fn unrelated_producers_do_not_launch_opted_in_architecture_adapter() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let root = fixture()?;
    let marker = root.path().join("executed");
    let driver = root.path().join("driver");
    fs::write(
        &driver,
        format!("#!/bin/sh\ntouch '{}'\nexit 1\n", marker.display()),
    )?;
    fs::set_permissions(&driver, fs::Permissions::from_mode(0o755))?;
    configure(root.path(), &driver)?;
    let report = measure(root.path(), "hotspots", "hotspots.json")?;
    assert_eq!(report["measurement_confidence"]["partial"], true);
    assert!(!marker.exists());
    Ok(())
}

#[test]
#[ignore = "requires pinned nightly rustc-dev adapter; set RQLENS_TEST_COMPILER_DRIVER"]
fn compiler_policy_detects_macro_mutations_and_rejects_partial_or_forged_evidence() -> Result<()> {
    let driver = fs::canonicalize(
        std::env::var_os("RQLENS_TEST_COMPILER_DRIVER").context("compiler driver")?,
    )?;
    let root = fixture()?;
    configure(root.path(), &driver)?;
    let source = root.path().join("src/lib.rs");
    let text = fs::read_to_string(&source)?
        + "\npub mod isolated { macro_rules! boundary { () => { pub fn run() { /* mutation */ } } } boundary!(); }\n";
    fs::write(&source, &text)?;
    let clean = measure(root.path(), "architecture-rules", "architecture_rules.json")?;
    assert_eq!(clean["measurement_confidence"]["complete"], true, "{clean}");
    assert!(
        clean["measurement_confidence"]
            .get("semantic_identity")
            .is_none()
    );
    assert_eq!(clean["summary"]["violation_count"], 0);
    assert!(check(root.path())?);
    let syntax = measure(root.path(), "hotspots", "hotspots.json")?;
    assert_eq!(
        syntax["measurement_confidence"]["partial"], true,
        "architecture must not promote body metrics"
    );

    fs::write(
        &source,
        text.replace("/* mutation */", "crate::denied::call();"),
    )?;
    let mut violated = measure(root.path(), "architecture-rules", "architecture_rules.json")?;
    assert_eq!(violated["measurement_confidence"]["complete"], true);
    assert_eq!(
        violated["summary"]["violation_count"], 2,
        "one violation per normal/test context"
    );
    assert!(!check(root.path())?);
    let rows = violated["data"]["violations"]
        .as_array()
        .context("violations")?;
    assert_ne!(
        rows[0]["compiler_context_id"],
        rows[1]["compiler_context_id"]
    );
    assert!(rows.iter().all(|v| {
        v["reference_origins"]
            .as_array()
            .is_some_and(|origins| origins.iter().any(|origin| origin["generated"] == true))
    }));
    violated["data"]["violations"] = serde_json::json!([]);
    violated["data"]["summary"]["violation_count"] = serde_json::json!(0);
    violated["summary"]["violation_count"] = serde_json::json!(0);
    fs::write(
        root.path().join("target/analysis/architecture_rules.json"),
        serde_json::to_vec(&violated)?,
    )?;
    assert!(
        !check(root.path())?,
        "forged zero must not bypass native graph validation"
    );
    violated["data"]["rules"]["rules"][0]["deny"] = serde_json::json!(["absent::module"]);
    fs::write(
        root.path().join("target/analysis/architecture_rules.json"),
        serde_json::to_vec(&violated)?,
    )?;
    assert!(
        !check(root.path())?,
        "embedded rules must match active configuration"
    );

    let config = root.path().join("rqlens.toml");
    fs::write(
        &config,
        format!(
            "fingerprint_inputs=['missing-generator-input']\n{}",
            fs::read_to_string(&config)?
        ),
    )?;
    let incomplete = measure(root.path(), "architecture-rules", "architecture_rules.json")?;
    assert_eq!(incomplete["measurement_confidence"]["partial"], true);
    assert!(!check(root.path())?);
    configure(root.path(), &driver)?;
    fs::write(
        &source,
        text + "\ncompile_error!(\"capture must fail closed\");\n",
    )?;
    let failed = measure(root.path(), "architecture-rules", "architecture_rules.json")?;
    assert_eq!(failed["measurement_confidence"]["partial"], true);
    assert_eq!(
        failed["data"]["compiler_inventory"]["profiles"],
        serde_json::json!([])
    );
    assert!(!check(root.path())?);
    Ok(())
}

#[test]
#[ignore = "requires pinned nightly rustc-dev adapter; set RQLENS_TEST_COMPILER_DRIVER"]
fn selected_unsupported_targets_are_not_silently_excluded() -> Result<()> {
    let driver = fs::canonicalize(
        std::env::var_os("RQLENS_TEST_COMPILER_DRIVER").context("compiler driver")?,
    )?;
    let root = fixture()?;
    configure(root.path(), &driver)?;
    fs::create_dir(root.path().join("examples"))?;
    fs::write(root.path().join("examples/demo.rs"), "fn main() {}")?;
    let report = measure(root.path(), "architecture-rules", "architecture_rules.json")?;
    assert_eq!(report["measurement_confidence"]["partial"], true);
    assert_eq!(report["summary"]["compilation_context_count"], 0);
    assert!(
        report["measurement_confidence"]["missing_input"]
            .as_array()
            .context("reasons")?
            .iter()
            .any(|r| r.as_str().is_some_and(|s| s.contains("target kinds")))
    );
    assert!(!check(root.path())?);
    Ok(())
}
