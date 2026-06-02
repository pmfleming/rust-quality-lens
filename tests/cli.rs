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

fn write_clone_fixture(name: &str) -> PathBuf {
    let root = repo_root().join("target").join("test-fixtures").join(name);
    if root.exists() {
        fs::remove_dir_all(&root).expect("old test fixture should be removable");
    }
    fs::create_dir_all(root.join("src")).expect("test fixture src should be created");
    fs::write(
        root.join("rqlens.toml"),
        r#"project_name = "clone-confidence"
project_root = "."
source_roots = ["src"]
output_dir = "target/analysis"
"#,
    )
    .expect("test config should be written");
    fs::write(
        root.join("src").join("lib.rs"),
        r#"
pub fn repeated_one(input: usize) -> usize {
    let first = input + 1;
    let second = first + 2;
    let third = second + 3;
    let fourth = third + 4;
    if fourth > 10 {
        fourth
    } else {
        input
    }
}

pub fn repeated_two(input: usize) -> usize {
    let first = input + 1;
    let second = first + 2;
    let third = second + 3;
    let fourth = third + 4;
    if fourth > 10 {
        fourth
    } else {
        input
    }
}
"#,
    )
    .expect("test source should be written");
    root
}

fn write_architecture_fixture(name: &str, artifacts: bool) -> PathBuf {
    let root = repo_root().join("target").join("test-fixtures").join(name);
    if root.exists() {
        fs::remove_dir_all(&root).expect("old test fixture should be removable");
    }
    fs::create_dir_all(root.join("src")).expect("test fixture src should be created");
    fs::write(
        root.join("rqlens.toml"),
        format!(
            r#"project_name = "{name}"
project_root = "."
source_roots = ["src"]
output_dir = "target/analysis"
"#
        ),
    )
    .expect("test config should be written");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"arch-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("cargo manifest should be written");
    fs::write(
        root.join("src").join("lib.rs"),
        "pub mod domain;\npub mod service;\n",
    )
    .expect("lib source should be written");
    fs::write(
        root.join("src").join("domain.rs"),
        "use crate::service::run;\npub fn value() -> i32 { run() }\n",
    )
    .expect("domain source should be written");
    fs::write(
        root.join("src").join("service.rs"),
        "use crate::domain::value;\npub fn run() -> i32 { value() }\n",
    )
    .expect("service source should be written");
    if artifacts {
        std::thread::sleep(std::time::Duration::from_millis(20));
        let analysis = root.join("target").join("analysis");
        fs::create_dir_all(&analysis).expect("analysis dir should be created");
        fs::write(
            analysis.join("hotspots.json"),
            serde_json::to_string(&serde_json::json!([
                {"module_key": "lib", "score": 1.0},
                {"module_key": "domain", "score": 10.0},
                {"module_key": "service", "score": 20.0}
            ]))
            .unwrap(),
        )
        .expect("hotspots should be written");
        fs::write(
            analysis.join("correctness_review.json"),
            serde_json::to_string(&serde_json::json!({
                "tests": [
                    {"module": "domain", "last_status": "failed"},
                    {"module": "service", "last_status": "passed"}
                ]
            }))
            .unwrap(),
        )
        .expect("correctness should be written");
        fs::write(analysis.join("locality_metrics.json"), "[]")
            .expect("locality should be written");
        fs::write(analysis.join("leverage_metrics.json"), "[]")
            .expect("leverage should be written");
        fs::write(
            analysis.join("slowspots.json"),
            serde_json::to_string(&serde_json::json!([
                {"module_key": "service", "benchmark_score": 5.0, "mean_ms": 2.0, "variance": 0.5}
            ]))
            .unwrap(),
        )
        .expect("slowspots should be written");
    }
    root
}

