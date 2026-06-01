use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn rqlens() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_rqlens"))
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(rqlens())
        .args(args)
        .current_dir(repo_root())
        .output()
        .expect("rqlens command should run")
}

fn read_json(path: impl AsRef<Path>) -> Value {
    serde_json::from_str(&fs::read_to_string(path).expect("json file should exist"))
        .expect("json should parse")
}

#[test]
fn catalog_lists_board_compatible_tasks() {
    let output = run(&[
        "catalog",
        "--config",
        "tests/fixtures/mini_rust_project/rqlens.toml",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["project_name"], "mini-rust-project");
    let ids: Vec<&str> = payload["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|task| task["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"quality.escape_hatches"));
    assert!(ids.contains(&"quality.type_health"));
    assert!(ids.contains(&"correctness.catalog"));
    assert!(ids.contains(&"map.architecture"));
}

#[test]
fn type_health_keeps_same_named_types_separate() {
    let output = run(&[
        "measure",
        "type-health",
        "--config",
        "tests/fixtures/golden_rust_project/rqlens.toml",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload = read_json(
        repo_root().join("tests/fixtures/golden_rust_project/target/analysis/type_health.json"),
    );
    let rows = payload.as_array().unwrap();
    let alpha = rows
        .iter()
        .find(|item| item["qualified_name"] == "alpha::Config")
        .unwrap();
    let beta = rows
        .iter()
        .find(|item| item["qualified_name"] == "beta::Config")
        .unwrap();
    assert_eq!(alpha["method_count"], 1);
    assert_eq!(beta["method_count"], 2);
    assert!(rows.iter().any(|item| item["qualified_name"] == "domain::TupleConfig"
        && item["shape"] == "tuple"));
}

#[test]
fn map_captures_direct_module_path_dependency() {
    let output = run(&[
        "measure",
        "map",
        "--config",
        "tests/fixtures/mini_rust_project/rqlens.toml",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload =
        read_json(repo_root().join("tests/fixtures/mini_rust_project/target/analysis/map.json"));
    assert_eq!(payload["meta"]["project_name"], "mini-rust-project");
    assert_eq!(payload["meta"]["risk_model_id"], "rqlens.architecture_risk");
    assert_eq!(
        payload["meta"]["layer_ruleset"]["id"],
        "rqlens.generic_layers"
    );
    let edges = payload["graph"]["edges"].as_array().unwrap();
    assert!(
        edges
            .iter()
            .any(|edge| { edge["data"]["source"] == "lib" && edge["data"]["target"] == "math" })
    );
}

#[test]
fn correctness_writes_review_and_test_catalog() {
    let output = run(&[
        "measure",
        "correctness",
        "--config",
        "tests/fixtures/mini_rust_project/rqlens.toml",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let root = repo_root().join("tests/fixtures/mini_rust_project/target/analysis");
    let review = read_json(root.join("correctness_review.json"));
    let catalog = read_json(root.join("test_catalog.json"));
    assert_eq!(review["summary"]["test_count"], 0);
    assert!(catalog.as_array().unwrap().is_empty());
}
