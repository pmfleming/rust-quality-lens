#![allow(dead_code)]

use serde::Serialize;
use serde_json::{Value, json};

use crate::config::LensConfig;
use crate::facts::RunContext;
use crate::measurement::{MODEL_ID, MODEL_VERSION, source_confidence};
use crate::tool::MeasureTool;
use crate::util::project_input_fingerprint;

#[derive(Debug, Serialize)]
pub(crate) struct ArtifactEnvelope<T: Serialize> {
    pub(crate) version: u64,
    pub(crate) generated_from: &'static str,
    pub(crate) payload: T,
}

pub(crate) fn artifact_document(
    tool: &MeasureTool,
    config: &LensConfig,
    context: &RunContext,
    payload: Value,
) -> Value {
    let mut confidence = payload
        .get("measurement_confidence")
        .cloned()
        .unwrap_or_else(|| source_confidence(&config.source_roots, &context.source_facts));
    if matches!(
        tool,
        MeasureTool::ArchitectureRules
            | MeasureTool::Locality
            | MeasureTool::Leverage
            | MeasureTool::Map
    ) && let Some(object) = confidence.as_object_mut()
    {
        object.insert(
            "semantic_identity".to_string(),
            context.identity_resolution.to_json(),
        );
    }
    let mut document = json!({
        "schema_version": 2,
        "generated_from": "rqlens",
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "generator_version": env!("CARGO_PKG_VERSION"),
        "input_fingerprint": project_input_fingerprint(&config.project_root, &config.source_roots),
        "tool": tool.name(),
        "risk_model_id": MODEL_ID,
        "risk_model_version": MODEL_VERSION,
        "measurement_confidence": confidence,
        "summary": artifact_summary(tool, context, &payload),
    });
    if let Some(object) = document.as_object_mut() {
        object.insert(
            if payload.is_array() {
                "records"
            } else {
                "data"
            }
            .to_string(),
            payload,
        );
    }
    document
}

fn artifact_summary(tool: &MeasureTool, context: &RunContext, payload: &Value) -> Value {
    if let Some(records) = payload.as_array() {
        if matches!(tool, MeasureTool::Clones) {
            return clone_summary(records, context);
        }
        json!({"record_count": records.len()})
    } else {
        payload
            .get("summary")
            .or_else(|| payload.get("meta").and_then(|meta| meta.get("summary")))
            .cloned()
            .unwrap_or_else(|| json!({}))
    }
}

