//! CLI trust regressions: compare evidence without sorting or deleting findings.
#![allow(clippy::expect_used, clippy::unwrap_used)]
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn fixture(source: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    fs::write(root.path().join("src/lib.rs"), source).unwrap();
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = 'trust-fixture'\nversion = '0.0.0'\nedition = '2024'\n[workspace]\n",
    )
    .unwrap();
    fs::write(root.path().join("rqlens.toml"),
        "source_roots = ['src']\noutput_dir = 'analysis'\n[rust]\nidentity_resolution = 'disabled'\n").unwrap();
    root
}

fn invoke(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rqlens"))
        .args(args)
        .args(["--config", root.join("rqlens.toml").to_str().unwrap()])
        .current_dir(root)
        .output()
        .unwrap()
}

fn measure(root: &Path, tool: &str, file: &str) -> Value {
    let output = invoke(root, &["measure", tool]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&fs::read(root.join("analysis").join(file)).unwrap()).unwrap()
}

fn without_timestamps(mut value: Value) -> Value {
    match &mut value {
        Value::Object(fields) => {
            fields.remove("generated_at");
            fields.remove("measured_at");
            for item in fields.values_mut() {
                *item = without_timestamps(item.take());
            }
        }
        Value::Array(items) => {
            for item in items {
                *item = without_timestamps(item.take());
            }
        }
        _ => {}
    }
    value
}

fn type_policy_status(root: &Path) -> Value {
    let _output = invoke(root, &["check", "--fail-on", "partial"]);
    let report: Value =
        serde_json::from_slice(&fs::read(root.join("analysis/policy_report.json")).unwrap())
            .unwrap();
    report["artifact_status"]["type_health.json"]["status"].clone()
}

