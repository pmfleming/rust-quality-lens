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
fn run_context_reuses_inventory_graph_and_artifact_fingerprint() -> anyhow::Result<()> {
    let (root, config) = source_project(Some("pub fn value() {}"))?;
    // No helper invocation is necessary to verify snapshot and lazy-cache behavior.
    let context = RunContext::new(&config, &[])?;
    assert_eq!(context.source_files.len(), 1);
    assert!(std::ptr::eq(context.module_graph(), context.module_graph()));
    let first =
        crate::contracts::artifact_document(&MeasureTool::Hotspots, &config, &context, json!([]))?;
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
