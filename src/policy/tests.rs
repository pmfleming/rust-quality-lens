use super::*;
use crate::config::LensConfig;
use crate::measurement::MODEL_VERSION;

fn fixture() -> anyhow::Result<(tempfile::TempDir, LensConfig)> {
    let root = tempfile::tempdir()?;
    fs::create_dir(root.path().join("src"))?;
    fs::write(root.path().join("src/lib.rs"), "pub fn value() {}")?;
    let path = root.path().join("rqlens.toml");
    fs::write(&path, "source_roots = [\"src\"]\n")?;
    let config = LensConfig::load(Some(path))?;
    Ok((root, config))
}

fn document(config: &LensConfig, tool: &str, data: Value) -> Value {
    json!({
        "schema_version": 3, "risk_model_version": MODEL_VERSION, "tool": tool,
        "measurement_confidence": {"complete": true, "partial": false},
        "input_fingerprint": config.input_fingerprint(),
        "data": data,
    })
}

#[test]
fn enabled_policies_require_evidence_and_write_failure_reports() -> anyhow::Result<()> {
    let (_root, config) = fixture()?;
    for policy in [
        FailPolicy::TestFailure,
        FailPolicy::PracticeFailure,
        FailPolicy::ReliabilityFinding,
        FailPolicy::OperationalFailure,
        FailPolicy::Threshold,
        FailPolicy::Regression,
    ] {
        assert!(run_check(&config, None, std::slice::from_ref(&policy), 600.0, 0.0).is_err());
        let report: Value = serde_json::from_str(&fs::read_to_string(
            config.output_dir.join("policy_report.json"),
        )?)?;
        assert_eq!(report["passed"], false);
        assert!(
            report["failed_policies"]
                .as_array()
                .is_some_and(|failed| failed.contains(&json!(policy.as_str())))
        );
        assert!(
            report["input_failures"]
                .as_array()
                .is_some_and(|failures| !failures.is_empty())
        );
    }
    // Optional, unrelated inputs do not make an inspection-only check fail.
    assert!(run_check(&config, None, &[], 600.0, 0.0).is_ok());
    Ok(())
}

#[test]
fn malformed_partial_and_stale_test_evidence_cannot_pass() -> anyhow::Result<()> {
    let (_root, config) = fixture()?;
    let path = config.output_dir.join("correctness_review.json");
    let valid = document(
        &config,
        "correctness-run",
        json!({
            "summary": {"failed": 0, "run_failed": false, "compile_failed": false, "unknown": 0, "last_run": {"status": "passed"}}, "tests": []
        }),
    );
    write_json(&path, &valid)?;
    assert!(run_check(&config, None, &[FailPolicy::TestFailure], 600.0, 0.0).is_ok());
    for (pointer, replacement) in [
        ("/schema_version", Value::Null),
        ("/data/summary/last_run", Value::Null),
        ("/data/summary/unknown", json!(1)),
        ("/data/summary/failed", json!("0")),
        ("/data/summary/run_failed", Value::Null),
        ("/data/summary/compile_failed", json!(true)),
        (
            "/measurement_confidence",
            json!({"complete": false, "partial": true}),
        ),
        ("/input_fingerprint/digest", json!("old")),
    ] {
        let mut invalid = valid.clone();
        *invalid
            .pointer_mut(pointer)
            .ok_or_else(|| anyhow::anyhow!("missing test pointer"))? = replacement;
        write_json(&path, &invalid)?;
        assert!(
            run_check(&config, None, &[FailPolicy::TestFailure], 600.0, 0.0).is_err(),
            "{pointer}"
        );
    }
    fs::write(&path, "{broken")?;
    assert!(run_check(&config, None, &[FailPolicy::TestFailure], 600.0, 0.0).is_err());
    let report: Value = serde_json::from_str(&fs::read_to_string(
        config.output_dir.join("policy_report.json"),
    )?)?;
    assert_eq!(
        report["artifact_status"]["correctness_review.json"]["status"],
        "unreadable"
    );
    Ok(())
}

#[test]
fn operational_policy_rechecks_observation_age() -> anyhow::Result<()> {
    let (_root, config) = fixture()?;
    let mut evidence = document(
        &config,
        "operational-evidence",
        json!({"records": [{"status": "healthy"}]}),
    );
    evidence["schema_version"] = json!(1);
    evidence["telemetry_input"] = json!({
        "window": {"end": (chrono::Utc::now() - chrono::Duration::days(30)).to_rfc3339()},
        "max_age_hours": 24
    });
    write_json(
        &config.output_dir.join("operational_evidence.json"),
        &evidence,
    )?;
    assert!(run_check(&config, None, &[FailPolicy::OperationalFailure], 600.0, 0.0).is_err());
    let inputs = super::inputs::Inputs::load(&config);
    assert_eq!(
        inputs.status["operational_evidence.json"]["status"],
        "stale"
    );
    Ok(())
}

