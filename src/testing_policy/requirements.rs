//! Only explicit ownership obligations are enforceable, never test-value advice.
use super::{BehaviorReview, Review, finding};
use serde_json::{Value, json};

pub(crate) const RULE: &str = "rust.testing.required-owner";

pub(super) fn findings(path: &str, behaviors: &[BehaviorReview]) -> Vec<Value> {
    behaviors.iter().filter(|row| row.declared.required)
        .filter(|row| matches!(row.status.as_str(), "unresolved" | "observed_failure"))
        .map(|row| {
            let mut finding = finding(path, &row.declared, RULE,
                "A declared required primary owner is unresolved or has an observed failure; restore this explicit protection.");
            finding["category"] = json!("static-finding");
            finding["status"] = json!("failed");
            finding["owner_status"] = json!(row.status);
            finding
        }).collect()
}

pub(crate) fn unavailability(payload: &Value) -> Option<String> {
    let report: Review = match serde_json::from_value(payload["testing_policy"].clone()) {
        Ok(report) => report,
        Err(_) => return Some("No valid testing-policy ownership evaluation".into()),
    };
    if report.status != "reviewed" {
        return Some("Testing policy is not configured or its manifest is unavailable".into());
    }
    let required = report
        .behaviors
        .iter()
        .filter(|row| row.declared.required)
        .collect::<Vec<_>>();
    if required.is_empty() {
        return Some("No explicitly required behavior owners are declared".into());
    }
    if required.iter().any(|row| row.status == "unknown") {
        return Some(
            "Required owner execution is unknown, skipped, stale, incomplete or not attributable"
                .into(),
        );
    }
    None
}
