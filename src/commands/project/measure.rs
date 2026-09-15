use anyhow::Result;
use std::fs;

use crate::config::LensConfig;
use crate::contracts::artifact_document;
use crate::facts::RunContext;
use crate::measurement::ArtifactBatch;
use crate::producers;
use crate::tool::MeasureTool;

pub(super) fn run(tool: MeasureTool, config: LensConfig) -> Result<()> {
    fs::create_dir_all(&config.output_dir)?;
    let _output_lock = crate::util::lock_file(&config.output_dir.join("rqlens"))?;
    let tools = if matches!(tool, MeasureTool::All) {
        MeasureTool::all_tools()
    } else {
        vec![tool]
    };
    let context = RunContext::new(&config, &tools)?;
    let mut batch = ArtifactBatch::new(&config)?;
    for tool in &tools {
        let payload = producers::produce_measurement(tool, &batch.config, &context)?;
        if matches!(tool, MeasureTool::Correctness | MeasureTool::CorrectnessRun) {
            batch.write("test_catalog.json", &payload["tests"])?;
        }
        let document = artifact_document(tool, &config, &context, payload)?;
        batch.write(tool.output_file(), &document)?;
    }
    batch.publish(&context.snapshot, &config)?;
    for tool in tools {
        println!(
            "Wrote {} visibility data to {}",
            tool.name(),
            config.output_dir.join(tool.output_file()).display()
        );
    }
    Ok(())
}
