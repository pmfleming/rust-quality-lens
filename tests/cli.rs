use anyhow::Context;

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

fn run(args: &[&str]) -> anyhow::Result<std::process::Output> {
    let output = Command::new(rqlens())
        .args(args)
        .current_dir(repo_root())
        .output()
        .context("rqlens command should run")?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output)
}

// All CLI fixtures use the same analysis layout; keep invocation and lookup together.
fn measure_fixture(tool: &str, config: &str) -> anyhow::Result<PathBuf> {
    run(&["measure", tool, "--config", config])?;
    Ok(repo_root()
        .join(config)
        .parent()
        .context("fixture config has a parent")?
        .join("target/analysis"))
}

fn json_array(value: &Value) -> anyhow::Result<&Vec<Value>> {
    value.as_array().context("expected a JSON array")
}

fn read_json(path: impl AsRef<Path>) -> anyhow::Result<Value> {
    let value = read_document(path)?;
    Ok(value
        .get("records")
        .or_else(|| value.get("data"))
        .cloned()
        .unwrap_or(value))
}

fn read_document(path: impl AsRef<Path>) -> anyhow::Result<Value> {
    serde_json::from_str(&fs::read_to_string(path).context("json file should exist")?)
        .context("json should parse")
}

fn fixture_root(name: &str) -> anyhow::Result<PathBuf> {
    let root = repo_root().join("target/test-fixtures").join(name);
    if root.exists() {
        fs::remove_dir_all(&root).context("old test fixture should be removable")?;
    }
    fs::create_dir_all(root.join("src")).context("test fixture src should be created")?;
    Ok(root)
}

fn write_files(root: &Path, files: &[(&str, &str)]) -> anyhow::Result<()> {
    files
        .iter()
        .try_for_each(|(path, text)| fs::write(root.join(path), text))
        .context("fixture files should be written")?;

    Ok(())
}

fn write_clone_fixture(name: &str) -> anyhow::Result<PathBuf> {
    let root = fixture_root(name)?;
    fs::create_dir_all(root.join("tests")).context("test fixture tests should be created")?;
    fs::write(
        root.join("Cargo.toml"),
        r#"[package]
name = "clone-confidence"
version = "0.1.0"
edition = "2024"
"#,
    )
    .context("test manifest should be written")?;
    fs::write(
        root.join("rqlens.toml"),
        r#"project_name = "clone-confidence"
project_root = "."
source_roots = ["src"]
output_dir = "target/analysis"
"#,
    )
    .context("test config should be written")?;
    let repeated = |name| {
        format!(
            r#"pub fn repeated_{name}(input: usize) -> usize {{
    let first = input + 1;
    let second = first + 2;
    let third = second + 3;
    let fourth = third + 4;
    if fourth > 10 {{
        fourth
    }} else {{
        input
    }}
}}
"#
        )
    };
    fs::write(
        root.join("src/lib.rs"),
        format!(
            "\npub mod alpha;\npub mod beta;\n\n{}\n{}",
            repeated("one"),
            repeated("two")
        ),
    )?;
    for (module, increment, threshold) in [("alpha", 1, 10), ("beta", 5, 30)] {
        let read = clone_function(&format!("read_{module}"), increment, threshold);
        let write = clone_function(&format!("write_{module}"), increment + 2, threshold + 10);
        fs::write(
            root.join(format!("src/{module}.rs")),
            format!("\n{read}\n{write}"),
        )?;
    }
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
        .context("first duplicated test should be written")?;
    fs::write(root.join("tests").join("dup_b.rs"), duplicated_test(20))
        .context("second duplicated test should be written")?;
    Ok(root)
}

fn clone_function(name: &str, increment: usize, threshold: usize) -> String {
    let second = increment + 1;
    format!(
        r#"pub fn {name}(input: usize) -> usize {{
    let first = input + {increment};
    let second = first + {second};
    if second > {threshold} {{
        second
    }} else {{
        input
    }}
}}
"#
    )
}

