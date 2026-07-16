#![allow(clippy::expect_used, clippy::unwrap_used)]

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
    let value: Value =
        serde_json::from_str(&fs::read_to_string(path).expect("json file should exist"))
            .expect("json should parse");
    value
        .get("records")
        .or_else(|| value.get("data"))
        .cloned()
        .unwrap_or(value)
}

fn read_document(path: impl AsRef<Path>) -> Value {
    serde_json::from_str(&fs::read_to_string(path).expect("json file should exist"))
        .expect("json should parse")
}

fn write_clone_fixture(name: &str) -> PathBuf {
    let root = repo_root().join("target").join("test-fixtures").join(name);
    if root.exists() {
        fs::remove_dir_all(&root).expect("old test fixture should be removable");
    }
    fs::create_dir_all(root.join("src")).expect("test fixture src should be created");
    fs::create_dir_all(root.join("tests")).expect("test fixture tests should be created");
    fs::write(
        root.join("Cargo.toml"),
        r#"[package]
name = "clone-confidence"
version = "0.1.0"
edition = "2024"
"#,
    )
    .expect("test manifest should be written");
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
pub mod alpha;
pub mod beta;

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
    fs::write(
        root.join("src").join("alpha.rs"),
        r#"
pub fn read_alpha(input: usize) -> usize {
    let first = input + 1;
    let second = first + 2;
    if second > 10 {
        second
    } else {
        input
    }
}

pub fn write_alpha(input: usize) -> usize {
    let first = input + 3;
    let second = first + 4;
    if second > 20 {
        second
    } else {
        input
    }
}
"#,
    )
    .expect("alpha source should be written");
    fs::write(
        root.join("src").join("beta.rs"),
        r#"
pub fn read_beta(input: usize) -> usize {
    let first = input + 5;
    let second = first + 6;
    if second > 30 {
        second
    } else {
        input
    }
}

pub fn write_beta(input: usize) -> usize {
    let first = input + 7;
    let second = first + 8;
    if second > 40 {
        second
    } else {
        input
    }
}
"#,
    )
    .expect("beta source should be written");
    let duplicated_test = |threshold: usize| {
        format!(
            r#"
#[test]
fn duplicated_setup() {{
    let input = {threshold};
    let first = input + 1;
    let second = first + 2;
    let third = second + 3;
    assert!(third > input);
}}
"#
        )
    };
    fs::write(root.join("tests").join("dup_a.rs"), duplicated_test(10))
        .expect("first duplicated test should be written");
    fs::write(root.join("tests").join("dup_b.rs"), duplicated_test(20))
        .expect("second duplicated test should be written");
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
    }
    root
}

fn write_entrypoint_fixture(name: &str) -> PathBuf {
    let root = repo_root().join("target").join("test-fixtures").join(name);
    if root.exists() {
        fs::remove_dir_all(&root).expect("old entrypoint fixture should be removable");
    }
    fs::create_dir_all(root.join("src")).expect("entrypoint src should be created");
    fs::create_dir_all(root.join("tools")).expect("entrypoint tools should be created");
    fs::write(
        root.join("rqlens.toml"),
        format!(
            r#"project_name = "{name}"
project_root = "."
source_roots = ["src", "tools"]
output_dir = "target/analysis"
"#
        ),
    )
    .expect("entrypoint config should be written");
    fs::write(
        root.join("Cargo.toml"),
        r#"[package]
name = "entrypoint-fixture"
version = "0.1.0"
edition = "2024"

[[bin]]
name = "custom-tool"
path = "tools/custom.rs"
"#,
    )
    .expect("entrypoint manifest should be written");
    fs::write(
        root.join("src").join("lib.rs"),
        "pub mod domain;\npub mod service;\n",
    )
    .expect("entrypoint lib should be written");
    fs::write(
        root.join("src").join("domain.rs"),
        "pub fn value() -> i32 { 1 }\n",
    )
    .expect("entrypoint domain should be written");
    fs::write(
        root.join("src").join("service.rs"),
        "use crate::domain::value;\npub fn run() -> i32 { value() }\n",
    )
    .expect("entrypoint service should be written");
    fs::write(
        root.join("src").join("main.rs"),
        r#"
use crate::domain::value;
use crate::service::run;

fn main() {
    let first = parse();
    let second = execute();
    report(first + second);
}

fn parse() -> i32 {
    value()
}

fn execute() -> i32 {
    run()
}

fn report(value: i32) {
    println!("{}", value);
}
"#,
    )
    .expect("default bin should be written");
    fs::write(
        root.join("tools").join("custom.rs"),
        r#"
use crate::domain::value;
use crate::service::run;

fn main() {
    let first = parse();
    let second = execute();
    report(first + second);
}

fn parse() -> i32 {
    value()
}

fn execute() -> i32 {
    run()
}

fn report(value: i32) {
    println!("{}", value);
}
"#,
    )
    .expect("custom bin should be written");
    root
}

