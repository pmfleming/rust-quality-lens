use super::{Review, model::Manifest, ownership::owner_status, requirements};
use anyhow::{Result, ensure};
use serde_json::Value;

pub(crate) fn validate_report(payload: &Value) -> Result<()> {
    let required_findings = payload["findings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| row["rule_id"] == requirements::RULE)
        .collect::<Vec<_>>();
    let Some(value) = payload.get("testing_policy") else {
        ensure!(
            required_findings.is_empty(),
            "ownership findings require a policy report"
        );
        return Ok(());
    };
    let report: Review = serde_json::from_value(value.clone())?;
    validate_structure(&report)?;
    let expected = requirements::findings(
        report.manifest.as_deref().unwrap_or_default(),
        &report.behaviors,
    );
    ensure!(
        expected.len() == required_findings.len(),
        "required-owner findings disagree with observations"
    );
    for finding in expected {
        let matching = required_findings
            .iter()
            .filter(|row| row["behavior_id"] == finding["behavior_id"])
            .collect::<Vec<_>>();
        ensure!(
            matching.len() == 1
                && matching[0]["owner_status"] == finding["owner_status"]
                && matching[0]["status"] == "failed",
            "required-owner finding is missing, duplicated or inconsistent"
        );
    }
    Ok(())
}

fn validate_structure(report: &Review) -> Result<()> {
    ensure!(report.version == 1, "unsupported testing policy report");
    ensure!(
        ["not_configured", "unavailable", "reviewed"].contains(&report.status.as_str()),
        "invalid testing policy status"
    );
    ensure!(
        report.status == "reviewed"
            || (report.behaviors.is_empty() && report.candidates.is_empty()),
        "unevaluated testing policy contains claims"
    );
    ensure!(
        report.status == "not_configured"
            || report
                .manifest
                .as_ref()
                .is_some_and(|path| !path.trim().is_empty()),
        "configured policy has no manifest"
    );
    let manifest = Manifest {
        version: 1,
        behaviors: report
            .behaviors
            .iter()
            .map(|row| row.declared.clone())
            .collect(),
        dispositions: report
            .candidates
            .iter()
            .map(|row| serde_json::from_value(row["declared"].clone()))
            .collect::<std::result::Result<_, _>>()?,
    };
    manifest.validate()?;
    super::sensitivity::validate_output(&report.sensitivity, &report.behaviors)?;
    for row in &report.behaviors {
        ensure!(
            row.observed.len() == row.declared.owner.tests.len(),
            "incomplete owner observations"
        );
        for (observed, declared) in row.observed.iter().zip(&row.declared.owner.tests) {
            ensure!(
                observed.test == *declared,
                "owner observation selector mismatch"
            );
            ensure!(
                ["resolved", "unresolved", "ambiguous"].contains(&observed.resolution.as_str()),
                "invalid owner resolution"
            );
            ensure!(
                ["passed", "failed", "skipped", "unknown"].contains(&observed.execution.as_str()),
                "invalid owner execution"
            );
            ensure!(
                observed.resolution == "resolved" || observed.execution == "unknown",
                "unresolved owner claims execution"
            );
        }
        ensure!(
            row.status == owner_status(&row.observed, &report.execution_evidence),
            "owner status disagrees with observations"
        );
    }
    Ok(())
}
