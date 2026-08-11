use anyhow::Result;
use serde_json::{Value, json};

use crate::config::LensConfig;
use crate::facts::RunContext;
use crate::measurement::{project_relative_path, source_confidence};
use crate::util::round2;

pub(super) fn produce(config: &LensConfig, context: &RunContext) -> Result<Value> {
    let confidence = source_confidence(&config.source_roots, &context.source_facts);
    let mut rows = context
        .source_facts
        .iter()
        .filter(|fact| fact.parse_status == "ok")
        .filter(|fact| fact.graph.public_api_count > 0 || matches!(fact.module_key.as_str(), "lib" | "main"))
        .map(|fact| {
            let total = fact.graph.public_api_count;
            let documented = fact.graph.documented_public_api_count.min(total);
            let missing = total.saturating_sub(documented);
            let percent = if total == 0 {
                100.0
            } else {
                documented as f64 / total as f64 * 100.0
            };
            let signals = [
                (missing > 0).then(|| {
                    format!("{missing} syntactically public items without documentation")
                }),
                (matches!(fact.module_key.as_str(), "lib" | "main") && !fact.graph.has_crate_docs)
                    .then(|| "crate-level documentation is missing".to_string()),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
            json!({
                "module_key": fact.module_key,
                "module_id": fact.module_id,
                "package_name": fact.package_name,
                "target_name": fact.target_name,
                "path": project_relative_path(&config.project_root, &fact.path),
                "public_item_count": total,
                "visibility_scope": "syntactic_pub",
                "documented_public_item_count": documented,
                "missing_documentation_count": missing,
                "documentation_percent": round2(percent),
                "crate_level_documentation": if matches!(fact.module_key.as_str(), "lib" | "main") { Value::Bool(fact.graph.has_crate_docs) } else { Value::Null },
                "signals": signals,
                "source": "https://rust-lang.github.io/api-guidelines/documentation.html",
                "measurement_confidence": confidence,
            })
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right["missing_documentation_count"]
            .as_u64()
            .cmp(&left["missing_documentation_count"].as_u64())
            .then_with(|| left["module_id"].as_str().cmp(&right["module_id"].as_str()))
    });
    Ok(Value::Array(rows))
}
