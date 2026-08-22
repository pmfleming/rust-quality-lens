use anyhow::Result;
use serde_json::{Value, json};

use crate::config::LensConfig;
use crate::facts::RunContext;
use crate::measurement::{project_relative_path, source_confidence};

pub(super) fn produce(config: &LensConfig, context: &RunContext) -> Result<Value> {
    let confidence = source_confidence(&config.source_roots, &context.source_facts);
    let mut findings = context
        .source_facts
        .iter()
        .filter(|fact| fact.parse_status == "ok")
        .flat_map(|fact| {
            let path = project_relative_path(&config.project_root, &fact.path);
            let path_is_test = is_test_path(&path) || is_test_module(&fact.module_key);
            let confidence = &confidence;
            fact.items.quality_findings.iter().map(move |finding| {
                let test_code = path_is_test || finding.test_code;
                let safety = finding.rule_id.starts_with("rust.safety.");
                super::with_fact_identity(fact, json!({
                    "rule_id": finding.rule_id,
                    "category": "static-finding",
                    "kind": finding.kind,
                    "severity": if test_code { "advisory" } else if safety { "error" } else { "warning" },
                    "path": path,
                    "line": finding.line,
                    "module_key": fact.module_key,
                    "scope": if test_code { "test" } else { "production" },
                    "message": finding.message,
                    "source": rule_source(&finding.rule_id),
                    "measurement_confidence": confidence,
                }))
            })
        })
        .collect::<Vec<_>>();
    findings.sort_by(|left, right| {
        left["path"]
            .as_str()
            .cmp(&right["path"].as_str())
            .then_with(|| left["line"].as_u64().cmp(&right["line"].as_u64()))
            .then_with(|| left["rule_id"].as_str().cmp(&right["rule_id"].as_str()))
    });
    Ok(Value::Array(findings))
}

fn is_test_path(path: &str) -> bool {
    let file = path.rsplit('/').next().unwrap_or(path);
    path.starts_with("tests/")
        || path.contains("/tests/")
        || path.starts_with("benches/")
        || path.starts_with("examples/")
        || file == "tests.rs"
        || file == "test.rs"
        || file.ends_with("_tests.rs")
}

fn is_test_module(module: &str) -> bool {
    module
        .split("::")
        .any(|component| matches!(component, "test" | "tests") || component.ends_with("_tests"))
}

fn rule_source(rule_id: &str) -> &'static str {
    if rule_id.starts_with("rust.safety.") {
        "https://doc.rust-lang.org/nomicon/"
    } else {
        "https://rust-lang.github.io/rust-clippy/master/index.html"
    }
}
