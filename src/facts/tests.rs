use super::*;
use serde_json::json;

#[test]
fn run_context_reuses_inventory_graph_and_artifact_fingerprint() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    std::fs::create_dir(root.path().join("src"))?;
    std::fs::write(root.path().join("src/lib.rs"), "pub fn value() {}")?;
    let path = root.path().join("rqlens.toml");
    std::fs::write(&path, "source_roots = [\"src\"]\n")?;
    let config = LensConfig::load(Some(path))?;
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
