use anyhow::Result;
use std::fs;

use crate::config::LensConfig;
use crate::contracts::artifact_document;
use crate::facts::RunContext;
use crate::producers;
use crate::tool::MeasureTool;
use crate::util::write_json;

pub(super) fn run(tool: MeasureTool, config: LensConfig) -> Result<()> {
    fs::create_dir_all(&config.output_dir)?;
    let _output_lock = crate::util::lock_file(&config.output_dir.join("rqlens"))?;
    let tools = if matches!(tool, MeasureTool::All) {
        MeasureTool::all_tools()
    } else {
        vec![tool]
    };
    let context = RunContext::new(&config, &tools)?;
    for tool in tools {
        write_measurement(&tool, &config, &context)?;
    }
    Ok(())
}

fn write_measurement(tool: &MeasureTool, config: &LensConfig, context: &RunContext) -> Result<()> {
    let output = config.output_dir.join(tool.output_file());
    let payload = producers::produce_measurement(tool, config, context)?;
    if matches!(tool, MeasureTool::Correctness | MeasureTool::CorrectnessRun) {
        write_json(
            &config.output_dir.join("test_catalog.json"),
            &payload["tests"],
        )?;
    }
    write_json(&output, &artifact_document(tool, config, context, payload))?;
    println!(
        "Wrote {} visibility data to {}",
        tool.name(),
        output.display()
    );
    Ok(())
}