fn write_architecture_fixture(name: &str, artifacts: bool) -> anyhow::Result<PathBuf> {
    let root = fixture_root(name)?;
    write_files(
        &root,
        &[
            (
                "rqlens.toml",
                &format!(
                    "project_name = \"{name}\"\nproject_root = \".\"\nsource_roots = [\"src\"]\noutput_dir = \"target/analysis\"\n"
                ),
            ),
            (
                "Cargo.toml",
                "[package]\nname = \"arch-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            ("src/lib.rs", "pub mod domain;\npub mod service;\n"),
            (
                "src/domain.rs",
                "use crate::service::run;\npub fn value() -> i32 { run() }\n",
            ),
            (
                "src/service.rs",
                "use crate::domain::value;\npub fn run() -> i32 { value() }\n",
            ),
        ],
    )?;
    if artifacts {
        std::thread::sleep(std::time::Duration::from_millis(20));
        let analysis = root.join("target").join("analysis");
        fs::create_dir_all(&analysis).context("analysis dir should be created")?;
        fs::write(
            analysis.join("hotspots.json"),
            serde_json::to_string(&serde_json::json!([
                {"module_key": "lib", "score": 1.0},
                {"module_key": "domain", "score": 10.0},
                {"module_key": "service", "score": 20.0}
            ]))
            .context("fixture value")?,
        )
        .context("hotspots should be written")?;
        fs::write(
            analysis.join("correctness_review.json"),
            serde_json::to_string(&serde_json::json!({
                "tests": [
                    {"module": "domain", "last_status": "failed"},
                    {"module": "service", "last_status": "passed"}
                ]
            }))
            .context("fixture value")?,
        )
        .context("correctness should be written")?;
        fs::write(analysis.join("locality_metrics.json"), "[]")
            .context("locality should be written")?;
        fs::write(analysis.join("leverage_metrics.json"), "[]")
            .context("leverage should be written")?;
        wrap_fixture_artifacts(&root)?;
    }
    Ok(root)
}

fn wrap_fixture_artifacts(root: &Path) -> anyhow::Result<()> {
    let analysis = measure_fixture(
        "api-health",
        root.join("rqlens.toml").to_str().context("fixture value")?,
    )?;
    let template = read_document(analysis.join("api_health.json"))?;
    for (file, tool) in [
        ("hotspots.json", "hotspots"),
        ("correctness_review.json", "correctness-run"),
        ("locality_metrics.json", "locality"),
        ("leverage_metrics.json", "leverage"),
    ] {
        let mut payload = read_json(analysis.join(file))?;
        if tool == "correctness-run" {
            payload["summary"] =
                serde_json::json!({"failed": 1, "run_failed": false, "compile_failed": false});
        }
        let mut envelope = template.clone();
        envelope
            .as_object_mut()
            .context("fixture value")?
            .remove("records");
        envelope["tool"] = serde_json::json!(tool);
        envelope["measurement_confidence"] =
            serde_json::json!({"complete": true, "partial": false});
        let payload_key = if payload.is_array() {
            "records"
        } else {
            "data"
        };
        envelope[payload_key] = payload;
        fs::write(
            analysis.join(file),
            serde_json::to_vec(&envelope).context("fixture value")?,
        )?;
    }
    Ok(())
}

fn write_syntax_error_fixture(name: &str) -> anyhow::Result<PathBuf> {
    let root = fixture_root(name)?;
    write_files(
        &root,
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"syntax-error-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            (
                "rqlens.toml",
                "project_name = \"syntax-error-fixture\"\nproject_root = \".\"\nsource_roots = [\"src\"]\noutput_dir = \"target/analysis\"\n",
            ),
            (
                "src/lib.rs",
                "//! Partially measurable crate.\n\npub fn recovered(flag: bool) -> bool {\n    if flag { true } else { false }\n}\n\npub fn broken( {\n    // source metrics survive\n",
            ),
        ],
    )?;
    Ok(root)
}

fn write_entrypoint_fixture(name: &str) -> anyhow::Result<PathBuf> {
    let root = fixture_root(name)?;
    fs::create_dir_all(root.join("tools/custom")).context("entrypoint tools should be created")?;
    fs::write(
        root.join("rqlens.toml"),
        format!(
            r#"project_name = "{name}"
project_root = "."
output_dir = "target/analysis"
"#
        ),
    )
    .context("entrypoint config should be written")?;
    fs::write(
        root.join("Cargo.toml"),
        r#"[package]
name = "entrypoint-fixture"
version = "0.1.0"
edition = "2024"

[[bin]]
name = "custom-tool"
path = "tools/custom.rs"

[workspace]
"#,
    )
    .context("entrypoint manifest should be written")?;
    fs::write(
        root.join("src").join("lib.rs"),
        "pub mod domain;\npub mod service;\n",
    )
    .context("entrypoint lib should be written")?;
    fs::write(
        root.join("src").join("domain.rs"),
        "pub fn value() -> i32 { 1 }\n",
    )
    .context("entrypoint domain should be written")?;
    fs::write(
        root.join("src").join("service.rs"),
        "use crate::domain::value;\npub fn run() -> i32 { value() }\n",
    )
    .context("entrypoint service should be written")?;
    let entrypoint = r#"
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
"#;
    fs::write(root.join("src/main.rs"), entrypoint).context("default bin should be written")?;
    fs::write(
        root.join("tools/custom.rs"),
        format!("\nmod child;\n{entrypoint}"),
    )
    .context("custom bin should be written")?;
    fs::write(
        root.join("tools/custom/child.rs"),
        "pub fn custom_child() -> i32 { 1 }\n",
    )
    .context("custom bin child module should be written")?;
    Ok(root)
}

fn node_data<'a>(payload: &'a Value, id: &str) -> anyhow::Result<&'a Value> {
    json_array(&payload["graph"]["nodes"])?
        .iter()
        .map(|node| &node["data"])
        .find(|data| data["id"] == id || data["module_key"] == id)
        .context("node should exist")
}

#[test]
fn catalog_lists_board_compatible_tasks() -> anyhow::Result<()> {
    let output = run(&[
        "catalog",
        "--config",
        "tests/fixtures/mini_rust_project/rqlens.toml",
    ])?;
    let payload: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(payload["project_name"], "mini-rust-project");
    let ids: Vec<&str> = json_array(&payload["tasks"])?
        .iter()
        .map(|task| task["id"].as_str().context("task ID"))
        .collect::<anyhow::Result<_>>()?;
    assert!(ids.contains(&"quality.escape_hatches"));
    assert!(ids.contains(&"quality.type_health"));
    assert!(ids.contains(&"correctness.catalog"));
    assert!(ids.contains(&"map.architecture"));

    Ok(())
}

#[test]
fn config_and_artifact_schemas_are_machine_readable() -> anyhow::Result<()> {
    artifact_schema_prints_known_output_contracts()?;
    let output = run(&["config-schema"])?;
    let payload: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(payload["properties"]["source_roots"]["type"], "array");
    assert_eq!(
        payload["properties"]["rust"]["properties"]["identity_resolution"]["enum"],
        serde_json::json!(["auto", "required", "disabled"])
    );
    assert_eq!(
        payload["properties"]["policy"]["properties"]["waivers"]["items"]["required"],
        serde_json::json!(["rule_id", "reason", "owner", "expires"])
    );
    assert_eq!(
        payload["properties"]["policy"]["properties"]["rules"]["additionalProperties"]["properties"]
            ["level"]["enum"],
        serde_json::json!(["off", "advisory", "warning", "error"])
    );
    assert_eq!(
        payload["properties"]["verification"]["properties"]["feature_matrix"]["default"],
        false
    );
    assert_eq!(
        payload["properties"]["verification"]["properties"]["mutation"]["default"],
        false
    );
    assert_eq!(
        payload["properties"]["verification"]["properties"]["flaky_test_runs"]["minimum"],
        1
    );
    assert_eq!(
        payload["properties"]["verification"]["properties"]["sanitizers"]["items"]["enum"],
        serde_json::json!(["address", "leak", "memory", "thread"])
    );

    Ok(())
}

