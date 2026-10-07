use super::RunContext;
use crate::{test_support::source_project, tool::MeasureTool};
use serde_json::json;

#[test]
fn overlapping_scans_share_facts_without_dropping_test_only_files() -> anyhow::Result<()> {
    let (root, config) = source_project(Some("pub fn value() {}"))?;
    let shared = RunContext::new(&config, &[MeasureTool::Clones])?;
    assert_eq!(shared.source_facts.len(), 1);
    assert!(std::sync::Arc::ptr_eq(
        &shared.source_facts,
        &shared.correctness_facts
    ));
    std::fs::create_dir(root.path().join("tests"))?;
    std::fs::write(root.path().join("tests/extra.rs"), "#[test] fn extra() {}")?;
    let separate = RunContext::new(&config, &[MeasureTool::Clones])?;
    assert_eq!(separate.source_facts.len(), 1);
    assert_eq!(separate.correctness_facts.len(), 2);
    assert!(!std::sync::Arc::ptr_eq(
        &separate.source_facts,
        &separate.correctness_facts
    ));
    Ok(())
}

#[test]
fn clone_percentage_counts_unique_nonblank_source_lines() -> anyhow::Result<()> {
    let source = "fn one() {\n\n    \n    run();\n}\nfn two() {\n\n    run();\n}\n";
    let (_root, config) = source_project(Some(source))?;
    let context = RunContext::new(&config, &[MeasureTool::Hotspots])?;
    let instance = json!({"file_path": context.source_files[0], "start_line": 1,
        "end_line": 9, "snippet": source});
    let document = crate::contracts::artifact_document(
        &MeasureTool::Clones,
        &config,
        &context,
        json!([{"engine": "token", "instances": [instance, instance]},
            {"engine": "ast", "instances": [instance]}]),
    )?;
    assert_eq!(document["summary"]["duplicated_line_count"], 6);
    assert_eq!(document["summary"]["source_nonblank_line_count"], 6);
    assert_eq!(document["summary"]["duplication_percent"], 100.0);
    assert_eq!(document["summary"]["duplication_model_version"], 2);
    let empty = crate::contracts::artifact_document(
        &MeasureTool::Clones,
        &config,
        &RunContext::new(&config, &[])?,
        json!([]),
    )?;
    assert_eq!(empty["summary"]["duplication_percent"], 0.0);
    Ok(())
}

#[test]
fn run_context_reuses_inventory_graph_and_artifact_fingerprint() -> anyhow::Result<()> {
    let (root, config) = source_project(Some("pub fn value() {}"))?;
    // No helper invocation is necessary to verify snapshot and lazy-cache behavior.
    let context = RunContext::new(&config, &[])?;
    assert_eq!(context.source_files.len(), 1);
    assert!(std::ptr::eq(context.module_graph(), context.module_graph()));
    let first =
        crate::contracts::artifact_document(&MeasureTool::Hotspots, &config, &context, json!([]))?;
    let schema = crate::contracts::artifact_schemas(&MeasureTool::Hotspots);
    assert!(first.as_object().is_some_and(|fields| {
        fields
            .keys()
            .all(|key| schema["properties"].get(key).is_some())
    }));
    let map_schema = crate::contracts::artifact_schemas(&MeasureTool::Map);
    assert_eq!(
        map_schema["properties"]["data"]["properties"]["modules"]["items"]["type"],
        "object"
    );
    std::fs::write(root.path().join("src/new.rs"), "pub fn new() {}")?;
    let second =
        crate::contracts::artifact_document(&MeasureTool::Clones, &config, &context, json!([]))?;
    assert_eq!(first["input_fingerprint"], second["input_fingerprint"]);
    assert_eq!(context.source_files.len(), 1);
    assert!(context.snapshot.ensure_unchanged(&config).is_err());
    assert!(
        crate::contracts::artifact_document(
            &MeasureTool::Coverage,
            &config,
            &context,
            json!({"measurement_confidence": {"complete": true, "partial": true}})
        )
        .is_err()
    );
    Ok(())
}