#[test]
fn aliased_impls_abstain_in_rows_envelopes_and_policy() {
    let root = fixture(
        "pub mod a { pub struct Widget; }\npub mod b { pub struct Widget; }\npub mod implementation { use crate::a::Widget as LocalWidget; impl LocalWidget { pub fn answer(&self) -> u8 { 42 } } }\n#[test] fn behavior() { assert_eq!(a::Widget.answer(), 42); }\n",
    );
    let compiled = Command::new("cargo")
        .args([
            "test",
            "--offline",
            "--manifest-path",
            root.path().join("Cargo.toml").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let document = measure(root.path(), "type-health", "type_health.json");
    assert_eq!(document["measurement_confidence"]["complete"], false);
    assert_eq!(
        document["measurement_confidence"]["type_identity"]["unresolved_impl_blocks"],
        1
    );
    assert_eq!(document["capabilities"][0]["status"], "partial");
    for row in document["records"].as_array().unwrap() {
        assert!(row["method_count"].is_null());
        assert!(row["structural_risk"].is_null());
        assert_eq!(row["observed_method_count"], 0);
        assert_eq!(row["measurement_confidence"]["complete"], false);
    }
    assert_eq!(type_policy_status(root.path()), "partial");
}

#[test]
fn omitted_macro_types_and_impl_only_inputs_are_not_complete_empty_results() {
    for source in [
        "macro_rules! types { () => { pub struct Generated; } } types!();\n",
        "trait Marker {} impl Marker for Vec<()> {}\n",
    ] {
        let root = fixture(source);
        let document = measure(root.path(), "type-health", "type_health.json");
        assert_eq!(document["records"], serde_json::json!([]));
        assert_eq!(document["measurement_confidence"]["complete"], false);
        assert_eq!(document["capabilities"][0]["status"], "partial");
        assert!(
            !document["measurement_confidence"]["unsupported_pattern"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(type_policy_status(root.path()), "partial");
    }
}

#[test]
fn compiling_attribute_generated_types_are_explicitly_unexpanded() {
    let root = fixture(
        "#[generator::emit_type] pub struct Authored;\n#[test] fn generated_exists() { let _ = Generated; }\n",
    );
    fs::create_dir_all(root.path().join("generator/src")).unwrap();
    fs::write(root.path().join("generator/Cargo.toml"), "[package]\nname = 'generator'\nversion = '0.1.0'\nedition = '2024'\n[lib]\nproc-macro = true\n").unwrap();
    fs::write(root.path().join("generator/src/lib.rs"), "#[proc_macro_attribute] pub fn emit_type(_: proc_macro::TokenStream, mut item: proc_macro::TokenStream) -> proc_macro::TokenStream { item.extend(\"pub struct Generated;\".parse::<proc_macro::TokenStream>().unwrap()); item }\n").unwrap();
    let manifest = root.path().join("Cargo.toml");
    let mut content = fs::read_to_string(&manifest).unwrap();
    content.push_str("\n[dependencies]\ngenerator = { path = 'generator' }\n");
    fs::write(&manifest, content).unwrap();
    let compiled = Command::new("cargo")
        .args([
            "test",
            "--offline",
            "--manifest-path",
            manifest.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let document = measure(root.path(), "type-health", "type_health.json");
    assert_eq!(document["measurement_confidence"]["complete"], false);
    assert!(
        document["measurement_confidence"]["unsupported_pattern"]
            .as_array()
            .unwrap()
            .iter()
            .any(|reason| reason
                .as_str()
                .unwrap_or_default()
                .contains("#[generator::emit_type]"))
    );
    assert_eq!(document["records"].as_array().unwrap().len(), 1);
    assert_eq!(document["records"][0]["type_name"], "Authored");
    assert!(document["records"][0]["method_count"].is_null());
    assert_eq!(type_policy_status(root.path()), "partial");
}

#[test]
fn statement_macros_cannot_hide_non_local_impls() {
    let root = fixture(
        "pub struct Widget; macro_rules! make_impl { () => { impl Widget { pub fn answer(&self) -> u8 { 42 } } } } fn install() { make_impl!(); } #[test] fn generated_works() { assert_eq!(Widget.answer(), 42); }\n",
    );
    let compiled = Command::new("cargo")
        .args([
            "test",
            "--offline",
            "--manifest-path",
            root.path().join("Cargo.toml").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let document = measure(root.path(), "type-health", "type_health.json");
    assert_eq!(document["measurement_confidence"]["complete"], false);
    assert!(
        document["measurement_confidence"]["unsupported_pattern"]
            .as_array()
            .unwrap()
            .iter()
            .any(|reason| reason
                .as_str()
                .unwrap_or_default()
                .contains("unexpanded macro make_impl!"))
    );
    assert_eq!(document["records"][0]["observed_method_count"], 0);
    assert!(document["records"][0]["method_count"].is_null());
}

#[test]
fn known_zero_and_explicit_owners_remain_complete() {
    let root = fixture(
        "pub struct Plain; pub struct Root; impl crate::Root { pub fn root(&self) {} }\npub mod other { pub struct Widget; impl self::Widget { pub fn one(&self) {} } pub mod child { impl super::Widget { pub fn two(&self) {} } } }\n",
    );
    let document = measure(root.path(), "type-health", "type_health.json");
    assert_eq!(document["measurement_confidence"]["complete"], true);
    for (name, expected) in [("Plain", 0), ("Root", 1), ("Widget", 2)] {
        let row = document["records"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["type_name"] == name)
            .unwrap();
        assert_eq!(row["method_count"], expected);
        assert!(row["structural_risk"].is_number());
    }
    assert_eq!(type_policy_status(root.path()), "available");
}

#[test]
fn suffix_collisions_conditional_duplicates_and_reference_impls_abstain() {
    for source in [
        "pub mod local { pub struct Widget; } mod elsewhere { use std::collections::HashMap as Widget; trait Marker { fn one(&self); } impl Marker for Widget<u8, u8> { fn one(&self) {} } }\n",
        "#[cfg(unix)] pub struct Widget; #[cfg(not(unix))] pub struct Widget; impl Widget { pub fn one(&self) {} }\n",
        "#[cfg(unix)] pub struct Widget;\n",
        "#[derive(Debug)] pub struct Widget;\n",
        "pub struct Widget; trait Marker {} impl Marker for &Widget {}\n",
        "pub struct Widget; macro_rules! method { () => { pub fn generated(&self) {} } } impl Widget { method!(); }\n",
        "pub struct Widget; trait Marker {} fn local() { use std::collections::HashMap as Widget; impl Marker for Widget<u8,u8> {} }\n",
    ] {
        let root = fixture(source);
        let document = measure(root.path(), "type-health", "type_health.json");
        assert_eq!(document["measurement_confidence"]["complete"], false);
        for row in document["records"].as_array().unwrap() {
            assert!(row["method_count"].is_null());
            assert!(row["structural_risk"].is_null());
            assert_eq!(row["observed_method_count"], 0);
        }
    }
}

#[test]
fn requested_but_disabled_or_missing_semantics_are_partial() {
    let root = fixture("pub mod other; pub fn call() { other::go(); }\n");
    fs::write(root.path().join("src/other.rs"), "pub fn go() {}\n").unwrap();
    for mode in ["disabled", "auto"] {
        fs::write(root.path().join("rqlens.toml"), format!("source_roots = ['src']\noutput_dir = 'analysis'\n[rust]\nidentity_resolution = '{mode}'\nrust_analyzer = '/rqlens-missing-test-analyzer'\n")).unwrap();
        let document = measure(root.path(), "locality", "locality_metrics.json");
        assert_eq!(document["measurement_confidence"]["complete"], false);
        assert_eq!(
            document["measurement_confidence"]["semantic_identity"]["complete"],
            false
        );
        assert_eq!(document["capabilities"][0]["status"], "partial");
        assert!(
            document["measurement_confidence"]["missing_input"]
                .as_array()
                .unwrap()
                .iter()
                .any(|reason| reason.as_str().unwrap_or_default().contains("semantic"))
        );
    }
}

#[test]
fn virtual_workspace_correctness_is_repeatable_without_a_root_package() {
    let root = fixture("");
    fs::create_dir_all(root.path().join("member/src")).unwrap();
    fs::write(
        root.path().join("Cargo.toml"),
        "[workspace]\nmembers = ['member']\nresolver = '3'\n",
    )
    .unwrap();
    fs::write(
        root.path().join("member/Cargo.toml"),
        "[package]\nname = 'member'\nversion = '0.0.0'\nedition = '2024'\n",
    )
    .unwrap();
    fs::write(
        root.path().join("member/src/lib.rs"),
        "#[test] fn one() { assert_eq!(1, 1); }\n",
    )
    .unwrap();
    fs::write(
        root.path().join("rqlens.toml"),
        "source_roots = ['member/src']\noutput_dir = 'analysis'\n",
    )
    .unwrap();
    let first = without_timestamps(measure(
        root.path(),
        "correctness",
        "correctness_review.json",
    ));
    assert_eq!(first["data"]["summary"]["test_count"], 1);
    assert_eq!(
        first,
        without_timestamps(measure(
            root.path(),
            "correctness",
            "correctness_review.json"
        ))
    );
}

#[test]
fn repeated_map_preserves_edge_order_and_evidence() {
    let declarations = (0..12)
        .map(|i| format!("pub mod m{i};\n"))
        .collect::<String>();
    let calls = (0..12)
        .map(|i| format!("m{i}::value();\n"))
        .collect::<String>();
    let root = fixture(&format!("{declarations}pub fn call() {{ {calls} }}\n"));
    for i in 0..12 {
        fs::write(
            root.path().join(format!("src/m{i}.rs")),
            "pub fn value() -> u8 { 1 }\n",
        )
        .unwrap();
    }
    let first = without_timestamps(measure(root.path(), "map", "map.json"));
    let edges = first["data"]["graph"]["edges"].as_array().unwrap();
    for kind in ["dependency", "containment"] {
        assert!(
            edges
                .iter()
                .filter(|edge| edge["data"]["kind"] == kind)
                .count()
                >= 12,
            "fixture must exercise unordered {kind} adjacency sets"
        );
    }
    for _ in 0..3 {
        assert_eq!(
            first,
            without_timestamps(measure(root.path(), "map", "map.json"))
        );
    }
    fs::write(
        root.path().join("src/m0.rs"),
        "pub fn value() -> u8 { 2 }\npub struct Added;\n",
    )
    .unwrap();
    let changed = without_timestamps(measure(root.path(), "map", "map.json"));
    assert_ne!(first["input_fingerprint"], changed["input_fingerprint"]);
    assert_ne!(
        first["data"]["graph"]["nodes"],
        changed["data"]["graph"]["nodes"]
    );
}
