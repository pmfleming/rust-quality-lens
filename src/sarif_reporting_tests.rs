use super::{add_source_findings, annotate_result};
use crate::config::PolicyWaiver;
use crate::test_support::source_project;
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[test]
fn common_report_cases_preserve_disposition_and_waivers() -> Result<()> {
    let (_root, mut config) = source_project(None)?;
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("../contracts/reporting-cases.json"))?;
    for case in cases {
        let mut finding = case.clone();
        finding["rule_id"] = case["id"].clone();
        finding["message"] = json!("fixture");
        if case["suppressed"] == true {
            config.policy.waivers.push(PolicyWaiver {
                rule_id: case["id"].as_str().context("case ID")?.into(),
                path: None,
                reason: "fixture waiver".into(),
                owner: "test".into(),
                expires: "2099-01-01".into(),
            });
        }
        let mut results = Vec::new();
        add_source_findings(
            &config,
            std::iter::once(&finding),
            &mut BTreeMap::new(),
            &mut results,
        );
        assert_eq!(results[0]["level"], case["expected_level"]);
        assert_eq!(
            crate::findings::finding_level(&finding),
            case["expected_level"].as_str().context("expected level")?
        );
        assert_eq!(
            results[0].get("suppressions").is_some(),
            case["suppressed"] == true
        );
        for status in ["unavailable", "timed-out", "incomplete", "failed_execution"] {
            finding["status"] = json!(status);
            let mut result = json!({});
            annotate_result(&config, &finding, &mut result);
            assert!(result.get("suppressions").is_none());
            assert!(!crate::findings::is_waivable(&finding));
        }
    }
    Ok(())
}
