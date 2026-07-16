use serde_json::Value;

mod confidence;
mod paths;
mod provenance;
mod scoring;

pub(crate) use confidence::{source_confidence, source_scan_confidence_with_errors};
pub(crate) use paths::{
    classify_module, classify_path, layer_color, module_for_path, project_relative_path,
    test_kind_for_path, title_from_name,
};
pub(crate) use provenance::{Provenance, ToolScoreMetadata, provenance, tool_score_metadata};
pub(crate) use scoring::{
    escape_labels, escape_weights, risk_model_classification, risk_model_tool_scores,
    risk_model_weights, type_risk,
};

pub(crate) const MODEL_ID: &str = "rqlens.architecture_risk";
pub(crate) const MODEL_VERSION: u64 = 4;
pub(crate) const RULESET_ID: &str = "rqlens.generic_layers";
pub(crate) const RULESET_VERSION: u64 = 1;

pub(crate) fn option_json(value: Option<f64>) -> Value {
    value.map(Value::from).unwrap_or(Value::Null)
}