fn node_data<'a>(payload: &'a Value, id: &str) -> &'a Value {
    payload["graph"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| &node["data"])
        .find(|data| data["id"] == id)
        .expect("node should exist")
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
fn map_reports_missing_artifacts_as_unknown_metrics() {
    let fixture = write_architecture_fixture("arch-missing-artifacts", false);
    let config = fixture.join("rqlens.toml").to_string_lossy().to_string();
    let output = run(&["measure", "map", "--config", &config]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload = read_json(fixture.join("target/analysis/map.json"));
    assert_eq!(
        payload["meta"]["summary"]["artifact_status"]["hotspots.json"]["status"],
        "missing"
    );
    assert_eq!(
        payload["meta"]["summary"]["artifact_status"]["slowspots.json"]["status"],
        "missing"
    );
    assert_eq!(payload["measurement_confidence"]["partial"], true);
    let service = node_data(&payload, "service");
    assert!(service["performance_risk"].is_null());
    assert!(service["total_score"].is_null());
    assert!(
        service["unknown_metrics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|metric| metric == "performance_risk")
    );
}

#[test]
fn map_scores_git_benchmarks_cycles_and_layer_violations() {
    let fixture = write_architecture_fixture("arch-complete-artifacts", true);
    let config = fixture.join("rqlens.toml").to_string_lossy().to_string();
    let output = run(&["measure", "map", "--config", &config]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload = read_json(fixture.join("target/analysis/map.json"));
    assert_eq!(
        payload["meta"]["summary"]["artifact_status"]["hotspots.json"]["status"],
        "available"
    );
    assert_eq!(
        payload["meta"]["summary"]["artifact_status"]["slowspots.json"]["status"],
        "available"
    );
    let domain = node_data(&payload, "domain");
    assert_eq!(domain["layer_violation_count"], 1);
    assert_eq!(domain["cycle_member"], true);
    assert!(domain["architectural_risk"].as_f64().unwrap() >= 140.0);
    assert!(domain["total_score"].as_f64().is_some());
    assert!(
        domain["raw_facts"]["git_history"]["commit_count"]
            .as_u64()
            .is_some()
    );
    let service = node_data(&payload, "service");
    assert_eq!(service["performance_risk"], 100.0);
    assert!(service["total_score"].as_f64().is_some());
    assert!(service["unknown_metrics"].as_array().unwrap().is_empty());
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

#[test]
fn hotspots_report_syntax_fact_confidence() {
    let output = run(&[
        "measure",
        "hotspots",
        "--config",
        "tests/fixtures/mini_rust_project/rqlens.toml",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload = read_json(
        repo_root().join("tests/fixtures/mini_rust_project/target/analysis/hotspots.json"),
    );
    let rows = payload.as_array().unwrap();
    assert!(!rows.is_empty());
    let confidence = &rows[0]["measurement_confidence"];
    assert_eq!(confidence["complete"], true);
    assert_eq!(confidence["partial"], false);
    assert_eq!(confidence["confidence_scope"], "syntax_facts");
    assert_eq!(
        confidence["required_inputs"],
        serde_json::json!(["rust_source_files", "rust_syntax_facts"])
    );
    assert!(confidence["missing_input"].as_array().unwrap().is_empty());
}

#[test]
fn clones_report_source_scan_confidence_without_syntax_fact_requirements() {
    let fixture = write_clone_fixture("clone-confidence");
    let config = fixture.join("rqlens.toml").to_string_lossy().to_string();
    let output = run(&["measure", "clones", "--config", &config]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload = read_json(fixture.join("target/analysis/clones.json"));
    let rows = payload.as_array().unwrap();
    assert!(!rows.is_empty());
    let token_row = rows
        .iter()
        .find(|row| row["engine"] == "token")
        .expect("token clone rows should be emitted");
    let token_confidence = &token_row["measurement_confidence"];
    assert_eq!(token_confidence["complete"], true);
    assert_eq!(token_confidence["partial"], false);
    assert_eq!(token_confidence["confidence_scope"], "source_scan");
    assert_eq!(
        token_confidence["required_inputs"],
        serde_json::json!(["rust_source_files"])
    );
    assert!(
        token_confidence["missing_input"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let ast_row = rows
        .iter()
        .find(|row| row["engine"] == "ast")
        .expect("AST clone rows should be emitted");
    assert_eq!(
        ast_row["measurement_confidence"]["confidence_scope"],
        "syntax_facts"
    );
}
