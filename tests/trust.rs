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
