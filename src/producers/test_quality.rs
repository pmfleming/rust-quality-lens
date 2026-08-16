use anyhow::Result;
use serde_json::{Value, json};
use std::collections::BTreeSet;

use crate::config::LensConfig;
use crate::facts::{FileFacts, RunContext, TestFact};
use crate::measurement::source_confidence;

pub(super) fn produce(config: &LensConfig, context: &RunContext) -> Result<Value> {
    let facts = merged_facts(context);
    let paths = if context.correctness_paths.is_empty() {
        &config.source_roots
    } else {
        &context.correctness_paths
    };
    let confidence = source_confidence(paths, &facts);
    let mut seen = BTreeSet::new();
    let tests = facts
        .iter()
        .flat_map(|fact| fact.items.tests.iter().map(move |test| (fact, test)))
        .filter(|(_, test)| seen.insert((test.path.clone(), test.qualified_name.clone())))
        .map(|(fact, test)| test_row(fact, test))
        .collect::<Vec<_>>();
    let findings = tests.iter().flat_map(test_findings).collect::<Vec<_>>();
    Ok(json!({
        "summary": {
            "test_count": tests.len(),
            "finding_count": findings.len(),
            "assertionless_test_count": tests.iter().filter(|test| test["assertion_count"] == 0).count(),
            "ignored_test_count": tests.iter().filter(|test| test["ignored"] == true).count(),
        },
        "tests": tests,
        "findings": findings,
        "measurement_confidence": confidence,
    }))
}

fn merged_facts(context: &RunContext) -> Vec<FileFacts> {
    if context.correctness_facts.is_empty() {
        context.source_facts.clone()
    } else {
        context.correctness_facts.clone()
    }
}

fn test_row(fact: &FileFacts, test: &TestFact) -> Value {
    json!({
        "name": test.name,
        "qualified_name": test.qualified_name,
        "path": test.path,
        "line": test.line,
        "module_key": test.module_key,
        "module_id": fact.module_id,
        "package_name": fact.package_name,
        "assertion_count": test.assertion_count,
        "sut_call_count": test.sut_call_count,
        "ignored": test.ignored,
    })
}

fn test_findings(test: &Value) -> Vec<Value> {
    let mut findings = Vec::new();
    if test["assertion_count"] == 0 {
        findings.push(finding(
            test,
            "rust.test.no-assertion",
            "test body has no recognized assertion or snapshot macro",
        ));
    }
    if test["sut_call_count"] == 0 {
        findings.push(finding(
            test,
            "rust.test.no-sut-call",
            "test body has no recognized function, method, or non-assertion macro call",
        ));
    }
    if test["ignored"] == true {
        findings.push(finding(test, "rust.test.ignored", "test is ignored"));
    }
    findings
}

fn finding(test: &Value, rule_id: &str, message: &str) -> Value {
    json!({
        "rule_id": rule_id,
        "category": "static-finding",
        "kind": "test-quality",
        "severity": "advisory",
        "path": test["path"],
        "line": test["line"],
        "module_key": test["module_key"],
        "module_id": test["module_id"],
        "package_name": test["package_name"],
        "message": message,
        "source": "rqlens_test_quality",
    })
}
