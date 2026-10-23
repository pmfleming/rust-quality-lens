//! Additional phases of the same primary CLI journey, using disposable fixtures.
use super::*;
use serde_json::json;

pub(super) fn required_owner_policy(root: &Path) -> Result<()> {
    setup(root)?;
    fs::write(
        root.join("rqlens.toml"),
        "[rust]\nidentity_resolution = 'disabled'\n[testing_policy]\nmanifest = 'testing.toml'\n[policy.rules.\"rust.testing.required-owner\"]\nlevel = 'error'\nmax = 0\n",
    )?;
    run(root, &["measure", "test-quality"])?;
    assert_eq!(evaluation(root)?["status"], "incomplete");
    run(root, &["measure", "correctness-run"])?;
    run(root, &["measure", "test-quality"])?;
    assert_eq!(evaluation(root)?["status"], "passed");

    fs::write(
        root.join("testing.toml"),
        manifest().replace("name = \"journey\"", "name = \"missing\""),
    )?;
    run(root, &["measure", "test-quality"])?;
    let evaluation = evaluation(root)?;
    assert_eq!(evaluation["status"], "exceeded");
    assert_eq!(evaluation["observed"], 1);
    run(root, &["sarif"])?;
    let policy = document(root, "policy_report.json")?;
    let sarif: Value = serde_json::from_str(&fs::read_to_string(
        root.join("target/analysis/rqlens.sarif"),
    )?)?;
    let finding = policy["test_quality_warnings"]
        .as_array()
        .context("required-owner findings")?
        .iter()
        .find(|row| row["rule_id"] == "rust.testing.required-owner")
        .context("required owner")?;
    let exported = sarif["runs"][0]["results"]
        .as_array()
        .context("SARIF")?
        .iter()
        .find(|row| row["ruleId"] == "rust.testing.required-owner")
        .context("exported required owner")?;
    assert_eq!(exported["properties"]["rqlens_finding"], *finding);

    // A contradictory complete artifact must not manufacture verified zero.
    let mut forged = document(root, "test_quality.json")?;
    forged["data"]["findings"] = json!([]);
    fs::write(
        root.join("target/analysis/test_quality.json"),
        serde_json::to_vec(&forged)?,
    )?;
    assert_eq!(self::evaluation(root)?["status"], "incomplete");
    Ok(())
}

fn evaluation(root: &Path) -> Result<Value> {
    let output = invoke(root, &["check"])?;
    // Other configured-rule sources are intentionally absent in this bounded
    // workflow: assert this rule's evaluation, not a whole-project green gate.
    assert!(!output.status.success());
    let policy = document(root, "policy_report.json")?;
    Ok(policy["policy_rule_evaluations"][0].clone())
}

fn experiment_run(root: &Path, status: &str) -> Result<Value> {
    let args = ["test", "--test", "journey", "journey", "--", "--exact"];
    let output = Command::new("cargo")
        .args(args)
        .current_dir(root)
        .output()?;
    assert_eq!(output.status.success(), status == "passed");
    let log = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if status == "assertion_failed" {
        assert!(log.contains("assertion `left == right` failed"));
    }
    Ok(
        json!({"command": std::iter::once("cargo").chain(args).collect::<Vec<_>>(), "status": status, "output": log}),
    )
}

pub(super) fn fault_sensitivity(root: &Path) -> Result<()> {
    setup(root)?;
    fs::write(
        root.join("rqlens.toml"),
        "[rust]\nidentity_resolution = 'disabled'\n[testing_policy]\nmanifest = 'testing.toml'\nfault_evidence = 'faults.json'\n",
    )?;
    run(root, &["measure", "test-quality"])?;
    let initial = document(root, "test_quality.json")?;
    let fingerprint =
        initial["data"]["testing_policy"]["sensitivity"]["subject_fingerprint"].clone();
    assert_eq!(fingerprint["complete"], true);
    let baseline = experiment_run(root, "passed")?;
    fs::write(root.join("src/lib.rs"), "pub fn value() -> u8 { 0 }\n")?;
    let injected = experiment_run(root, "assertion_failed")?;
    fs::write(root.join("src/lib.rs"), "pub fn value() -> u8 { 42 }\n")?;
    let restored = experiment_run(root, "passed")?;
    let mut evidence = json!({"version": 1, "input_fingerprint": fingerprint,
        "producer": "rqlens-cli-fixture", "producer_version": "1",
        "records": [{"id": "wrong-answer", "behavior": "answer",
            "owner": {"package": "ownership_fixture", "target": "journey", "path": "tests/journey.rs", "name": "journey"},
            "fault": "return the wrong answer", "fault_path": "src/lib.rs", "expected_failure": "answer assertion fails",
            "baseline": baseline, "injected": injected, "restored": restored}]
    });
    fs::write(root.join("faults.json"), serde_json::to_vec(&evidence)?)?;
    run(root, &["measure", "correctness-run"])?;
    run(root, &["measure", "test-quality"])?;
    let detected = document(root, "test_quality.json")?;
    assert_eq!(
        detected["data"]["testing_policy"]["sensitivity"]["records"][0]["status"],
        "reported_detected"
    );
    assert_eq!(
        detected["data"]["testing_policy"]["behaviors"][0]["status"],
        "observed_pass"
    );

    fs::write(root.join("src/lib.rs"), "this does not compile\n")?;
    evidence["records"][0]["injected"] = experiment_run(root, "compile_failed")?;
    fs::write(root.join("src/lib.rs"), "pub fn value() -> u8 { 42 }\n")?;
    evidence["records"][0]["restored"] = experiment_run(root, "passed")?;
    fs::write(root.join("faults.json"), serde_json::to_vec(&evidence)?)?;
    run(root, &["measure", "test-quality"])?;
    let inconclusive = document(root, "test_quality.json")?;
    assert_eq!(
        inconclusive["data"]["testing_policy"]["sensitivity"]["records"][0]["status"],
        "inconclusive"
    );
    assert_eq!(
        inconclusive["data"]["testing_policy"]["execution_evidence"],
        "stale"
    );
    assert_ne!(
        detected["input_fingerprint"]["digest"],
        inconclusive["input_fingerprint"]["digest"]
    );

    fs::write(root.join("src/lib.rs"), "pub fn value() -> u8 { 41 }\n")?;
    run(root, &["measure", "test-quality"])?;
    let stale = document(root, "test_quality.json")?;
    assert_eq!(
        stale["data"]["testing_policy"]["sensitivity"]["status"],
        "unavailable"
    );
    assert_eq!(stale["measurement_confidence"]["complete"], false);
    Ok(())
}
