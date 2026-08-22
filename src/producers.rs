use anyhow::Result;
use serde_json::{Value, json};

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

fn with_fact_identity(fact: &crate::facts::FileFacts, mut record: Value) -> Value {
    if let Some(object) = record.as_object_mut() {
        object.insert("module_id".to_string(), json!(fact.module_id));
        object.insert("package_name".to_string(), json!(fact.package_name));
        object.insert("target_name".to_string(), json!(fact.target_name));
        object.insert("identity_backend".to_string(), json!(fact.identity_backend));
    }
    record
}

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
        MeasureTool::Practices => practices::produce(config, context),
        MeasureTool::All => unreachable!(),
    }
}