fn clone_summary(records: &[Value], context: &RunContext) -> Value {
    let mut duplicate_lines = std::collections::BTreeSet::new();
    for instance in records
        .iter()
        .filter(|record| record["engine"] == "token")
        .flat_map(|record| record["instances"].as_array().into_iter().flatten())
    {
        let (Some(path), Some(start), Some(end)) = (
            instance["file_path"].as_str(),
            instance["start_line"].as_u64(),
            instance["end_line"].as_u64(),
        ) else {
            continue;
        };
        for line in start..=end {
            duplicate_lines.insert((path.to_string(), line));
        }
    }
    let source_lines = context
        .source_facts
        .iter()
        .map(|fact| fact.source.source_nonblank_line_count)
        .sum::<usize>();
    let duplication_percent = if source_lines == 0 {
        0.0
    } else {
        crate::util::round2(duplicate_lines.len() as f64 / source_lines as f64 * 100.0)
    };
    json!({
        "record_count": records.len(),
        "token_record_count": records.iter().filter(|record| record["engine"] == "token").count(),
        "ast_record_count": records.iter().filter(|record| record["engine"] == "ast").count(),
        "duplicated_line_count": duplicate_lines.len(),
        "source_nonblank_line_count": source_lines,
        "duplication_percent": duplication_percent,
    })
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
    pub(crate) change_evidence: Value,
    pub(crate) measurements: Vec<ReviewMeasurement>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ReviewScope {
    pub(crate) changed_since: Option<String>,
    pub(crate) diff_file: Option<String>,
    pub(crate) changed_files: Vec<String>,
    pub(crate) changed_lines: Value,
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
            let schemas = MeasureTool::schema_tools()
                .into_iter()
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
    let payload = match tool {
        MeasureTool::Hotspots => array_schema(
            "hotspots.json",
            &[
                "name",
                "kind",
                "module_key",
                "score",
                "quality_score",
                "score_components",
                "measurement_confidence",
            ],
            json!({
                "name": {"type": "string"},
                "kind": {"type": "string", "enum": ["module", "function"]},
                "module_key": {"type": "string"},
                "module_id": {"type": "string"},
                "package_name": {"type": "string"},
                "target_name": {"type": "string"},
                "identity_backend": {"type": "string"},
                "score": {"type": ["number", "null"]},
                "quality_score": {"type": ["number", "null"]},
                "risk_level": {"type": "string", "enum": ["normal", "watch", "high", "extreme", "unknown"]},
                "cyclomatic_complexity": {"type": "integer", "minimum": 1},
                "cognitive_complexity": {"type": "integer", "minimum": 0},
                "cyclomatic_complexity_sum": {"type": ["integer", "null"], "minimum": 0},
                "cyclomatic_complexity_average": {"type": ["number", "null"], "minimum": 0},
                "cyclomatic_complexity_max": {"type": ["integer", "null"], "minimum": 0},
                "cognitive_complexity_sum": {"type": ["integer", "null"], "minimum": 0},
                "cognitive_complexity_average": {"type": ["number", "null"], "minimum": 0},
                "cognitive_complexity_max": {"type": ["integer", "null"], "minimum": 0},
                "complexity_metric_model_id": {"type": "string", "const": "rqlens.rust_standard_complexity"},
                "complexity_metric_model_version": {"type": "integer", "const": 1},
                "parse_status": {"type": "string"},
                "syntax_backend": {"type": "string", "enum": ["tree-sitter-rust", "text"]},
                "syntax_error_count": {"type": "integer", "minimum": 0},
                "evidence_status": {"type": "string", "enum": ["partial"]},
                "score_components": {"type": "array", "items": {"type": "object"}},
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
        MeasureTool::Reliability => array_schema(
            "reliability_findings.json",
            &["rule_id", "category", "severity", "path", "line", "message"],
            json!({
                "rule_id": {"type": "string"},
                "category": {"type": "string", "const": "static-finding"},
                "kind": {"type": "string"},
                "severity": {"type": "string", "enum": ["error", "warning", "advisory"]},
                "path": {"type": "string"},
                "line": {"type": "integer"},
                "module_key": {"type": "string"},
                "module_id": {"type": "string"},
                "scope": {"type": "string", "enum": ["production", "test"]},
                "message": {"type": "string"},
                "source": {"type": "string"},
                "measurement_confidence": measurement_confidence_schema(),
            }),
        ),
        MeasureTool::ArchitectureRules => object_schema(
            "architecture_rules.json",
            &["summary", "violations", "measurement_confidence"],
            json!({
                "summary": {"type": "object"},
                "violations": {"type": "array", "items": {"type": "object"}},
                "measurement_confidence": measurement_confidence_schema(),
            }),
        ),
        MeasureTool::TestQuality => object_schema(
            "test_quality.json",
            &["summary", "tests", "findings", "measurement_confidence"],
            json!({
                "summary": {"type": "object"},
                "tests": {"type": "array", "items": {"type": "object"}},
                "findings": {"type": "array", "items": {"type": "object"}},
                "measurement_confidence": measurement_confidence_schema(),
            }),
        ),
        MeasureTool::ApiHealth => array_schema(
            "api_health.json",
            &[
                "module_key",
                "path",
                "public_item_count",
                "documented_public_item_count",
                "documentation_percent",
            ],
            json!({
                "module_key": {"type": "string"},
                "module_id": {"type": "string"},
                "path": {"type": "string"},
                "public_item_count": {"type": "integer"},
                "visibility_scope": {"type": "string", "const": "syntactic_pub"},
                "documented_public_item_count": {"type": "integer"},
                "missing_documentation_count": {"type": "integer"},
                "documentation_percent": {"type": "number"},
                "crate_level_documentation": {"type": ["boolean", "null"]},
                "signals": {"type": "array", "items": {"type": "string"}},
                "measurement_confidence": measurement_confidence_schema(),
            }),
        ),
        MeasureTool::SemanticApi => object_schema(
            "semantic_api.json",
            &["summary", "items", "measurement_confidence"],
            json!({
                "summary": {"type": "object"},
                "items": {"type": "array", "items": {"type": "object"}},
                "command": {"type": "object"},
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
        MeasureTool::Coverage => object_schema(
            "coverage.json",
            &["summary", "files", "measurement_confidence"],
            json!({
                "summary": {"type": "object"},
                "files": {"type": "array", "items": {"type": "object"}},
                "measurement_confidence": measurement_confidence_schema(),
            }),
        ),
        MeasureTool::FunctionRisk => object_schema(
            "function_risk.json",
            &["summary", "formula", "functions", "measurement_confidence"],
            json!({
                "summary": {"type": "object"},
                "formula": {"type": "string"},
                "functions": {"type": "array", "items": {"type": "object"}},
                "measurement_confidence": measurement_confidence_schema(),
            }),
        ),
        MeasureTool::Practices => object_schema(
            "rust_practices.json",
            &[
                "version",
                "profile",
                "summary",
                "checks",
                "measurement_confidence",
            ],
            json!({
                "version": {"type": "integer"},
                "profile": {"type": "string"},
                "summary": {"type": "object"},
                "checks": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["rule_id", "category", "severity", "status", "source"],
                        "properties": {
                            "rule_id": {"type": "string"},
                            "category": {"type": "string", "enum": ["verified-gate", "static-finding", "heuristic-signal"]},
                            "severity": {"type": "string", "enum": ["error", "warning", "advisory"]},
                            "status": {"type": "string", "enum": ["passed", "failed", "unavailable", "timed-out", "skipped"]},
                            "source": {"type": "string"},
                            "evidence": {"type": "object"}
                        },
                        "additionalProperties": true
                    }
                },
                "measurement_confidence": measurement_confidence_schema(),
            }),
        ),
        MeasureTool::All => unreachable!(),
    };
    envelope_schema(tool, payload)
}

fn envelope_schema(tool: &MeasureTool, payload: Value) -> Value {
    let array_payload = payload["type"] == "array";
    let payload_key = if array_payload { "records" } else { "data" };
    let mut properties = serde_json::Map::new();
    properties.insert(
        "schema_version".to_string(),
        json!({"type": "integer", "const": 2}),
    );
    properties.insert("generated_from".to_string(), json!({"type": "string"}));
    properties.insert(
        "generated_at".to_string(),
        json!({"type": "string", "format": "date-time"}),
    );
    properties.insert("generator_version".to_string(), json!({"type": "string"}));
    properties.insert(
        "input_fingerprint".to_string(),
        json!({
            "type": "object",
            "required": ["algorithm", "digest", "file_count", "complete"],
            "properties": {
                "algorithm": {"type": "string"},
                "digest": {"type": "string"},
                "file_count": {"type": "integer"},
                "complete": {"type": "boolean"},
                "read_errors": {"type": "array", "items": {"type": "string"}}
            }
        }),
    );
    properties.insert(
        "tool".to_string(),
        if matches!(tool, MeasureTool::Correctness | MeasureTool::CorrectnessRun) {
            json!({"type": "string", "enum": ["correctness", "correctness-run"]})
        } else {
            json!({"type": "string", "const": tool.name()})
        },
    );
    properties.insert("risk_model_id".to_string(), json!({"type": "string"}));
    properties.insert("risk_model_version".to_string(), json!({"type": "integer"}));
    properties.insert(
        "measurement_confidence".to_string(),
        measurement_confidence_schema(),
    );
    properties.insert("summary".to_string(), json!({"type": "object"}));
    properties.insert(payload_key.to_string(), payload);
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": format!("{} artifact envelope", tool.output_file()),
        "type": "object",
        "required": ["schema_version", "generated_from", "tool", "measurement_confidence", "summary", payload_key],
        "properties": properties,
        "additionalProperties": false,
    })
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
