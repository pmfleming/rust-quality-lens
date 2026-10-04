//! Shared practice projection: exact observations, not inferred engine agreement.
use super::normalize_records;
use crate::{config::LensConfig, util::stable_hash};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(crate) struct PracticeProjection {
    pub(crate) findings: Vec<Value>,
    pub(crate) evidence: Value,
}

struct DiagnosticGroup {
    finding: Value,
    observations: Vec<Value>,
}

pub(crate) fn project(config: &LensConfig, document: Option<&Value>) -> PracticeProjection {
    let mut source = document.cloned().unwrap_or(Value::Null);
    let groups = collect_diagnostics(&mut source);
    let mut findings: Vec<Value> = source["data"]["checks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|check| {
            matches!(
                check["status"].as_str(),
                Some("failed" | "unavailable" | "timed-out")
            )
        })
        .cloned()
        .collect();
    normalize_records(config, &mut findings);
    for gate in &mut findings {
        gate["message"] = json!(format!(
            "{}: {}",
            gate["title"]
                .as_str()
                .unwrap_or_else(|| gate["rule_id"].as_str().unwrap_or_default()),
            gate["status"].as_str().unwrap_or_default()
        ));
    }
    findings.extend(groups.into_values().map(|mut group| {
        group.finding["observations"] = json!(group.observations);
        group.finding
    }));
    for finding in &mut findings {
        if let Some(rule) = finding["rule_id"]
            .as_str()
            .and_then(|id| config.policy.rules.get(id))
        {
            finding["policy_scope"] = json!({"included": rule.includes(finding["path"].as_str(), finding["package_name"].as_str()), "configuration": rule});
        }
    }
    PracticeProjection {
        findings,
        evidence: json!({"projection": "practice-v1", "scope_relation": "not_established", "artifact_with_diagnostic_refs": source}),
    }
}

fn collect_diagnostics(source: &mut Value) -> BTreeMap<String, DiagnosticGroup> {
    let mut groups = BTreeMap::<String, DiagnosticGroup>::new();
    for (check_index, check) in source
        .get_mut("data")
        .and_then(|data| data.get_mut("checks"))
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let diagnostics = check
            .as_object_mut()
            .and_then(|object| object.remove("diagnostics"));
        let mut references = Vec::new();
        for (diagnostic_index, diagnostic) in diagnostics
            .as_ref()
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            let mut finding = diagnostic.clone();
            if let Some(object) = finding.as_object_mut() {
                object.remove("id");
                object.remove("identity_version");
            }
            let key = finding.to_string();
            let group = groups.entry(key.clone()).or_insert_with(|| {
                finding["id"] = json!(format!("rqlens:diagnostic:{}", stable_hash(&key)));
                finding["identity_version"] = json!("exact-diagnostic-v1");
                DiagnosticGroup {
                    finding,
                    observations: Vec::new(),
                }
            });
            references.push(json!({"finding_id": group.finding["id"], "observation_index": group.observations.len()}));
            group.observations.push(json!({"check_index": check_index, "diagnostic_index": diagnostic_index, "diagnostic": diagnostic}));
        }
        if diagnostics.is_some() {
            check["diagnostics"] = json!(references);
        }
    }
    groups
}