fn artifact_schema_prints_known_output_contracts() -> anyhow::Result<()> {
    let output = run(&["artifact-schema"])?;
    let payload: Value = serde_json::from_slice(&output.stdout)?;
    let properties = payload["properties"].as_object().context("fixture value")?;
    for file in [
        "hotspots.json",
        "clones.json",
        "api_health.json",
        "rust_escape_hatches.json",
        "reliability_findings.json",
        "type_health.json",
        "correctness_review.json",
        "coverage.json",
        "locality_metrics.json",
        "leverage_metrics.json",
        "map.json",
        "rust_practices.json",
    ] {
        assert!(properties.contains_key(file), "missing schema for {file}");
    }

    let hotspot_fields =
        &properties["hotspots.json"]["properties"]["records"]["items"]["properties"];
    assert_eq!(hotspot_fields["halstead_metric_model_version"]["const"], 1);
    assert!(
        json_array(&hotspot_fields["halstead"]["required"])?
            .iter()
            .any(|field| field == "distinct_operands")
    );

    let map_output = run(&["artifact-schema", "map"])?;
    let map_schema: Value = serde_json::from_slice(&map_output.stdout)?;
    assert!(
        json_array(&map_schema["required"])?
            .iter()
            .any(|key| key == "toolchain")
    );
    assert_eq!(map_schema["properties"]["schema_version"]["const"], 3);
    assert!(
        json_array(&map_schema["properties"]["data"]["required"])?
            .iter()
            .any(|key| key == "graph")
    );

    Ok(())
}

#[test]
fn verify_writes_structured_practice_evidence() -> anyhow::Result<()> {
    let config = "tests/fixtures/mini_rust_project/rqlens.toml";
    run(&["verify", "--config", config])?;
    let report = read_json(
        repo_root().join("tests/fixtures/mini_rust_project/target/analysis/rust_practices.json"),
    )?;
    let checks = report["checks"]
        .as_array()
        .context("checks should be an array")?;
    assert!(checks.iter().any(|check| {
        check["rule_id"] == "rust.official.rustfmt"
            && matches!(check["status"].as_str(), Some("passed" | "failed"))
    }));
    assert!(checks.iter().all(|check| check["source"].is_string()));
    assert!(
        checks
            .iter()
            .any(|check| check["rule_id"] == "rust.project.msrv-compatible")
    );
    assert_eq!(
        report["measurement_confidence"]["confidence_scope"],
        "verified_practices"
    );

    Ok(())
}

#[test]
fn init_writes_default_config() -> anyhow::Result<()> {
    let root = repo_root()
        .join("target")
        .join("test-fixtures")
        .join("init-command");
    if root.exists() {
        fs::remove_dir_all(&root).context("old init fixture should be removable")?;
    }
    fs::create_dir_all(&root).context("init fixture dir should be created")?;
    let config_path = root.join("rqlens.toml");
    run(&["init", "--path", &config_path.to_string_lossy(), "--force"])?;
    let text = fs::read_to_string(config_path).context("config should be written")?;
    assert!(text.contains("Cargo workspace and local path packages are discovered automatically"));
    assert!(text.contains("[rust]"));
    assert!(text.contains("[verification]"));

    Ok(())
}

#[test]
fn review_uses_diff_file_scope() -> anyhow::Result<()> {
    let diff_file = repo_root()
        .join("target")
        .join("test-fixtures")
        .join("review.diff");
    if let Some(parent) = diff_file.parent() {
        fs::create_dir_all(parent).context("review fixture dir should be created")?;
    }
    fs::write(
        &diff_file,
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n",
    )
    .context("diff should be written")?;
    run(&[
        "review",
        "--diff-file",
        &diff_file.to_string_lossy(),
        "--config",
        "tests/fixtures/mini_rust_project/rqlens.toml",
    ])?;
    let payload = read_json(
        repo_root().join("tests/fixtures/mini_rust_project/target/analysis/review.json"),
    )?;
    assert_eq!(payload["version"], 3);
    assert_eq!(payload["finding_snapshot"]["version"], 1);
    assert_eq!(payload["baseline_comparison"]["status"], "not_configured");
    assert_eq!(payload["scope"]["measured_rust_files"][0], "src/lib.rs");
    assert_eq!(
        payload["scope"]["changed_lines"]["src/lib.rs"],
        serde_json::json!([[1, 1]])
    );
    assert!(payload["change_evidence"]["status"].is_string());
    assert!(json_array(&payload["measurements"])?.len() >= 4);

    Ok(())
}

#[test]
fn structural_cohesion_is_published_with_explicit_scope() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    fs::create_dir(root.path().join("src"))?;
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='cohesion-fixture'\nversion='0.1.0'\nedition='2024'\n",
    )?;
    fs::write(
        root.path().join("src/lib.rs"),
        "pub struct State { x: i32, y: i32 } impl State { pub fn x(&self) -> i32 { self.x } pub fn y(&self) -> i32 { self.y } }",
    )?;
    let config = root.path().join("rqlens.toml");
    fs::write(&config, "[rust]\nidentity_resolution='disabled'\n")?;
    run(&[
        "measure",
        "module-cohesion",
        "--config",
        config.to_str().context("fixture value")?,
    ])?;
    let report = read_json(root.path().join("target/analysis/module_cohesion.json"))?;
    assert_eq!(report["entropy_model_version"], 2);
    assert!(report["summary"]["total_changed_lines"].is_null());
    assert!(report["summary"]["change_entropy"].is_null());
    assert!(report["records"][0]["changed_lines"].is_null());
    assert!(report["history_error"].is_string());
    assert_eq!(report["structural"]["model_version"], 1);
    let row = &report["structural"]["records"][0];
    assert_eq!(row["field_sharing_percent"], 0.0);
    assert_eq!(row["component_count"], 2);
    assert_eq!(row["measurement_confidence"]["complete"], true);
    assert_eq!(
        report["maintenance"]["measurement_confidence"]["complete"],
        false
    );

    Ok(())
}