fn node_data<'a>(payload: &'a Value, id: &str) -> &'a Value {
    payload["graph"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| &node["data"])
        .find(|data| data["id"] == id || data["module_key"] == id)
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
fn config_schema_prints_machine_readable_schema() {
    let output = run(&["config-schema"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["title"], "rust-quality-lens config");
    assert_eq!(payload["properties"]["source_roots"]["type"], "array");
    assert_eq!(
        payload["properties"]["rust"]["properties"]["identity_resolution"]["enum"],
        serde_json::json!(["auto", "required", "disabled"])
    );
}

#[test]
fn artifact_schema_prints_known_output_contracts() {
    let output = run(&["artifact-schema"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    let properties = payload["properties"].as_object().unwrap();
    for file in [
        "hotspots.json",
        "clones.json",
        "rust_escape_hatches.json",
        "type_health.json",
        "correctness_review.json",
        "coverage.json",
        "locality_metrics.json",
        "leverage_metrics.json",
        "map.json",
    ] {
        assert!(properties.contains_key(file), "missing schema for {file}");
    }

    let map_output = run(&["artifact-schema", "map"]);
    assert!(
        map_output.status.success(),
        "{}",
        String::from_utf8_lossy(&map_output.stderr)
    );
    let map_schema: Value = serde_json::from_slice(&map_output.stdout).unwrap();
    assert_eq!(map_schema["title"], "map.json artifact envelope");
    assert!(
        map_schema["properties"]["data"]["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|key| key == "graph")
    );
}

#[test]
fn init_writes_default_config() {
    let root = repo_root()
        .join("target")
        .join("test-fixtures")
        .join("init-command");
    if root.exists() {
        fs::remove_dir_all(&root).expect("old init fixture should be removable");
    }
    fs::create_dir_all(&root).expect("init fixture dir should be created");
    let config_path = root.join("rqlens.toml");
    let output = run(&["init", "--path", &config_path.to_string_lossy(), "--force"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = fs::read_to_string(config_path).expect("config should be written");
    assert!(text.contains("source_roots = [\"src\"]"));
    assert!(text.contains("[rust]"));
}

#[test]
fn review_uses_diff_file_scope() {
    let diff_file = repo_root()
        .join("target")
        .join("test-fixtures")
        .join("review.diff");
    if let Some(parent) = diff_file.parent() {
        fs::create_dir_all(parent).expect("review fixture dir should be created");
    }
    fs::write(
        &diff_file,
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n",
    )
    .expect("diff should be written");
    let output = run(&[
        "review",
        "--diff-file",
        &diff_file.to_string_lossy(),
        "--config",
        "tests/fixtures/mini_rust_project/rqlens.toml",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload =
        read_json(repo_root().join("tests/fixtures/mini_rust_project/target/analysis/review.json"));
    assert_eq!(payload["scope"]["measured_rust_files"][0], "src/lib.rs");
    assert!(payload["measurements"].as_array().unwrap().len() >= 4);
}

#[test]
fn generated_artifacts_keep_expected_top_level_shapes() {
    let output = run(&[
        "measure",
        "all",
        "--config",
        "tests/fixtures/mini_rust_project/rqlens.toml",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let analysis = repo_root().join("tests/fixtures/mini_rust_project/target/analysis");
    let hotspots = read_json(analysis.join("hotspots.json"));
    assert!(hotspots.as_array().unwrap().iter().all(|row| {
        row.get("module_key").is_some()
            && row.get("kind").is_some()
            && row.get("score").is_some()
            && row.get("score_components").is_some()
            && row.get("measurement_confidence").is_some()
    }));
    assert!(
        hotspots
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["kind"] == "function")
    );
    let correctness = read_json(analysis.join("correctness_review.json"));
    for key in ["version", "generated_from", "summary", "tests"] {
        assert!(correctness.get(key).is_some(), "missing {key}");
    }
    let map = read_json(analysis.join("map.json"));
    for key in ["meta", "graph", "modules", "measurement_confidence"] {
        assert!(map.get(key).is_some(), "missing {key}");
    }
    assert!(map["graph"]["nodes"].as_array().is_some());
    assert!(map["graph"]["edges"].as_array().is_some());
}

#[test]
fn generated_artifacts_match_envelope_conformance_snapshot() {
    let output = run(&[
        "measure",
        "all",
        "--config",
        "tests/fixtures/mini_rust_project/rqlens.toml",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Value = serde_json::from_str(include_str!("snapshots/artifact_envelopes.json"))
        .expect("snapshot should parse");
    let analysis = repo_root().join("tests/fixtures/mini_rust_project/target/analysis");
    for (file, expected_keys) in expected.as_object().unwrap() {
        let document = read_document(analysis.join(file));
        let mut actual = document
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        actual.sort();
        assert_eq!(
            actual,
            expected_keys
                .as_array()
                .unwrap()
                .iter()
                .map(|key| key.as_str().unwrap().to_string())
                .collect::<Vec<_>>(),
            "contract drift in {file}"
        );
        assert_eq!(document["schema_version"], 2);
    }
}

#[test]
fn normalized_measurements_match_golden_snapshot() {
    let config = "tests/fixtures/mini_rust_project/rqlens.toml";
    let output = run(&["measure", "all", "--config", config]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let analysis = repo_root().join("tests/fixtures/mini_rust_project/target/analysis");
    let hotspots = read_json(analysis.join("hotspots.json"));
    let map = read_json(analysis.join("map.json"));
    let normalized = serde_json::json!({
        "hotspots": hotspots.as_array().unwrap().iter()
            .filter(|row| row["kind"] == "module")
            .map(|row| serde_json::json!({
            "module_key": row["module_key"],
            "max_function_score": row["max_function_score"],
            "p95_function_score": row["p95_function_score"],
            "score": row["score"],
        })).collect::<Vec<_>>(),
        "dependency_edges": map["graph"]["edges"].as_array().unwrap().iter()
            .filter(|edge| edge["data"]["kind"] == "dependency")
            .map(|edge| serde_json::json!([
                edge["data"]["source"],
                edge["data"]["target"],
            ]))
            .collect::<Vec<_>>(),
    });
    let expected: Value = serde_json::from_str(include_str!("snapshots/mini_metrics.json"))
        .expect("metric snapshot should parse");
    assert_eq!(normalized, expected);
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
    let domain = rows
        .iter()
        .find(|item| item["qualified_name"] == "domain::Config")
        .unwrap();
    assert_eq!(domain["method_count"], 1);
    assert_eq!(domain["impl_file_count"], 1);
    assert!(
        domain["impl_files"][0]
            .as_str()
            .unwrap()
            .ends_with("src/service.rs")
    );
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
    assert!(edges.iter().any(|edge| {
        edge["data"]["source"] == "mini-rust-project::mini-rust-project::lib"
            && edge["data"]["target"] == "mini-rust-project::mini-rust-project::math"
    }));
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
    assert_eq!(payload["measurement_confidence"]["partial"], true);
    let service = node_data(&payload, "service");
    assert!(service["total_score"].is_null());
    assert!(
        service["unknown_metrics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|metric| metric == "maintainability_risk")
    );
}

#[test]
fn map_scores_git_cycles_and_layer_violations() {
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
    assert!(service["total_score"].as_f64().is_some());
    assert!(service["unknown_metrics"].as_array().unwrap().is_empty());
}

#[test]
fn entrypoints_are_visible_and_discounted_in_outputs() {
    let fixture = write_entrypoint_fixture("entrypoint-awareness");
    let config = fixture.join("rqlens.toml").to_string_lossy().to_string();
    for tool in [
        "hotspots",
        "correctness",
        "locality",
        "leverage",
        "clones",
        "map",
    ] {
        let output = run(&["measure", tool, "--config", &config]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let analysis = fixture.join("target/analysis");
    let locality = read_json(analysis.join("locality_metrics.json"));
    let main_locality = locality
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["module_key"] == "main")
        .expect("main entrypoint should have locality row");
    assert_eq!(main_locality["target_kind"], "bin");
    assert_eq!(main_locality["entrypoint_kind"], "bin");
    assert_eq!(main_locality["is_entrypoint"], true);

    let map = read_json(analysis.join("map.json"));
    let main = node_data(&map, "main");
    assert_eq!(main["target_kind"], "bin");
    assert_eq!(main["entrypoint_kind"], "bin");
    assert_eq!(main["is_entrypoint"], true);
    let custom = node_data(&map, "custom");
    assert_eq!(custom["target_kind"], "bin");
    assert_eq!(custom["entrypoint_kind"], "bin");
    assert_eq!(custom["is_entrypoint"], true);

    let clones = read_json(analysis.join("clones.json"));
    let entrypoint_clone = clones
        .as_array()
        .unwrap()
        .iter()
        .find(|row| {
            row["engine"] == "module-responsibility"
                && row["is_entrypoint"] == true
                && row["target_kind"] == "bin"
        })
        .expect("entrypoint responsibility duplication should be surfaced");
    assert!(
        entrypoint_clone["instances"]
            .as_array()
            .unwrap()
            .iter()
            .any(|instance| instance["module_key"] == "main")
    );
    assert!(
        entrypoint_clone["instances"]
            .as_array()
            .unwrap()
            .iter()
            .any(|instance| instance["module_key"] == "custom")
    );

    let diff_file = fixture.join("entrypoint.diff");
    fs::write(
        &diff_file,
        "diff --git a/tools/custom.rs b/tools/custom.rs\n--- a/tools/custom.rs\n+++ b/tools/custom.rs\n@@ -1 +1 @@\n",
    )
    .expect("entrypoint diff should be written");
    let output = run(&[
        "review",
        "--diff-file",
        &diff_file.to_string_lossy(),
        "--config",
        &config,
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let review = read_json(analysis.join("review.json"));
    assert!(
        review["scope"]["entrypoints"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entrypoint| {
                entrypoint["module_key"] == "custom"
                    && entrypoint["target_kind"] == "bin"
                    && entrypoint["entrypoint_kind"] == "bin"
            })
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

#[test]
fn correctness_attributes_inline_tests_to_production_modules() {
    let output = run(&[
        "measure",
        "correctness",
        "--config",
        "tests/fixtures/golden_rust_project/rqlens.toml",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let review = read_json(
        repo_root()
            .join("tests/fixtures/golden_rust_project/target/analysis/correctness_review.json"),
    );
    let domain_test = review["tests"]
        .as_array()
        .unwrap()
        .iter()
        .find(|test| test["name"] == "macro_case_test")
        .expect("domain test should be discovered");
    assert!(
        domain_test["tested_modules"]
            .as_array()
            .unwrap()
            .iter()
            .any(|module| module == "domain")
    );
    let integration_test = review["tests"]
        .as_array()
        .unwrap()
        .iter()
        .find(|test| test["path"] == "tests/duplicate_a.rs")
        .expect("integration test should be discovered");
    assert!(
        integration_test["tested_modules"]
            .as_array()
            .unwrap()
            .iter()
            .any(|module| module == "alpha")
    );
}

#[test]
fn check_command_enforces_threshold_policy() {
    let config = "tests/fixtures/mini_rust_project/rqlens.toml";
    let measure = run(&["measure", "all", "--config", config]);
    assert!(
        measure.status.success(),
        "{}",
        String::from_utf8_lossy(&measure.stderr)
    );
    let check = run(&[
        "check",
        "--fail-on",
        "threshold",
        "--fail-on",
        "regression",
        "--baseline",
        "tests/fixtures/mini_rust_project/target/analysis",
        "--max-total-score",
        "10000",
        "--config",
        config,
    ]);
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
    let report: Value = serde_json::from_slice(&check.stdout).unwrap();
    assert_eq!(report["passed"], true);
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
    let module_row = rows
        .iter()
        .find(|row| {
            row["engine"] == "module-responsibility"
                && row["instances"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|instance| instance["module_key"] == "alpha")
                && row["instances"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|instance| instance["module_key"] == "beta")
        })
        .expect("module responsibility duplication rows should be emitted");
    assert_eq!(
        module_row["risk_calibration"],
        "clones_module_responsibility"
    );
    let test_row = rows
        .iter()
        .find(|row| row["engine"] == "test-ast")
        .expect("duplicated test body rows should be emitted");
    assert_eq!(test_row["risk_calibration"], "clones_test_ast");
    assert_eq!(
        test_row["measurement_confidence"]["confidence_scope"],
        "syntax_facts"
    );
}
