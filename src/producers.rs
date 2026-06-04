use anyhow::Result;
use serde_json::Value;

use crate::MeasureTool;
use crate::config::LensConfig;
use crate::facts::RunContext;

mod clones;
mod correctness;
mod escape_hatches;
mod hotspots;
mod map;
mod topology;
mod type_health;

pub(crate) fn produce_measurement(
    tool: &MeasureTool,
    config: &LensConfig,
    context: &RunContext,
) -> Result<Value> {
    match tool {
        MeasureTool::Hotspots => hotspots::produce(config, context),
        MeasureTool::Clones => clones::produce(config, context),
        MeasureTool::EscapeHatches => escape_hatches::produce(config, context),
        MeasureTool::TypeHealth => type_health::produce(config, context),
        MeasureTool::Correctness => correctness::produce(config, context, false),
        MeasureTool::CorrectnessRun => correctness::produce(config, context, true),
        MeasureTool::Locality => topology::locality(context),
        MeasureTool::Leverage => topology::leverage(context),
        MeasureTool::Map => map::produce(config, context),
        MeasureTool::All => unreachable!(),
    }
}
