use serde_json::{Value, json};

pub(super) fn schema() -> Value {
    let text = json!({"type": "string", "minLength": 1});
    let origin = super::origin_schema();
    json!({"type": "object", "additionalProperties": false,
        "required": ["model_version", "crate_id", "modules", "declarations", "references", "covered_categories", "limitations"],
        "properties": {
            "model_version": {"const": 1}, "crate_id": text,
            "covered_categories": {"type": "array", "uniqueItems": true, "items": {"enum": super::architecture::CATEGORIES}},
            "limitations": {"type": "array", "items": {"type": "string"}},
            "excluded_compiler_infrastructure": {"type": "array", "uniqueItems": true, "items": {"enum": ["standard_library_injection", "test_harness", "proc_macro_harness"]}},
            "modules": {"type": "array", "minItems": 1, "items": {
                "type": "object", "additionalProperties": false, "required": ["id", "path", "parent_id", "origin"],
                "properties": {"id": text, "path": {"type": "string"}, "parent_id": {"type": ["string", "null"]}, "origin": origin}}},
            "declarations": {"type": "array", "items": {
                "type": "object", "additionalProperties": false, "required": ["id", "module_id", "kind", "origin"],
                "properties": {"id": text, "module_id": text, "kind": text, "origin": origin}}},
            "references": {"type": "array", "items": {
                "type": "object", "additionalProperties": false, "required": ["source_id", "target", "kind", "origin"],
                "properties": {"source_id": text, "kind": {"enum": ["import", "type", "trait", "body", "method"]}, "origin": origin,
                    "target": {"oneOf": [
                        {"type": "object", "additionalProperties": false, "required": ["status", "declaration_id"],
                            "properties": {"status": {"const": "local"}, "declaration_id": text}},
                        {"type": "object", "additionalProperties": false, "required": ["status", "crate_id", "crate_name", "definition_id", "module_path", "module_id"],
                            "properties": {"status": {"const": "external"}, "crate_id": text, "crate_name": text, "definition_id": text, "module_path": {"type": "string"}, "module_id": text}},
                        {"type": "object", "additionalProperties": false, "required": ["status", "reason"],
                            "properties": {"status": {"const": "unknown"}, "reason": text}}
                    ]}
                }}}
        }
    })
}
