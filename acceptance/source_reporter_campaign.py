#!/usr/bin/env python3
"""Source/imported reporter parity fixtures; not detector or complete-scope certification."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import subprocess

from evidence_contract_campaign import FILES

RULE = "rust.reliability.unwrap"


def execute(binary, root, *arguments):
    return subprocess.run([str(binary), *arguments], cwd=root, capture_output=True, text=True, timeout=120)


def fixture(binary, root, mode):
    (root / "src").mkdir(parents=True)
    (root / "src/lib.rs").write_text("pub fn example() {}\n")
    (root / "Cargo.toml").write_text('[package]\nname="source-fixture"\nversion="0.1.0"\nedition="2024"\n')
    config = 'source_roots=["src"]\noutput_dir="analysis"\n[rust]\nidentity_resolution="disabled"\n'
    for rule in [RULE, "boundary", "rust.test.ignored"]:
        config += f'[policy.rules."{rule}"]\nlevel="error"\nmax=100\n'
        if mode == "excluded":
            config += 'exclude_paths=["src/**"]\n'
    config += '[[architecture.rules]]\nid="boundary"\nfrom=["*"]\ndeny=["*"]\nlevel="warning"\n'
    if mode in ["waived", "expired"]:
        expiry = "2099-01-01" if mode == "waived" else "2000-01-01"
        config += f'[[policy.waivers]]\nrule_id="{RULE}"\nreason="fixture"\nowner="test"\nexpires="{expiry}"\n'
    if mode == "external":
        for adapter in ["a", "b", "a-b"]:
            config += f'[policy.rules."external.{adapter}.rule"]\nlevel="error"\nmax=100\n'
    (root / "rqlens.toml").write_text(config)
    measured = execute(binary, root, "measure", "reliability")
    (root / "measure.log").write_text(measured.stdout + measured.stderr)
    assert measured.returncode == 0
    seed = json.loads((root / "analysis/reliability_findings.json").read_text())
    row = {"rule_id": RULE, "severity": "warning", "scope": "production", "path": "src/lib.rs",
           "line": 1, "message": "fixture", "policy_selected": False, "policy_scope": {"included": False}}
    for file, (tool, key, data) in FILES.items():
        payload = copy.deepcopy(data)
        if file == "reliability_findings.json":
            payload = [dict(row, severity="advisory", scope="test", id="original-test"),
                       dict(row, id="original-production"), dict(row, severity="error", disposition="info", id="original-info")]
        elif file == "architecture_rules.json":
            payload["violations"] = [dict(row, rule_id="boundary", severity="error", disposition="warn")]
        elif file == "test_quality.json":
            payload["findings"] = [dict(row, rule_id="rust.test.ignored", disposition="block")]
        document = {"schema_version": seed["schema_version"], "risk_model_version": seed["risk_model_version"],
                    "tool": tool, "measurement_confidence": {"complete": mode != "partial", "partial": mode == "partial"},
                    "input_fingerprint": seed["input_fingerprint"], key: payload}
        (root / "analysis" / file).write_text(json.dumps(document))
    return seed


def capture(binary, root):
    policy_result = execute(binary, root, "check")
    sarif_result = execute(binary, root, "sarif")
    (root / "policy.stdout.json").write_text(policy_result.stdout)
    (root / "policy.stderr.log").write_text(policy_result.stderr)
    (root / "sarif.log").write_text(sarif_result.stdout + sarif_result.stderr)
    assert sarif_result.returncode == 0
    policy = json.loads((root / "analysis/policy_report.json").read_text())
    sarif = json.loads((root / "analysis/rqlens.sarif").read_text())["runs"][0]
    assert policy == json.loads(policy_result.stdout)
    assert (policy_result.returncode == 0) == policy["passed"]
    return policy, sarif


def source_rows(policy):
    fields = ["reliability_findings", "reliability_warnings", "reliability_out_of_scope",
              "architecture_violations", "architecture_warnings", "test_quality_findings", "test_quality_warnings"]
    return [row for field in fields for row in policy[field]] + [entry["finding"] for entry in policy["waived_findings"]]


def verify_sources(root, policy, sarif, mode):
    expected = {row["id"]: row for row in source_rows(policy)}
    actual = {row["properties"]["rqlens_finding"]["id"]: row["properties"]["rqlens_finding"] for row in sarif["results"]}
    assert actual == expected and len(actual) == 5
    assert policy["source_evidence"] == sarif["properties"]["source_evidence"]
    for file in ["reliability_findings.json", "architecture_rules.json", "test_quality.json"]:
        assert policy["source_evidence"][file] == json.loads((root / "analysis" / file).read_text())
    for result in sarif["results"]:
        row = result["properties"]["rqlens_finding"]
        origin = row["origin"]
        raw = policy["source_evidence"][origin["artifact"]]
        raw_rows = raw["records"] if "records" in raw else raw["data"]["violations" if origin["artifact"] == "architecture_rules.json" else "findings"]
        original = raw_rows[origin["record_index"]]
        for key, value in original.items():
            if key not in ["id", "identity_version", "policy_scope", "policy_selected"]:
                assert row[key] == value
        assert result["partialFingerprints"]["rqlens/source-anchor-v1"] == row["id"]
        selected = not (origin["artifact"] == "reliability_findings.json" and original["scope"] == "test")
        assert row["policy_selected"] == selected
        assert row["policy_scope"]["included"] == (mode != "excluded")
        assert bool(result.get("suppressions")) == (mode == "waived" and row["rule_id"] == RULE)
        assert result["level"] == {"block": "error", "warn": "warning", "info": "note"}.get(row.get("disposition"), "note" if row["severity"] == "advisory" else row["severity"])
    evaluation, = [row for row in policy["policy_rule_evaluations"] if row["rule_id"] == RULE]
    assert evaluation["observed_findings"] == (0 if mode in ["excluded", "waived"] else 2)
    assert evaluation["status"] == ("incomplete" if mode == "partial" else "passed")
    assert policy["passed"] == (mode != "partial")


def imported(seed):
    return {"tool": {"name": "fixture-engine", "version": "1"}, "status": "completed",
            "input_fingerprint": seed["input_fingerprint"], "confidence": {"complete": True},
            "scope": {"included": ["src"], "excluded": ["tests"], "rules": ["rule"]}, "metrics": [{"name": "fixture", "value": 1}],
            "findings": [{"rule_id": "rule", "id": "original", "severity": "error", "disposition": "warn",
                          "message": "imported", "path": "src/lib.rs", "line": 1, "policy_selected": False, "policy_scope": {"included": False}}]}


def external_cases(binary, root, seed):
    report = root / "import.json"
    report.write_text(json.dumps(imported(seed)))
    for adapter in ["a", "b", "a-b"]:
        result = execute(binary, root, "ingest", "--adapter", adapter, "--input", str(report))
        assert result.returncode == 0, result.stderr
    policy, sarif = capture(binary, root)
    expected = {row["id"]: row for row in policy["external_findings"]}
    actual = {row["properties"]["rqlens_finding"]["id"]: row["properties"]["rqlens_finding"] for row in sarif["results"] if row["ruleId"].startswith("external.")}
    assert expected == actual and len(expected) == 3
    assert policy["external_evidence"] == sarif["properties"]["external_evidence"]
    for row in expected.values():
        assert row["policy_selected"] is True and row["policy_scope"]["included"] is True
        origin = row["origin"]
        envelope = policy["external_evidence"][origin["adapter"]]
        assert envelope == json.loads((root / "analysis" / f'external_{origin["adapter"]}.json').read_text())
        assert envelope["data"]["findings"][origin["record_index"]]["id"] == "original"
    original = {path: path.read_bytes() for path in (root / "analysis").glob("external_*.json")}
    rejected = []
    for adapter in ["a/b", "a.b", "a b", "../a", ""]:
        assert execute(binary, root, "ingest", "--adapter", adapter, "--input", str(report)).returncode != 0
        assert {path: path.read_bytes() for path in (root / "analysis").glob("external_*.json")} == original
        rejected.append(f"adapter:{adapter}")
    for field, value in [("message", None), ("rule_id", ""), ("severity", "invalid"), ("disposition", "invalid"), ("path", {}), ("line", 0)]:
        bad = imported(seed)
        bad["findings"][0][field] = value
        report.write_text(json.dumps(bad))
        assert execute(binary, root, "ingest", "--adapter", "a", "--input", str(report)).returncode != 0
        assert (root / "analysis/external_a.json").read_bytes() == original[root / "analysis/external_a.json"]
        rejected.append(field)
    for field in ["name", "version"]:
        bad = imported(seed)
        bad["tool"][field] = " "
        report.write_text(json.dumps(bad))
        assert execute(binary, root, "ingest", "--adapter", "a", "--input", str(report)).returncode != 0
        assert (root / "analysis/external_a.json").read_bytes() == original[root / "analysis/external_a.json"]
        rejected.append(f"tool:{field}")
    path = root / "analysis/external_a.json"
    envelope = json.loads(original[path])
    for name in ["version", "adapter", "partial", "stale", "corrupt"]:
        bad = copy.deepcopy(envelope)
        if name == "version": bad["schema_version"] = 999
        if name == "adapter": bad["adapter"] = "b"
        if name == "partial": bad["data"]["status"] = "partial"
        if name == "stale": bad["data"]["input_fingerprint"]["digest"] = "stale"
        path.write_text("{broken" if name == "corrupt" else json.dumps(bad))
        policy, sarif = capture(binary, root)
        assert policy["passed"] is False
        assert policy["external_evidence_reasons"] == sarif["properties"]["external_evidence_reasons"]
        assert policy["external_evidence_reasons"]
        assert set(policy["external_evidence"]) == {"b", "a-b"}
        assert len(policy["external_findings"]) == 2
        rejected.append(f"reader:{name}")
    path.write_bytes(original[path])
    capture(binary, root)
    return rejected


def source_rejections(binary, root):
    results = []
    files = ["reliability_findings.json", "architecture_rules.json", "test_quality.json"]
    for file in files:
        path = root / "analysis" / file
        original = path.read_bytes()
        prior = (root / "analysis/rqlens.sarif").read_bytes()
        for mutation, status in [("stale", "stale"), ("corrupt", "unreadable"), ("version", "incompatible")]:
            invalid = json.loads(original)
            if mutation == "stale": invalid["input_fingerprint"]["digest"] = "stale"
            if mutation == "version": invalid["schema_version"] = 999
            path.write_text("{broken" if mutation == "corrupt" else json.dumps(invalid))
            checked = execute(binary, root, "check")
            exported = execute(binary, root, "sarif")
            (root / f"{file}-{mutation}.log").write_text(checked.stdout + checked.stderr + exported.stderr)
            assert checked.returncode != 0 and exported.returncode != 0
            policy = json.loads((root / "analysis/policy_report.json").read_text())
            assert policy["artifact_status"][file]["status"] == status
            assert (root / "analysis/rqlens.sarif").read_bytes() == prior
            results.append(f"{file}:{mutation}")
        path.unlink()
        policy, sarif = capture(binary, root)
        assert policy["passed"] is False
        assert policy["source_evidence"][file] is None and sarif["properties"]["source_evidence"][file] is None
        results.append(f"{file}:missing")
        path.write_bytes(original)
        capture(binary, root)
    return results


def run(binary, output):
    binary = binary.resolve(strict=True)
    output.mkdir(parents=True, exist_ok=False)
    cases = []
    for mode in ["plain", "excluded", "waived", "expired", "partial", "external"]:
        root = output / mode
        seed = fixture(binary, root, mode)
        if mode == "external":
            rejection_cases = external_cases(binary, root, seed)
        else:
            policy, sarif = capture(binary, root)
            verify_sources(root, policy, sarif, mode)
        for command, file in [("check", "policy_report.json"), ("sarif", "rqlens.sarif")]:
            previous = (root / "analysis" / file).read_bytes()
            execute(binary, root, command)
            assert (root / "analysis" / file).read_bytes() == previous
        cases.append(mode)
    rejection_cases.extend(source_rejections(binary, output / "plain"))
    result = {"binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(), "source_and_external_cases": cases,
              "rejection_cases": rejection_cases, "exact_reporter_replays": 12, "passed": True,
              "scope": "Synthetic native-envelope and import-boundary fixtures; not detector or engine-scope certification"}
    (output / "results.json").write_text(json.dumps(result, indent=2) + "\n")
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/debug/rqlens"))
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = run(args.binary, args.output.resolve())
    print(f'{len(result["source_and_external_cases"])} agreement cases, {len(result["rejection_cases"])} rejection cases passed')
