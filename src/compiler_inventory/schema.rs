use crate::contracts::schema::property_groups;
use serde_json::{Value, json};

pub(crate) fn origin_schema() -> Value {
    json!({
        "type": "object", "additionalProperties": false,
        "required": ["generated", "location_kind", "location"],
        "properties": {
            "generated": {"type": "boolean"},
            "location_kind": {"enum": ["authored", "invocation_anchor", "unavailable"]},
            "location": {
                "type": ["object", "null"], "additionalProperties": false,
                "required": ["path", "line", "column", "end_line", "end_column"],
                "properties": property_groups(json!({"path": {"type": "string", "minLength": 1}}), &[
                    ("line column end_line end_column", json!({"type": "integer", "minimum": 1})),
                ])
            }
        }
    })
}

pub(crate) fn context_schema() -> Value {
    json!({
        "type": "object", "additionalProperties": false,
        "required": ["manifest_dir", "source_root", "target_triple", "test", "cfg", "compiler_version"],
        "properties": property_groups(json!({
            "test": {"type": "boolean"},
            "cfg": {"type": "array", "minItems": 1, "uniqueItems": true, "items": {"type": "string"}},
            "compiler_version": {"const": super::COMPILER_VERSION}
        }), &[("manifest_dir source_root target_triple", json!({"type": "string", "minLength": 1}))])
    })
}

pub(crate) fn profiles_schema() -> Value {
    let mut context = context_schema();
    let properties = &mut context["properties"];
    for (key, value) in [
        ("schema_version", json!({"const": 1})),
        ("backend", json!({"const": "rustc-hir"})),
        ("package_name", json!({"type": "string", "minLength": 1})),
        ("crate_name", json!({"type": "string", "minLength": 1})),
        (
            "types",
            definitions(
                &[
                    "id",
                    "qualified_name",
                    "name",
                    "kind",
                    "field_count",
                    "variant_count",
                    "variant_field_count",
                    "origin",
                ],
                json!({
                    "name": {"type": "string", "minLength": 1},
                    "qualified_name": {"type": "string", "minLength": 1},
                    "kind": {"enum": ["struct", "enum", "union"]},
                    "field_count": {"type": "integer", "minimum": 0},
                    "variant_count": {"type": "integer", "minimum": 0},
                    "variant_field_count": {"type": "integer", "minimum": 0}
                }),
            ),
        ),
        (
            "impls",
            definitions(
                &[
                    "id",
                    "owner_id",
                    "owner_display",
                    "owner_kind",
                    "method_ids",
                    "origin",
                ],
                json!({
                    "owner_id": {"type": ["string", "null"]},
                    "owner_display": {"type": "string"},
                    "owner_kind": {"enum": ["local_adt", "external_or_non_nominal"]},
                    "method_ids": {"type": "array", "uniqueItems": true, "items": {"type": "string", "minLength": 1}}
                }),
            ),
        ),
        (
            "functions",
            definitions(
                &["id", "qualified_name", "origin"],
                json!({"qualified_name": {"type": "string", "minLength": 1}}),
            ),
        ),
    ] {
        properties[key] = value;
    }
    context["required"] = json!([
        "manifest_dir",
        "source_root",
        "target_triple",
        "test",
        "cfg",
        "compiler_version",
        "schema_version",
        "backend",
        "package_name",
        "crate_name",
        "types",
        "impls",
        "functions"
    ]);
    json!({"type": "array", "items": context})
}

fn definitions(required: &[&str], mut properties: Value) -> Value {
    properties["id"] = json!({"type": "string", "minLength": 1});
    properties["origin"] = origin_schema();
    json!({"type": "array", "items": {"type": "object", "additionalProperties": false,
        "required": required, "properties": properties}})
}
