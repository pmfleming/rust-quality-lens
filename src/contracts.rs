#![allow(dead_code)]

use serde::Serialize;
use serde_json::{Value, json};

use crate::MeasureTool;

#[derive(Debug, Serialize)]
pub(crate) struct ArtifactEnvelope<T: Serialize> {
    pub(crate) version: u64,
    pub(crate) generated_from: &'static str,
    pub(crate) payload: T,
}

#[derive(Debug, Serialize)]
pub(crate) struct MeasurementConfidence {
    pub(crate) complete: bool,
    pub(crate) partial: bool,
    pub(crate) confidence_scope: String,
    pub(crate) required_inputs: Vec<String>,
    pub(crate) observed_inputs: Value,
    pub(crate) missing_input: Vec<String>,
    pub(crate) stale_input: Vec<String>,
    pub(crate) unsupported_pattern: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ReviewOutput {
    pub(crate) version: u64,
    pub(crate) generated_from: &'static str,
    pub(crate) scope: ReviewScope,
    pub(crate) measurements: Vec<ReviewMeasurement>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ReviewScope {
    pub(crate) changed_since: Option<String>,
    pub(crate) diff_file: Option<String>,
    pub(crate) changed_files: Vec<String>,
    pub(crate) measured_rust_files: Vec<String>,
    pub(crate) entrypoints: Vec<ReviewEntrypoint>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ReviewEntrypoint {
    pub(crate) path: String,
    pub(crate) module_key: String,
    pub(crate) target_kind: String,
    pub(crate) entrypoint_kind: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ReviewMeasurement {
    pub(crate) tool: String,
    pub(crate) output_file: String,
    pub(crate) payload: Value,
}

pub(crate) fn artifact_schemas(tool: &MeasureTool) -> Value {
    match tool {
        MeasureTool::All => {
            let schemas = MeasureTool::all_tools()
                .into_iter()
                .filter(|tool| !matches!(tool, MeasureTool::CorrectnessRun))
                .map(|tool| {
                    (
                        tool.output_file().to_string(),
                        artifact_schema_for_tool(&tool),
                    )
                })
                .collect::<serde_json::Map<_, _>>();
            json!({
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "title": "rust-quality-lens artifact schemas",
                "type": "object",
                "properties": schemas,
            })
        }
        _ => artifact_schema_for_tool(tool),
    }
}

fn artifact_schema_for_tool(tool: &MeasureTool) -> Value {
    match tool {
        MeasureTool::Hotspots => array_schema(
            "hotspots.json",
            &[
                "name",
                "module_key",
                "score",
                "quality_score",
                "measurement_confidence",
            ],
            json!({
                "name": {"type": "string"},
                "module_key": {"type": "string"},
                "score": {"type": "number"},
                "quality_score": {"type": "number"},
                "measurement_confidence": measurement_confidence_schema(),
            }),
        ),
        MeasureTool::Clones => array_schema(
            "clones.json",
            &[
                "engine",
                "hash",
                "score",
                "instances",
                "measurement_confidence",
            ],
            json!({
                "engine": {"type": "string", "enum": ["token", "ast", "module-responsibility", "test-ast"]},
                "hash": {"type": "string"},
                "score": {"type": "number"},
                "instances": {
                    "type": "array",
                    "items": {"type": "object"}
                },
                "measurement_confidence": measurement_confidence_schema(),
            }),
        ),
        MeasureTool::EscapeHatches => array_schema(
            "rust_escape_hatches.json",
            &[
                "module_key",
                "path",
                "escape_hatch_score",
                "total_count",
                "measurement_confidence",
            ],
            json!({
                "module_key": {"type": "string"},
                "path": {"type": "string"},
                "escape_hatch_score": {"type": "number"},
                "total_count": {"type": "integer"},
                "counts": {"type": "object"},
                "measurement_confidence": measurement_confidence_schema(),
            }),
        ),
        MeasureTool::TypeHealth => array_schema(
            "type_health.json",
            &[
                "type_name",
                "qualified_name",
                "module_key",
                "structural_risk",
            ],
            json!({
                "type_name": {"type": "string"},
                "qualified_name": {"type": "string"},
                "module_key": {"type": "string"},
                "structural_risk": {"type": "number"},
                "structural_score": {"type": "number"},
                "signals": {"type": "array", "items": {"type": "string"}},
            }),
        ),
        MeasureTool::Correctness | MeasureTool::CorrectnessRun => object_schema(
            "correctness_review.json",
            &["version", "generated_from", "summary", "tests"],
            json!({
                "version": {"type": "integer"},
                "generated_from": {"type": "string"},
                "summary": {"type": "object"},
                "measurement_confidence": measurement_confidence_schema(),
                "layers": {"type": "array", "items": {"type": "object"}},
                "tests": {"type": "array", "items": {"type": "object"}},
            }),
        ),
        MeasureTool::Locality => array_schema(
            "locality_metrics.json",
            &["module_key", "path", "locality_risk", "locality_score"],
            json!({
                "module_key": {"type": "string"},
                "path": {"type": ["string", "object"]},
                "target_kind": {"type": "string"},
                "entrypoint_kind": {"type": ["string", "null"]},
                "is_entrypoint": {"type": "boolean"},
                "locality_risk": {"type": "number"},
                "locality_score": {"type": "number"},
            }),
        ),
        MeasureTool::Leverage => array_schema(
            "leverage_metrics.json",
            &["module_key", "path", "leverage_score", "pressure_score"],
            json!({
                "module_key": {"type": "string"},
                "path": {"type": ["string", "object"]},
                "target_kind": {"type": "string"},
                "entrypoint_kind": {"type": ["string", "null"]},
                "is_entrypoint": {"type": "boolean"},
                "leverage_score": {"type": "number"},
                "pressure_score": {"type": "number"},
            }),
        ),
        MeasureTool::Map => object_schema(
            "map.json",
            &["meta", "graph", "modules", "measurement_confidence"],
            json!({
                "meta": {"type": "object"},
                "graph": {
                    "type": "object",
                    "required": ["nodes", "edges"],
                    "properties": {
                        "nodes": {"type": "array", "items": {"type": "object"}},
                        "edges": {"type": "array", "items": {"type": "object"}}
                    }
                },
                "modules": {"type": "array", "items": {"type": "string"}},
                "measurement_confidence": measurement_confidence_schema(),
            }),
        ),
        MeasureTool::All => unreachable!(),
    }
}

fn array_schema(title: &str, required: &[&str], properties: Value) -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": title,
        "type": "array",
        "items": {
            "type": "object",
            "required": required,
            "properties": properties,
            "additionalProperties": true,
        }
    })
}

fn object_schema(title: &str, required: &[&str], properties: Value) -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": title,
        "type": "object",
        "required": required,
        "properties": properties,
        "additionalProperties": true,
    })
}

fn measurement_confidence_schema() -> Value {
    json!({
        "type": "object",
        "required": ["complete", "partial", "confidence_scope"],
        "properties": {
            "complete": {"type": "boolean"},
            "partial": {"type": "boolean"},
            "confidence_scope": {"type": "string"},
            "required_inputs": {"type": "array", "items": {"type": "string"}},
            "observed_inputs": {"type": "object"},
            "missing_input": {"type": "array", "items": {"type": "string"}},
            "stale_input": {"type": "array", "items": {"type": "string"}},
            "unsupported_pattern": {"type": "array", "items": {"type": "string"}}
        },
        "additionalProperties": true,
    })
}
