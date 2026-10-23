use anyhow::Result;
use serde_json::{Value, json};
use std::collections::BTreeSet;

use crate::config::LensConfig;
use crate::facts::{FileFacts, RunContext, TestFact};

pub(super) fn produce(config: &LensConfig, context: &RunContext) -> Result<Value> {
    let facts = selected_facts(context);
    let confidence = if context.correctness_paths.is_empty() {
        context.source_confidence()
    } else {
        context.correctness_confidence()
    };
    let mut seen = BTreeSet::new();
    let tests = facts
        .iter()
        .flat_map(|fact| fact.items.tests.iter().map(move |test| (fact, test)))
        .filter(|(fact, test)| {
            seen.insert((
                &fact.package_name,
                &fact.target_name,
                &test.path,
                &test.qualified_name,
            ))
        })
        .map(|(fact, test)| test_row(config, fact, test))
        .collect::<Vec<_>>();
    let mut findings = tests.iter().flat_map(test_findings).collect::<Vec<_>>();
    let mut confidence = serde_json::from_value(confidence)?;
    let testing_policy = crate::testing_policy::review(
        config,
        &context.snapshot.fingerprint,
        &tests,
        &mut confidence,
        &mut findings,
    )?;
    Ok(json!({
        "testing_policy": testing_policy,
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

fn selected_facts(context: &RunContext) -> &[FileFacts] {
    if context.correctness_facts.is_empty() {
        &context.source_facts
    } else {
        &context.correctness_facts
    }
}

fn test_row(config: &LensConfig, fact: &FileFacts, test: &TestFact) -> Value {
    super::with_fact_identity(
        fact,
        json!({
            "name": test.name,
            "qualified_name": test.qualified_name,
            "path": test.path,
            "relative_path": crate::util::project_relative_path(&config.project_root, &test.path),
            "line": test.line,
            "module_key": test.module_key,
            "assertion_count": test.assertion_count,
            "sut_call_count": test.sut_call_count,
            "ignored": test.ignored,
        }),
    )
}

fn test_findings(test: &Value) -> Vec<Value> {
    let mut findings = Vec::new();
    if test["assertion_count"] == 0 {
        findings.push(finding(
            test,
            "rust.test.no-assertion",
            "no assertion recognized directly in this body; inspect helper, Result and compile-time checks before judging protection, rather than adding assertions to satisfy a count",
        ));
    }
    if test["sut_call_count"] == 0 {
        findings.push(finding(
            test,
            "rust.test.no-sut-call",
            "no subject call recognized directly in this body; inspect indirect execution and the owned behavior before judging test value",
        ));
    }
    if test["ignored"] == true {
        findings.push(finding(test, "rust.test.ignored", "test is ignored; review its required environment and important risk, not test-count targets"));
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