#[test]
fn auxiliary_baseline_errors_are_reported_instead_of_skipping_comparisons() -> anyhow::Result<()> {
    let (_root, config) = fixture()?;
    let baseline = config.project_root.join("baseline");
    fs::create_dir_all(&baseline)?;
    let map = document(
        &config,
        "map",
        json!({"graph": {"nodes": [{"data": {"id": "lib", "total_score": 10}}]}}),
    );
    write_json(&config.output_dir.join("map.json"), &map)?;
    write_json(&baseline.join("map.json"), &map)?;
    for (file, tool, data) in [
        (
            "coverage.json",
            "coverage",
            json!({"summary": {"lines": {"percent": 90.0}}}),
        ),
        (
            "performance.json",
            "performance",
            json!({"records": [{"id": "parse", "estimate_nanoseconds": 100.0}]}),
        ),
    ] {
        let mut current = document(&config, tool, data);
        if tool == "performance" {
            current["schema_version"] = json!(1);
        }
        write_json(&config.output_dir.join(file), &current)?;
        for invalid in [
            None,
            Some(json!({})),
            Some({
                let mut partial = current.clone();
                partial["measurement_confidence"] = json!({"complete": false, "partial": true});
                partial
            }),
        ] {
            if let Some(invalid) = invalid {
                write_json(&baseline.join(file), &invalid)?;
            }
            assert!(
                run_check(
                    &config,
                    Some(baseline.clone()),
                    &[FailPolicy::Regression],
                    600.0,
                    0.0
                )
                .is_err()
            );
            let report: Value = serde_json::from_str(&fs::read_to_string(
                config.output_dir.join("policy_report.json"),
            )?)?;
            assert_eq!(report["passed"], false);
            assert!(
                report["input_failures"]
                    .as_array()
                    .is_some_and(|rows| rows.iter().any(|row| row["artifact"] == "baseline"
                        && row["reason"]
                            .as_str()
                            .is_some_and(|reason| reason.contains(file))))
            );
        }
        write_json(&baseline.join(file), &current)?;
        assert!(
            run_check(
                &config,
                Some(baseline.clone()),
                &[FailPolicy::Regression],
                600.0,
                0.0
            )
            .is_ok()
        );
    }
    // A benchmark disappearing from either side cannot silently become an empty comparison.
    let performance = config.output_dir.join("performance.json");
    let mut current: Value = serde_json::from_str(&fs::read_to_string(&performance)?)?;
    current["data"]["records"][0]["id"] = json!("new-benchmark");
    write_json(&performance, &current)?;
    assert!(
        run_check(
            &config,
            Some(baseline),
            &[FailPolicy::Regression],
            600.0,
            0.0
        )
        .is_err()
    );
    let report: Value = serde_json::from_str(&fs::read_to_string(
        config.output_dir.join("policy_report.json"),
    )?)?;
    assert_eq!(
        report["evidence_deltas"].as_array().map(|rows| rows
            .iter()
            .filter(|row| row["status"] == "incomparable")
            .count()),
        Some(2)
    );
    Ok(())
}

#[test]
fn scoring_policies_reject_unknown_scores_and_missing_baselines() -> anyhow::Result<()> {
    let (_root, config) = fixture()?;
    let path = config.output_dir.join("map.json");
    let mut map = document(
        &config,
        "map",
        json!({"graph": {"nodes": [{"data": {"id": "lib", "total_score": null}}]}}),
    );
    write_json(&path, &map)?;
    assert!(run_check(&config, None, &[FailPolicy::Threshold], 600.0, 0.0).is_err());
    map["data"]["graph"]["nodes"][0]["data"]["total_score"] = json!(10.0);
    write_json(&path, &map)?;
    assert!(run_check(&config, None, &[FailPolicy::Threshold], 600.0, 0.0).is_ok());
    assert!(run_check(&config, None, &[FailPolicy::Threshold], 1.0, 0.0).is_err());
    assert!(run_check(&config, None, &[FailPolicy::Regression], 600.0, 0.0).is_err());
    assert!(run_check(&config, Some(path), &[FailPolicy::Regression], 600.0, 0.0).is_ok());
    Ok(())
}
