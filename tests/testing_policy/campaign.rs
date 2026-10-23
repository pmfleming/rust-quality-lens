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
    required_sources_allow_a_complete_gate(root)?;

    fs::write(
        root.join("testing.toml"),
        manifest().replace("name = \"journey\"", "name = \"missing\""),
    )?;
    run(root, &["measure", "test-quality"])?;
    let evaluation = evaluation(root)?;
    assert_eq!(evaluation["status"], "exceeded");
    assert_eq!(evaluation["observed"], 1);
    measure_policy_sources(root)?;
    let failed = invoke(root, &["check"])?;
    assert!(!failed.status.success());
    assert_reporter_agreement(root, "rust.testing.required-owner")?;
    reject_contradictory_owner_report(root)
}

fn reject_contradictory_owner_report(root: &Path) -> Result<()> {
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
    let _output = invoke(root, &["check"])?;
    let policy = document(root, "policy_report.json")?;
    Ok(policy["policy_rule_evaluations"][0].clone())
}

fn measure_policy_sources(root: &Path) -> Result<()> {
    for tool in ["practices", "reliability", "architecture-rules"] {
        run(root, &["measure", tool])?;
    }
    Ok(())
}

fn required_sources_allow_a_complete_gate(root: &Path) -> Result<()> {
    measure_policy_sources(root)?;
    run(root, &["check"])?;
    assert_eq!(document(root, "policy_report.json")?["passed"], true);
    Ok(())
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

fn capture_fault_evidence(root: &Path) -> Result<Value> {
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
    Ok(json!({"version": 1, "input_fingerprint": fingerprint,
        "producer": "rqlens-cli-fixture", "producer_version": "1",
        "records": [{"id": "wrong-answer", "behavior": "answer",
            "owner": {"package": "ownership_fixture", "target": "journey", "path": "tests/journey.rs", "name": "journey"},
            "fault": "return the wrong answer", "fault_path": "src/lib.rs", "expected_failure": "answer assertion fails",
            "baseline": baseline, "injected": injected, "restored": restored}]
    }))
}

pub(super) fn fault_sensitivity(root: &Path) -> Result<()> {
    let mut evidence = capture_fault_evidence(root)?;
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

    compile_failure_is_not_detection(root, &mut evidence, &detected)?;
    stale_fault_subject(root)
}

fn compile_failure_is_not_detection(
    root: &Path,
    evidence: &mut Value,
    detected: &Value,
) -> Result<()> {
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

    Ok(())
}

fn stale_fault_subject(root: &Path) -> Result<()> {
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