#[test]
fn measurement_bundle_preserves_contracts_scores_and_attribution() -> anyhow::Result<()> {
    let analysis = measure_fixture("all", "tests/fixtures/mini_rust_project/rqlens.toml")?;
    generated_artifacts_keep_expected_top_level_shapes(&analysis)?;
    generated_artifacts_match_envelope_conformance_snapshot(&analysis)?;
    normalized_measurements_match_golden_snapshot(&analysis)?;
    map_captures_direct_module_path_dependency(&analysis)?;
    correctness_writes_review_and_test_catalog(&analysis)?;
    hotspots_report_syntax_fact_confidence(&analysis)?;
    function_risk_uses_current_coverage(&analysis)?;

    Ok(())
}

fn function_risk_uses_current_coverage(analysis: &Path) -> anyhow::Result<()> {
    // A fresh synthetic coverage artifact, not whatever a previous run left behind.
    let mut document = read_document(analysis.join("hotspots.json"))?;
    let function = json_array(&document["records"])?
        .iter()
        .find(|row| row["kind"] == "function")
        .context("fixture value")?
        .clone();
    let root = repo_root().join("tests/fixtures/mini_rust_project");
    let path = Path::new(function["path"].as_str().context("fixture value")?)
        .strip_prefix(&root)
        .context("fixture value")?
        .to_string_lossy()
        .replace('\\', "/");
    document
        .as_object_mut()
        .context("fixture value")?
        .remove("records");
    document["tool"] = serde_json::json!("coverage");
    document["generated_from"] = serde_json::json!("test-fixture");
    document["capabilities"] = serde_json::json!([]);
    document["measurement_confidence"] = serde_json::json!({"complete": true, "partial": false});
    document["data"] = serde_json::json!({
        "summary": {"lines": {"count": 1, "covered": 1, "percent": 100.0}},
        "files": [{"path": path, "line_hits": [{"line": function["start_line"], "covered": true}]}]
    });
    fs::write(
        analysis.join("coverage.json"),
        serde_json::to_vec(&document).context("fixture value")?,
    )?;
    measure_fixture(
        "function-risk",
        root.join("rqlens.toml").to_str().context("fixture value")?,
    )?;
    let report = read_json(analysis.join("function_risk.json"))?;
    assert_eq!(report["measurement_confidence"]["complete"], true);
    let measured = json_array(&report["functions"])?
        .iter()
        .find(|row| row["name"] == function["name"])
        .context("fixture value")?;
    assert_eq!(measured["coverage_percent"], 100.0);
    assert_eq!(
        measured["crap"].as_f64().context("fixture value")?,
        function["cyclomatic_complexity"]
            .as_f64()
            .context("fixture value")?
    );

    Ok(())
}

fn generated_artifacts_keep_expected_top_level_shapes(analysis: &Path) -> anyhow::Result<()> {
    let hotspots = read_json(analysis.join("hotspots.json"))?;
    assert!(json_array(&hotspots)?.iter().all(|row| {
        row.get("module_key").is_some()
            && row.get("kind").is_some()
            && row.get("score").is_some()
            && row.get("score_components").is_some()
            && row.get("measurement_confidence").is_some()
    }));
    let function = json_array(&hotspots)?
        .iter()
        .find(|row| row["kind"] == "function")
        .context("function hotspot should be emitted")?;
    assert!(function["cyclomatic_complexity"].as_u64().is_some());
    assert!(function["cognitive_complexity"].as_u64().is_some());
    assert_eq!(
        function["complexity_metric_model_id"],
        "rqlens.rust_standard_complexity"
    );
    let module = json_array(&hotspots)?
        .iter()
        .find(|row| row["kind"] == "module")
        .context("module hotspot should be emitted")?;
    assert!(module["cyclomatic_complexity_sum"].as_u64().is_some());
    assert!(module["cognitive_complexity_sum"].as_u64().is_some());
    for row in [function, module] {
        assert_eq!(row["halstead_metric_model_id"], "rqlens.rust_halstead");
        assert_eq!(row["halstead_metric_model_version"], 1);
        assert_eq!(row["halstead_status"], "measured");
        assert!(row["halstead"]["total_operators"].as_u64().is_some());
        assert_eq!(row["halstead_effort"], row["halstead"]["effort"]);
    }
    let correctness = read_json(analysis.join("correctness_review.json"))?;
    for key in ["version", "generated_from", "summary", "tests"] {
        assert!(correctness.get(key).is_some(), "missing {key}");
    }
    let map = read_json(analysis.join("map.json"))?;
    for key in ["meta", "graph", "modules", "measurement_confidence"] {
        assert!(map.get(key).is_some(), "missing {key}");
    }
    assert!(map["graph"]["nodes"].as_array().is_some());
    assert!(map["graph"]["edges"].as_array().is_some());

    Ok(())
}

fn generated_artifacts_match_envelope_conformance_snapshot(analysis: &Path) -> anyhow::Result<()> {
    let expected: Value = serde_json::from_str(include_str!("snapshots/artifact_envelopes.json"))
        .context("snapshot should parse")?;
    for (file, expected_keys) in expected.as_object().context("fixture value")? {
        let document = read_document(analysis.join(file))?;
        // Public schemas allow additive fields. Preserve required contract
        // fields without inventing a backwards-compatibility ban on extensions.
        let actual = document.as_object().context("artifact envelope")?;
        for key in json_array(expected_keys)? {
            let key = key.as_str().context("schema key")?;
            assert!(
                actual.contains_key(key),
                "missing contract field {file}:{key}"
            );
        }
        assert_eq!(document["schema_version"], 3);
    }
    Ok(())
}

