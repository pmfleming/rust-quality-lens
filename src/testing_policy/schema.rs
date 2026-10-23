use serde_json::{Value, json};

pub(crate) fn config_schema() -> Value {
    json!({
        "type": "object", "additionalProperties": false, "required": ["manifest"],
        "properties": {
            "manifest": {"type": "string", "minLength": 1, "description": "Project-relative version-1 testing policy TOML; automatically fingerprinted."},
            "evidence_inputs": {"type": "array", "items": {"type": "string", "minLength": 1}, "default": [], "description": "Project-relative requirement/support documents and non-Rust fixtures to fingerprint. Missing inputs make evidence partial."}
        }
    })
}

pub(crate) fn report_schema() -> Value {
    json!({
        "type": "object", "additionalProperties": false,
        "required": ["version", "status", "manifest", "execution_evidence", "behaviors", "limitations"],
        "properties": {
            "version": {"const": 1},
            "status": {"enum": ["not_configured", "unavailable", "reviewed"]},
            "manifest": {"type": ["string", "null"]},
            "execution_evidence": {"type": "string"},
            "limitations": strings(),
            "behaviors": {"type": "array", "items": {
                "type": "object", "additionalProperties": false,
                "required": ["declared", "observed", "status"],
                "properties": {
                    "declared": behavior_schema(),
                    "status": {"enum": ["unresolved", "observed_failure", "observed_pass", "unknown"]},
                    "observed": {"type": "array", "items": {
                        "type": "object", "additionalProperties": false,
                        "required": ["test", "resolution", "execution"],
                        "properties": {"test": selector_schema(), "resolution": {"enum": ["unresolved", "resolved", "ambiguous"]}, "execution": {"type": "string"}}
                    }}
                }
            }}
        }
    })
}

fn strings() -> Value {
    json!({"type": "array", "items": {"type": "string"}})
}

fn selector_schema() -> Value {
    json!({"type": "object", "additionalProperties": false,
        "required": ["package", "target", "path", "name"],
        "properties": {"package": {"type": "string", "minLength": 1}, "target": {"type": "string", "minLength": 1}, "path": {"type": "string", "minLength": 1}, "name": {"type": "string", "minLength": 1}}
    })
}

fn behavior_schema() -> Value {
    json!({"type": "object", "additionalProperties": false,
        "required": ["id", "description", "risks", "requirement", "scope", "expectations", "real_boundaries", "reviewer", "review_revision", "owner"],
        "properties": {
            "id": {"type": "string", "minLength": 1}, "description": {"type": "string", "minLength": 1},
            "risks": strings(), "requirement": {"type": "string", "minLength": 1},
            "scope": {"type": "string", "minLength": 1}, "expectations": strings(),
            "real_boundaries": strings(), "substituted_boundaries": strings(),
            "reviewer": {"type": "string", "minLength": 1}, "review_revision": {"type": "string", "minLength": 1},
            "required": {"type": "boolean", "default": false},
            "owner": {"type": "object", "additionalProperties": false,
                "required": ["role", "rationale", "tests"], "properties": {
                    "role": {"enum": ["end_to_end", "integration", "contract", "local"]},
                    "rationale": {"type": "string", "minLength": 1},
                    "tests": {"type": "array", "items": selector_schema()}
                }
            },
            "supporting": {"type": "array", "items": {
                "type": "object", "additionalProperties": false, "required": ["test", "distinct_risk"],
                "properties": {"test": selector_schema(), "distinct_risk": {"type": "string"}}
            }}
        }
    })
}
