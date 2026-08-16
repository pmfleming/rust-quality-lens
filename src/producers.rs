use anyhow::Result;
use serde_json::Value;

use crate::config::LensConfig;
use crate::facts::RunContext;
use crate::tool::MeasureTool;

mod api_health;
mod architecture_rules;
mod clones;
mod cohesion;
mod correctness;
mod coverage;
mod escape_hatches;
mod function_risk;
mod hotspots;
mod map;
mod practices;
mod reliability;
mod semantic_api;
mod test_quality;
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
        MeasureTool::Reliability => reliability::produce(config, context),
        MeasureTool::ArchitectureRules => architecture_rules::produce(config, context),
        MeasureTool::TestQuality => test_quality::produce(config, context),
        MeasureTool::ApiHealth => api_health::produce(config, context),
        MeasureTool::SemanticApi => semantic_api::produce(config),
        MeasureTool::Correctness => correctness::produce(config, context, false),
        MeasureTool::CorrectnessRun => correctness::produce(config, context, true),
        MeasureTool::Locality => topology::locality(context),
        MeasureTool::Leverage => topology::leverage(context),
        MeasureTool::ModuleCohesion => cohesion::produce(config, context),
        MeasureTool::Map => map::produce(config, context),
        MeasureTool::Coverage => coverage::produce(config, context),
        MeasureTool::FunctionRisk => function_risk::produce(config, context),
        MeasureTool::Practices => practices::produce(config),
        MeasureTool::All => unreachable!(),
    }
}