fn normalized_measurements_match_golden_snapshot(analysis: &Path) -> anyhow::Result<()> {
    let hotspots = read_json(analysis.join("hotspots.json"))?;
    let map = read_json(analysis.join("map.json"))?;
    let normalized = serde_json::json!({
        "hotspots": hotspots.as_array().context("fixture value")?.iter()
            .filter(|row| row["kind"] == "module")
            .map(|row| serde_json::json!({
            "module_key": row["module_key"],
            "max_function_score": row["max_function_score"],
            "p95_function_score": row["p95_function_score"],
            "score": row["score"],
        })).collect::<Vec<_>>(),
        "dependency_edges": map["graph"]["edges"].as_array().context("fixture value")?.iter()
            .filter(|edge| edge["data"]["kind"] == "dependency")
            .map(|edge| serde_json::json!([
                edge["data"]["source"],
                edge["data"]["target"],
            ]))
            .collect::<Vec<_>>(),
    });
    let expected: Value = serde_json::from_str(include_str!("snapshots/mini_metrics.json"))
        .context("metric snapshot should parse")?;
    assert_eq!(normalized, expected);

    Ok(())
}

#[test]
fn golden_fixture_preserves_type_ownership_and_test_attribution() -> anyhow::Result<()> {
    let config = "tests/fixtures/golden_rust_project/rqlens.toml";
    let analysis = measure_fixture("type-health", config)?;
    measure_fixture("correctness", config)?;
    correctness_attributes_inline_tests_to_production_modules(&analysis)?;
    let payload = read_json(analysis.join("type_health.json"))?;
    let rows = json_array(&payload)?;
    let alpha = rows
        .iter()
        .find(|item| item["qualified_name"] == "alpha::Config")
        .context("fixture value")?;
    let beta = rows
        .iter()
        .find(|item| item["qualified_name"] == "beta::Config")
        .context("fixture value")?;
    // This fixture includes unexpanded item/test macros: preserve known joins,
    // but do not present source observations as complete type inventories.
    assert_eq!(alpha["observed_method_count"], 1);
    assert_eq!(beta["observed_method_count"], 2);
    for row in rows {
        assert!(row["method_count"].is_null());
        assert!(row["structural_risk"].is_null());
        assert_eq!(row["measurement_confidence"]["complete"], false);
    }
    assert!(rows.iter().any(|item| item["qualified_name"] == "domain::TupleConfig"
        && item["shape"] == "tuple"));
    let domain = rows
        .iter()
        .find(|item| item["qualified_name"] == "domain::Config")
        .context("fixture value")?;
    assert_eq!(domain["observed_method_count"], 1);
    assert_eq!(domain["observed_impl_file_count"], 1);
    assert!(
        domain["impl_files"][0]
            .as_str()
            .context("fixture value")?
            .ends_with("src/service.rs")
    );

    Ok(())
}

fn map_captures_direct_module_path_dependency(analysis: &Path) -> anyhow::Result<()> {
    let payload = read_json(analysis.join("map.json"))?;
    assert_eq!(payload["meta"]["project_name"], "mini-rust-project");
    assert_eq!(payload["meta"]["risk_model_id"], "rqlens.architecture_risk");
    assert_eq!(
        payload["meta"]["layer_ruleset"]["id"],
        "rqlens.generic_layers"
    );
    let edges = json_array(&payload["graph"]["edges"])?;
    assert!(edges.iter().any(|edge| {
        edge["data"]["source"] == "mini-rust-project::mini_rust_project::lib"
            && edge["data"]["target"] == "mini-rust-project::mini_rust_project::math"
    }));

    Ok(())
}

#[test]
fn map_distinguishes_missing_and_complete_artifact_evidence() -> anyhow::Result<()> {
    for (name, complete) in [
        ("arch-missing-artifacts", false),
        ("arch-complete-artifacts", true),
    ] {
        let fixture = write_architecture_fixture(name, complete)?;
        let config = fixture.join("rqlens.toml").to_string_lossy().to_string();
        let payload = read_json(measure_fixture("map", &config)?.join("map.json"))?;
        if complete {
            map_scores_git_cycles_and_layer_violations(&payload)?;
        } else {
            map_reports_missing_artifacts_as_unknown_metrics(&payload)?;
        }
    }
    Ok(())
}

fn map_reports_missing_artifacts_as_unknown_metrics(payload: &Value) -> anyhow::Result<()> {
    assert_eq!(
        payload["meta"]["summary"]["artifact_status"]["hotspots.json"]["status"],
        "missing"
    );
    assert_eq!(payload["measurement_confidence"]["partial"], true);
    let service = node_data(payload, "service")?;
    assert!(service["total_score"].is_null());
    assert!(
        json_array(&service["unknown_metrics"])?
            .iter()
            .any(|metric| metric == "maintainability_risk")
    );

    Ok(())
}

fn map_scores_git_cycles_and_layer_violations(payload: &Value) -> anyhow::Result<()> {
    assert_eq!(
        payload["meta"]["summary"]["artifact_status"]["hotspots.json"]["status"],
        "available"
    );
    let domain = node_data(payload, "domain")?;
    assert_eq!(domain["layer_violation_count"], 1);
    assert_eq!(domain["cycle_member"], true);
    assert!(
        domain["architectural_risk"]
            .as_f64()
            .context("fixture value")?
            >= 140.0
    );
    assert!(domain["total_score"].as_f64().is_some());
    assert!(
        domain["raw_facts"]["git_history"]["commit_count"]
            .as_u64()
            .is_some()
    );
    let service = node_data(payload, "service")?;
    assert!(service["total_score"].as_f64().is_some());
    assert!(json_array(&service["unknown_metrics"])?.is_empty());

    Ok(())
}

