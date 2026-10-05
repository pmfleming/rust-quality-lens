//! Shared JSON Schema property constraints, independent of artifact schemas.
use serde_json::Value;

/// Apply one schema to groups of whitespace-separated property names.
/// Grouping identical constraints keeps artifact schemas on the same rules.
pub(crate) fn property_groups(mut properties: Value, groups: &[(&str, Value)]) -> Value {
    for (names, schema) in groups {
        for name in names.split_whitespace() {
            properties[name] = schema.clone();
        }
    }
    properties
}
