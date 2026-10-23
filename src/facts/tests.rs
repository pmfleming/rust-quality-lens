use super::RunContext;
use crate::{test_support::source_project, tool::MeasureTool};
use serde_json::json;

#[test]
fn overlapping_scans_share_facts_without_dropping_test_only_files() -> anyhow::Result<()> {
    let (root, config) = source_project(Some("pub fn value() {}"))?;
    let shared = RunContext::new(&config, &[MeasureTool::Clones])?;
    assert_eq!(shared.source_facts.len(), 1);
    assert_eq!(shared.correctness_facts.len(), 1);
    std::fs::create_dir(root.path().join("tests"))?;
    std::fs::write(root.path().join("tests/extra.rs"), "#[test] fn extra() {}")?;
    let separate = RunContext::new(&config, &[MeasureTool::Clones])?;
    assert_eq!(separate.source_facts.len(), 1);
    assert_eq!(separate.correctness_facts.len(), 2);
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