#[test]
fn entrypoints_are_visible_and_discounted_in_outputs() -> anyhow::Result<()> {
    let fixture = write_entrypoint_fixture("entrypoint-awareness")?;
    let config = fixture.join("rqlens.toml").to_string_lossy().to_string();
    for tool in [
        "hotspots",
        "correctness",
        "locality",
        "leverage",
        "clones",
        "map",
    ] {
        run(&["measure", tool, "--config", &config])?;
    }

    let analysis = fixture.join("target/analysis");
    validate_entrypoint_topology(&analysis)?;
    validate_entrypoint_clones(&analysis)?;
    validate_entrypoint_review(&fixture, &config, &analysis)
}

fn validate_entrypoint_topology(analysis: &Path) -> anyhow::Result<()> {
    let locality = read_json(analysis.join("locality_metrics.json"))?;
    let main_locality = json_array(&locality)?
        .iter()
        .find(|row| row["module_key"] == "main")
        .context("main entrypoint should have locality row")?;
    assert_eq!(main_locality["target_kind"], "bin");
    assert_eq!(main_locality["entrypoint_kind"], "bin");
    assert_eq!(main_locality["is_entrypoint"], true);

    let map = read_json(analysis.join("map.json"))?;
    let main = node_data(&map, "main")?;
    assert_eq!(main["target_kind"], "bin");
    assert_eq!(main["entrypoint_kind"], "bin");
    assert_eq!(main["is_entrypoint"], true);
    let custom = node_data(&map, "custom")?;
    assert_eq!(custom["target_kind"], "bin");
    assert_eq!(custom["entrypoint_kind"], "bin");
    assert_eq!(custom["is_entrypoint"], true);
    let custom_child = node_data(&map, "custom::child")?;
    assert_eq!(custom_child["target_name"], "custom-tool");
    assert_eq!(custom_child["module_key"], "custom::child");
    Ok(())
}

fn validate_entrypoint_clones(analysis: &Path) -> anyhow::Result<()> {
    let clones = read_json(analysis.join("clones.json"))?;
    let entrypoint_clone = json_array(&clones)?
        .iter()
        .find(|row| {
            row["engine"] == "module-responsibility"
                && row["is_entrypoint"] == true
                && row["target_kind"] == "bin"
        })
        .context("entrypoint responsibility duplication should be surfaced")?;
    assert!(
        json_array(&entrypoint_clone["instances"])?
            .iter()
            .any(|instance| instance["module_key"] == "main")
    );
    assert!(
        json_array(&entrypoint_clone["instances"])?
            .iter()
            .any(|instance| instance["module_key"] == "custom")
    );

    Ok(())
}

fn validate_entrypoint_review(fixture: &Path, config: &str, analysis: &Path) -> anyhow::Result<()> {
    let diff_file = fixture.join("entrypoint.diff");
    fs::write(
        &diff_file,
        "diff --git a/tools/custom.rs b/tools/custom.rs\n--- a/tools/custom.rs\n+++ b/tools/custom.rs\n@@ -1 +1 @@\n-old\n+new\n",
    )
    .context("entrypoint diff should be written")?;
    run(&[
        "review",
        "--diff-file",
        &diff_file.to_string_lossy(),
        "--config",
        config,
    ])?;
    let review = read_json(analysis.join("review.json"))?;
    assert!(
        json_array(&review["scope"]["entrypoints"])?
            .iter()
            .any(|entrypoint| {
                entrypoint["module_key"] == "custom"
                    && entrypoint["target_kind"] == "bin"
                    && entrypoint["entrypoint_kind"] == "bin"
            })
    );

    Ok(())
}

fn correctness_writes_review_and_test_catalog(root: &Path) -> anyhow::Result<()> {
    let review = read_json(root.join("correctness_review.json"))?;
    let catalog = read_json(root.join("test_catalog.json"))?;
    assert_eq!(review["summary"]["test_count"], 0);
    assert!(json_array(&catalog)?.is_empty());

    Ok(())
}

fn correctness_attributes_inline_tests_to_production_modules(
    analysis: &Path,
) -> anyhow::Result<()> {
    let review = read_json(analysis.join("correctness_review.json"))?;
    let domain_test = json_array(&review["tests"])?
        .iter()
        .find(|test| test["name"] == "macro_case_test")
        .context("domain test should be discovered")?;
    assert!(
        json_array(&domain_test["tested_modules"])?
            .iter()
            .any(|module| module == "domain")
    );
    let integration_test = json_array(&review["tests"])?
        .iter()
        .find(|test| test["path"] == "tests/duplicate_a.rs")
        .context("integration test should be discovered")?;
    assert!(
        json_array(&integration_test["tested_modules"])?
            .iter()
            .any(|module| module == "alpha")
    );

    Ok(())
}

#[test]
fn check_command_enforces_threshold_policy() -> anyhow::Result<()> {
    let root = write_architecture_fixture("threshold-policy", true)?;
    let config_path = root.join("rqlens.toml");
    let config = config_path.to_str().context("fixture value")?;
    let analysis = root.join("target/analysis");
    let mut map = read_document(analysis.join("api_health.json"))?;
    map.as_object_mut()
        .context("fixture value")?
        .remove("records");
    map["tool"] = serde_json::json!("map");
    map["measurement_confidence"] = serde_json::json!({"complete": true, "partial": false});
    map["data"] =
        serde_json::json!({"graph": {"nodes": [{"data": {"id": "lib", "total_score": 10.0}}]}});
    fs::write(
        analysis.join("map.json"),
        serde_json::to_vec(&map).context("fixture value")?,
    )?;
    let check = run(&[
        "check",
        "--fail-on",
        "threshold",
        "--fail-on",
        "regression",
        "--baseline",
        analysis.to_str().context("fixture value")?,
        "--max-total-score",
        "10000",
        "--config",
        config,
    ])?;
    let report: Value = serde_json::from_slice(&check.stdout)?;
    assert_eq!(report["passed"], true);
    assert!(report["evidence_deltas"].is_array());

    Ok(())
}

