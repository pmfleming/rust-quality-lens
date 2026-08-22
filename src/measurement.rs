use serde_json::{Value, json};

mod confidence;
mod paths;
mod provenance;
mod scoring;

pub(crate) use confidence::{source_confidence, source_scan_confidence_with_errors};
pub(crate) use paths::{
    classify_module, classify_path, module_for_path, project_relative_path, test_kind_for_path,
    title_from_name,
};
pub(crate) use provenance::{Provenance, ToolScoreMetadata, provenance, tool_score_metadata};
pub(crate) use scoring::{escape_labels, escape_weights, type_risk};

pub(crate) const MODEL_ID: &str = "rqlens.architecture_risk";
pub(crate) const MODEL_VERSION: u64 = 4;
pub(crate) const RULESET_ID: &str = "rqlens.generic_layers";
pub(crate) const RULESET_VERSION: u64 = 1;

pub(crate) fn option_json(value: Option<f64>) -> Value {
    value.map(Value::from).unwrap_or(Value::Null)
}

pub(crate) fn map_model_metadata() -> Value {
    json!({
        "risk_model_id": MODEL_ID,
        "risk_model_version": MODEL_VERSION,
        "risk_model_weights": scoring::risk_model_weights(),
        "risk_model_tool_scores": scoring::risk_model_tool_scores(),
        "risk_model_classification": scoring::risk_model_classification(),
        "layer_ruleset": {"id": RULESET_ID, "version": RULESET_VERSION},
    })
}

pub(crate) fn map_layer_color(layer: &str) -> &'static str {
    paths::layer_color(layer)
}
