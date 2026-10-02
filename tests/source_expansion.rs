use anyhow::{Context, Result};
use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn fixture(source: &str, mode: &str, analyzer: &str) -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir()?;
    fs::create_dir(root.path().join("src"))?;
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='expansion-fixture'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )?;
    fs::write(root.path().join("src/lib.rs"), source)?;
    fs::write(
        root.path().join("rqlens.toml"),
        format!(
            "source_roots=['src']\noutput_dir='analysis'\n[rust]\nidentity_resolution='{mode}'\nrust_analyzer='{analyzer}'\nidentity_timeout_seconds=30\n"
        ),
    )?;
    Ok(root)
}

fn invoke(root: &Path, tool: &str) -> Result<Output> {
    Ok(Command::new(env!("CARGO_BIN_EXE_rqlens"))
        .args(["measure", tool, "--config"])
        .arg(root.join("rqlens.toml"))
        .output()?)
}

fn measure(root: &Path) -> Result<Value> {
    let output = invoke(root, "source-expansion")?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(serde_json::from_slice(&fs::read(
        root.join("analysis/source_expansion.json"),
    )?)?)
}

#[test]
fn missing_or_disabled_backend_does_not_mean_expanded() -> Result<()> {
    for (mode, expected) in [("auto", "unavailable"), ("disabled", "disabled")] {
        let root = fixture(
            "macro_rules! m { () => { struct Generated; } } m!();",
            mode,
            "rqlens-nonexistent-expansion-backend",
        )?;
        let doc = measure(root.path())?;
        assert_eq!(doc["summary"]["observed_sites"], 1);
        assert_eq!(doc["summary"]["expanded_sites"], 0);
        assert_eq!(doc["summary"]["response_complete"], false);
        assert_eq!(doc["data"]["expansions"][0]["status"], expected);
        assert!(doc["data"]["expansions"][0]["expanded_source"].is_null());
        assert_eq!(doc["measurement_confidence"]["complete"], false);
        assert_eq!(doc["capabilities"][0]["status"], "partial");
    }
    Ok(())
}

#[test]
fn required_backend_failure_preserves_previous_output() -> Result<()> {
    let root = fixture(
        "macro_rules! m { () => {} } m!();",
        "required",
        "rqlens-nonexistent-expansion-backend",
    )?;
    fs::create_dir(root.path().join("analysis"))?;
    let path = root.path().join("analysis/source_expansion.json");
    fs::write(&path, "prior generation")?;
    assert!(!invoke(root.path(), "source-expansion")?.status.success());
    assert_eq!(fs::read_to_string(path)?, "prior generation");
    Ok(())
}

#[cfg(unix)]
#[test]
fn disabled_empty_and_unrequested_expansion_never_launch_the_backend() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let probe = tempfile::tempdir()?;
    let marker = probe.path().join("launched");
    let executable = probe.path().join("backend");
    fs::write(
        &executable,
        format!("#!/bin/sh\ntouch '{}'\nexit 1\n", marker.display()),
    )?;
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))?;
    for (source, mode, tool) in [
        (
            "macro_rules! m { () => {} } m!();",
            "disabled",
            "source-expansion",
        ),
        ("pub struct Plain;", "auto", "source-expansion"),
        ("macro_rules! m { () => {} } m!();", "auto", "type-health"),
    ] {
        let root = fixture(source, mode, &executable.to_string_lossy())?;
        assert!(invoke(root.path(), tool)?.status.success());
        assert!(!marker.exists());
    }
    Ok(())
}

fn normalize(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.remove("generated_at");
            object.remove("measured_at");
            object.values_mut().for_each(normalize);
        }
        Value::Array(array) => array.iter_mut().for_each(normalize),
        _ => (),
    }
}

#[test]
#[ignore = "requires live rust-analyzer and Rust sysroot"]
fn live_item_and_impl_expansions_preserve_origins_and_abstention() -> Result<()> {
    let root = fixture(
        "macro_rules! make { () => { pub struct Generated; } }\n/* é😀 */ make!();\nmacro_rules! methods { () => { pub fn answer(&self) -> u8 { 42 } } }\nimpl Generated { methods!(); }\n",
        "auto",
        "rust-analyzer",
    )?;
    assert!(invoke(root.path(), "type-health")?.status.success());
    let types = root.path().join("analysis/type_health.json");
    let original = fs::read(&types)?;
    let mut first = measure(root.path())?;
    assert_eq!(first["summary"]["expanded_sites"], 2, "{first}");
    assert_eq!(first["summary"]["response_complete"], true);
    assert_eq!(first["summary"]["semantic_inventory_complete"], false);
    assert_eq!(first["measurement_confidence"]["complete"], false);
    let entries = first["data"]["expansions"]
        .as_array()
        .context("missing expansions")?;
    assert_eq!(entries[0]["source"]["kind"], "item");
    assert_eq!(entries[1]["source"]["kind"], "impl");
    for (entry, name, kind) in [
        (&entries[0], "Generated", "struct"),
        (&entries[1], "answer", "impl-method"),
    ] {
        assert_eq!(entry["generated_syntax"]["parsed"], true);
        let declaration = &entry["generated_syntax"]["declarations"][0];
        assert_eq!(declaration["name"], name);
        assert_eq!(declaration["kind"], kind);
        let text = entry["expanded_source"].as_str().context("text")?;
        let start = declaration["start_byte"].as_u64().context("start")? as usize;
        let end = declaration["end_byte"].as_u64().context("end")? as usize;
        assert!(text.get(start..end).is_some_and(|body| body.contains(name)));
        assert!(!entry.to_string().contains("__RqlensFragment"));
    }
    assert!(
        entries[0]["expanded_source"]
            .as_str()
            .is_some_and(|text| text.contains("Generated"))
    );
    assert!(
        entries[1]["expanded_source"]
            .as_str()
            .is_some_and(|text| text.contains("answer"))
    );
    assert_eq!(fs::read(types)?, original);
    let mut second = measure(root.path())?;
    normalize(&mut first);
    normalize(&mut second);
    assert_eq!(first, second);
    Ok(())
}