#[test]
fn validation_command_scores_ranked_outcome_labels() -> anyhow::Result<()> {
    let analysis = repo_root().join("target/test-fixtures/validation-analysis");
    fs::create_dir_all(&analysis).context("validation fixture should exist")?;
    fs::write(
        analysis.join("map.json"),
        serde_json::to_string(&serde_json::json!({
            "risk_model_version": 4,
            "data": {"graph": {"nodes": [
                {"data": {"id": "demo::lib::risky", "module_key": "risky", "total_score": 100.0}},
                {"data": {"id": "demo::lib::stable", "module_key": "stable", "total_score": 1.0}}
            ]}}
        }))
        .context("fixture value")?,
    )
    .context("validation map should be written")?;
    fs::write(
        analysis.join("repository_outcomes.json"),
        serde_json::to_string(&serde_json::json!({
            "data": {"records": [{"evidence_class": "reviewed-label", "modules": ["risky"]}]}
        }))
        .context("fixture value")?,
    )
    .context("validation outcomes should be written")?;
    let output_dir = repo_root().join("target/test-fixtures/validation-report");
    let project = format!("demo={}", analysis.display());
    run(&[
        "validate",
        "--project",
        &project,
        "--output-dir",
        &output_dir.to_string_lossy(),
    ])?;
    let report = read_document(output_dir.join("validation_report.json"))?;
    assert_eq!(report["summary"]["validated_project_count"], 1);
    assert_eq!(report["projects"][0]["metrics"]["recall_at_10"], 1.0);

    Ok(())
}

#[test]
fn telemetry_command_preserves_observed_operational_evidence() -> anyhow::Result<()> {
    let input = repo_root().join("target/test-fixtures/telemetry.json");
    if let Some(parent) = input.parent() {
        fs::create_dir_all(parent).context("telemetry fixture dir should exist")?;
    }
    fs::write(
        &input,
        serde_json::to_string(&serde_json::json!({
            "window": {"end": chrono::Utc::now().to_rfc3339()},
            "signals": [{
                "id": "api-errors", "kind": "error-rate", "status": "healthy",
                "value": 0.1, "unit": "percent", "source": "test-monitor"
            }]
        }))
        .context("fixture value")?,
    )
    .context("telemetry fixture should be written")?;
    run(&[
        "telemetry",
        "--input",
        &input.to_string_lossy(),
        "--config",
        "rqlens.toml",
    ])?;
    let document = read_document(repo_root().join("target/analysis/operational_evidence.json"))?;
    assert_eq!(document["summary"]["healthy"], 1);
    assert_eq!(
        document["data"]["records"][0]["evidence_class"],
        "operational-observed"
    );

    Ok(())
}

#[test]
fn evidence_commands_preserve_provenance_and_missing_evidence() -> anyhow::Result<()> {
    run(&["outcomes", "--config", "rqlens.toml"])?;
    let document = read_document(repo_root().join("target/analysis/repository_outcomes.json"))?;
    assert_eq!(document["tool"], "repository-outcomes");
    assert!(document["data"]["records"].is_array());
    run(&[
        "performance",
        "--no-run",
        "--config",
        "tests/fixtures/mini_rust_project/rqlens.toml",
    ])?;
    let document = read_document(
        repo_root().join("tests/fixtures/mini_rust_project/target/analysis/performance.json"),
    )?;
    assert_eq!(document["tool"], "performance");
    assert_eq!(document["measurement_confidence"]["partial"], true);

    Ok(())
}

fn hotspots_report_syntax_fact_confidence(analysis: &Path) -> anyhow::Result<()> {
    let payload = read_json(analysis.join("hotspots.json"))?;
    let rows = json_array(&payload)?;
    assert!(!rows.is_empty());
    let confidence = &rows[0]["measurement_confidence"];
    assert_eq!(confidence["complete"], true);
    assert_eq!(confidence["partial"], false);
    assert_eq!(confidence["confidence_scope"], "syntax_facts");
    assert_eq!(
        confidence["required_inputs"],
        serde_json::json!(["rust_source_files", "rust_syntax_facts"])
    );
    assert!(json_array(&confidence["missing_input"])?.is_empty());

    Ok(())
}

#[test]
fn halstead_counts_survive_the_helper_and_artifact_pipeline() -> anyhow::Result<()> {
    let root = fixture_root("halstead")?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"halstead\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n",
    )?;
    fs::write(
        root.join("rqlens.toml"),
        "project_name = \"halstead\"\nsource_roots = [\"src\"]\n[rust]\nidentity_resolution = \"disabled\"\n",
    )?;
    let source = "fn f(x: i32) -> i32 { x + x + 1 }";
    fs::write(root.join("src/lib.rs"), source)?;
    fs::write(root.join("src/empty.rs"), "// no tokens")?;
    let config = root.join("rqlens.toml");
    let config = config.to_str().context("fixture value")?;
    let expected = serde_json::json!({
        "distinct_operators": 6, "total_operators": 7,
        "distinct_operands": 4, "total_operands": 7,
        "vocabulary": 10, "length": 14,
        "volume": 14.0 * 10_f64.log2(), "difficulty": 5.25,
        "effort": 5.25 * (14.0 * 10_f64.log2()),
    });
    for text in [
        source,
        "/// ignored\n#[inline]\nfn f ( x: i32 ) -> i32 {\n // ignored +\n x + x + 1\n}",
    ] {
        fs::write(root.join("src/lib.rs"), text)?;
        run(&["measure", "hotspots", "--config", config])?;
        validate_halstead_report(&root, &expected)?;
    }
    Ok(())
}

