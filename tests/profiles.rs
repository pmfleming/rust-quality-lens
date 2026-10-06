#![allow(clippy::expect_used, clippy::unwrap_used)]

use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn invoke(config: &Path, complete: bool) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rqlens"));
    command.args(["profiles", "--config"]).arg(config);
    if complete {
        command.arg("--require-complete");
    }
    command.output().expect("run profiles CLI")
}

fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    fs::write(root.path().join("Cargo.toml"), "[package]\nname='profile-fixture'\nversion='0.1.0'\nedition='2024'\nrust-version='1.95'\n[features]\nextra=[]\n").unwrap();
    fs::write(
        root.path().join("src/lib.rs"),
        "pub fn answer() -> u32 { 42 } #[test] fn works() { assert_eq!(answer(), 42); }",
    )
    .unwrap();
    // Synthetic backend counters exercise the report protocol, not an allocator claim.
    fs::write(root.path().join("src/main.rs"), r###"fn main() {
        if let Ok(path) = std::env::var("RQLENS_RESOURCE_REPORT") {
            std::fs::write(path, r##"{"schema_version":1,"metrics":{"allocations":2,"allocated_bytes":64}}"##).unwrap();
        }
    }"###).unwrap();
    root
}

fn report(root: &Path) -> Value {
    serde_json::from_slice(&fs::read(root.join("target/analysis/quality_profiles.json")).unwrap())
        .unwrap()
}

#[test]
fn profiles_execute_real_builds_tests_and_fresh_instrumentation() {
    let root = fixture();
    let config = root.path().join("rqlens.toml");
    let peak = cfg!(target_os = "linux")
        && Command::new("time")
            .arg("--version")
            .output()
            .is_ok_and(|output| {
                output.status.success() && String::from_utf8_lossy(&output.stdout).contains("GNU")
            });
    fs::write(
        &config,
        format!(
            r#"
[rust]
identity_resolution = "disabled"
[[profiles.compatibility]]
id = "active-tests"
operation = "test"
features = ["extra"]
[[profiles.compatibility]]
id = "minimum"
toolchain = "msrv"
no_default_features = true
[[profiles.resources]]
id = "release-build"
kind = "build"
release = true
peak_rss = {peak}
[[profiles.resources]]
id = "instrumented"
kind = "command"
command = ["cargo", "run", "--quiet"]
runs = 2
instrumented_metrics = ["allocations", "allocated_bytes"]
budgets = {{ allocations = 2 }}
"#
        ),
    )
    .unwrap();
    let output = invoke(&config, true);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = report(root.path());
    assert_eq!(report["measurement_confidence"]["complete"], true);
    assert_eq!(
        report["summary"]["compatibility"]["declared_profiles_pass_percent"],
        100.0
    );
    assert_eq!(
        report["summary"]["compatibility"]["compilation_confirmed_profiles"],
        2
    );
    assert_eq!(
        report["summary"]["compatibility"]["test_profiles_passed"],
        1
    );
    let build = &report["data"]["resources"][0]["observations"];
    assert_eq!(build[0]["phase"], "cold-build");
    assert_eq!(build[1]["phase"], "warm-noop-build");
    assert!(build[0]["metrics"]["artifact_bytes"].as_u64().unwrap() > 0);
    assert_eq!(
        build[0]["metrics"]["artifact_bytes"],
        build[1]["metrics"]["artifact_bytes"]
    );
    if peak {
        assert!(build[0]["metrics"]["peak_rss_bytes"].is_u64());
    }
    for observation in report["data"]["resources"][1]["observations"]
        .as_array()
        .unwrap()
    {
        assert_eq!(observation["metrics"]["allocations"], 2);
        assert_eq!(observation["metrics"]["allocated_bytes"], 64);
        assert_eq!(observation["budgets"][0]["status"], "healthy");
    }
    let schema = Command::new(env!("CARGO_BIN_EXE_rqlens"))
        .args(["profiles", "--schema"])
        .output()
        .unwrap();
    assert!(schema.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&schema.stdout).unwrap()["title"],
        "quality_profiles.json"
    );
}

#[test]
fn skipped_and_failed_profiles_write_evidence_without_passing_silently() {
    let root = fixture();
    let config = root.path().join("rqlens.toml");
    fs::write(
        &config,
        "[[profiles.compatibility]]\nid='elsewhere'\nhost_os='another-os'\noperation='test'\n",
    )
    .unwrap();
    assert!(invoke(&config, false).status.success());
    assert!(!invoke(&config, true).status.success());
    let document = report(root.path());
    assert_eq!(document["measurement_confidence"]["complete"], false);
    assert_eq!(
        document["summary"]["compatibility"]["declared_profiles_pass_percent"],
        0.0
    );
    assert_eq!(document["summary"]["compatibility"]["executed_profiles"], 0);
    fs::write(&config, "[[profiles.resources]]\nid='budget'\nkind='command'\ncommand=['cargo','run','--quiet']\ninstrumented_metrics=['allocations']\nbudgets={allocations=1}\n").unwrap();
    assert!(!invoke(&config, false).status.success());
    let document = report(root.path());
    assert_eq!(document["data"]["resources"][0]["status"], "failed");
    assert_eq!(
        document["data"]["resources"][0]["observations"][0]["budgets"][0]["status"],
        "breached"
    );
    assert_eq!(
        document["data"]["resources"][0]["measurement_confidence"]["complete"],
        true
    );
    fs::write(
        &config,
        "[[profiles.compatibility]]\nid='missing'\ntoolchain='9.99.0'\n",
    )
    .unwrap();
    assert!(!invoke(&config, false).status.success());
    assert_eq!(
        report(root.path())["data"]["compatibility"][0]["status"],
        "unavailable"
    );
    fs::write(&config, "[[profiles.compatibility]]\nid='broken-build'\n").unwrap();
    fs::write(
        root.path().join("src/lib.rs"),
        "compile_error!(\"deliberate fixture failure\");",
    )
    .unwrap();
    assert!(!invoke(&config, false).status.success());
    let document = report(root.path());
    assert_eq!(document["data"]["compatibility"][0]["status"], "failed");
    assert_eq!(
        document["data"]["compatibility"][0]["compilation_confirmed"],
        false
    );
    assert_eq!(
        document["data"]["compatibility"][0]["measurement_confidence"]["complete"],
        true
    );
}