fn validate_halstead_report(root: &Path, expected: &Value) -> anyhow::Result<()> {
    let payload = read_json(root.join("target/analysis/hotspots.json"))?;
    let rows = json_array(&payload)?;
    let with_paths = rows
        .iter()
        .map(|row| Ok((row, row["path"].as_str().context("hotspot path")?)))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let nonempty = with_paths
        .iter()
        .filter(|(_, path)| !path.ends_with("empty.rs"))
        .map(|(row, _)| *row)
        .collect::<Vec<_>>();
    assert_eq!(nonempty.len(), 2);
    for row in nonempty {
        assert_eq!(&row["halstead"], expected);
        assert_eq!(row["halstead_effort"], expected["effort"]);
        assert_eq!(row["halstead_status"], "measured");
        assert_eq!(
            row["halstead_scope"],
            if row["kind"] == "module" {
                "file"
            } else {
                "function_signature_and_body"
            }
        );
    }
    let empty = with_paths
        .iter()
        .find(|(_, path)| path.ends_with("empty.rs"))
        .map(|(row, _)| *row)
        .context("empty module hotspot")?;
    assert_eq!(empty["halstead_status"], "measured");
    assert_eq!(empty["halstead"]["length"], 0);
    assert_eq!(empty["halstead_effort"], 0.0);
    Ok(())
}

#[test]
fn syntax_failures_preserve_partial_source_evidence() -> anyhow::Result<()> {
    let fixture = write_syntax_error_fixture("partial-syntax-evidence")?;
    let config = fixture.join("rqlens.toml").to_string_lossy().to_string();
    run(&["measure", "hotspots", "--config", &config])?;

    let payload = read_json(fixture.join("target/analysis/hotspots.json"))?;
    let rows = payload.as_array().context("hotspots should be records")?;
    let row = rows
        .iter()
        .find(|row| row["kind"] == "module")
        .context("partial module row should be emitted")?;
    assert_eq!(row["evidence_status"], "partial");
    assert_eq!(row["syntax_backend"], "tree-sitter-rust");
    assert!(
        row["parse_status"]
            .as_str()
            .context("fixture value")?
            .starts_with("parse_error:")
    );
    assert_eq!(row["sloc"], 4);
    assert_eq!(row["cloc"], 2);
    assert!(row["score"].is_null());
    assert!(row["cyclomatic_complexity_sum"].as_u64().is_some());
    let function = rows
        .iter()
        .find(|row| row["function_name"] == "recovered")
        .context("tree-sitter should recover the valid function")?;
    assert_eq!(function["evidence_status"], "partial");
    assert_eq!(function["syntax_backend"], "tree-sitter-rust");
    assert_eq!(function["cyclomatic_complexity"], 2);
    assert_eq!(function["cognitive_complexity"], 2);
    assert!(function["score"].is_null());
    for partial in [row, function] {
        assert_eq!(partial["halstead_status"], "unavailable");
        assert!(partial.get("halstead").context("fixture value")?.is_null());
        assert!(
            partial
                .get("halstead_effort")
                .context("fixture value")?
                .is_null()
        );
    }
    assert_eq!(
        row["measurement_confidence"]["observed_inputs"]["rust_syntax_fact_files"],
        0
    );
    assert_eq!(
        row["measurement_confidence"]["observed_inputs"]["partial_source_metric_files"],
        1
    );
    assert_eq!(
        row["measurement_confidence"]["observed_inputs"]["tree_sitter_fallback_files"],
        1
    );

    Ok(())
}

#[test]
fn clones_report_source_scan_confidence_without_syntax_fact_requirements() -> anyhow::Result<()> {
    let fixture = write_clone_fixture("clone-confidence")?;
    let config = fixture.join("rqlens.toml").to_string_lossy().to_string();
    run(&["measure", "clones", "--config", &config])?;
    let payload = read_json(fixture.join("target/analysis/clones.json"))?;
    let rows = json_array(&payload)?;
    assert!(!rows.is_empty());
    let token_row = rows
        .iter()
        .find(|row| row["engine"] == "token")
        .context("token clone rows should be emitted")?;
    let token_confidence = &token_row["measurement_confidence"];
    assert_eq!(token_confidence["complete"], true);
    assert_eq!(token_confidence["partial"], false);
    assert_eq!(token_confidence["confidence_scope"], "source_scan");
    assert_eq!(
        token_confidence["required_inputs"],
        serde_json::json!(["rust_source_files"])
    );
    assert!(json_array(&token_confidence["missing_input"])?.is_empty());
    let ast_row = rows
        .iter()
        .find(|row| row["engine"] == "ast")
        .context("AST clone rows should be emitted")?;
    assert_eq!(
        ast_row["measurement_confidence"]["confidence_scope"],
        "syntax_facts"
    );
    let module_rows = rows
        .iter()
        .filter(|row| row["engine"] == "module-responsibility")
        .map(|row| {
            Ok((
                row,
                row["instances"]
                    .as_array()
                    .context("module clone instances")?,
            ))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let module_row = module_rows
        .into_iter()
        .find(|(_, instances)| {
            ["alpha", "beta"].iter().all(|name| {
                instances
                    .iter()
                    .any(|instance| instance["module_key"] == *name)
            })
        })
        .map(|(row, _)| row)
        .context("module responsibility duplication rows should be emitted")?;
    assert_eq!(
        module_row["risk_calibration"],
        "clones_module_responsibility"
    );
    let test_row = rows
        .iter()
        .find(|row| row["engine"] == "test-ast")
        .context("duplicated test body rows should be emitted")?;
    assert_eq!(test_row["risk_calibration"], "clones_test_ast");
    assert_eq!(
        test_row["measurement_confidence"]["confidence_scope"],
        "syntax_facts"
    );

    Ok(())
}
