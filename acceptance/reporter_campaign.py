#!/usr/bin/env python3
"""Native E4/E6 practice projection cases; no whole-reporter certification."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import subprocess

from probe_reporter_gaps import probe

RULE = "rust.compiler.diagnostic"


def checks():
    diagnostic = {"rule_id": RULE, "severity": "error", "path": "src/lib.rs", "line": 1,
                  "column": 1, "message": "shared observation", "package_name": "fixture"}
    return [{"rule_id": f"rust.official.{name}", "title": name, "status": "failed", "severity": "error",
             "tool": "cargo", "tool_version": "fixture-1", "diagnostics": [dict(diagnostic, id=f"origin-{index}")],
             "evidence": {"command": ["cargo", command], "scope": {"targets": ["lib"], "exclusions": ["tests"]}}}
            for index, (name, command) in enumerate([("cargo-check", "check"), ("clippy", "clippy")])]


def scenarios():
    rows = [("corroborating-observations", checks(), "", 1, 2, False)]
    for field, value in [("severity", "warning"), ("disposition", "warn"), ("message", "different observation"),
                         ("package_name", "other"), ("scope", {"excluded": ["src/generated"]})]:
        data = checks()
        data[1]["diagnostics"][0][field] = value
        rows.append((f"different-{field}", data, "", 2, 2, False))
    data = checks()
    for check in data:
        check["status"] = "passed"
        check["diagnostics"][0]["severity"] = "warning"
    rows.append(("passed-with-warning", data, "", 1, 2, False))
    data = checks()
    data[1]["diagnostics"][0].update(severity="warning", disposition="block")
    rows.append(("blocking-disposition", data, "", 2, 2, False))
    for kind in ["no-diagnostic", "no-inventory"]:
        data = checks()
        data[1]["status"] = "passed"
        if kind == "no-diagnostic":
            data[1]["diagnostics"] = []
        else:
            del data[1]["diagnostics"]
        rows.append((kind, data, "", 1, 1, False))
    data = checks()
    for check in data:
        for field in ["path", "line", "column"]:
            del check["diagnostics"][0][field]
    rows.append(("unlocated", data, "", 1, 2, False))
    data = checks()
    for check in data:
        check["diagnostics"][0].update(line=None, column=None)
    rows.append(("unknown-region", data, "", 1, 2, False))
    for label, configuration, waived in [
        ("waived", f'[[policy.waivers]]\nrule_id="{RULE}"\nreason="fixture"\nowner="test"\nexpires="2099-01-01"\n', True),
        ("expired", f'[[policy.waivers]]\nrule_id="{RULE}"\nreason="fixture"\nowner="test"\nexpires="2000-01-01"\n', False),
        ("path-exclusion", 'exclude_paths=["src/**"]\n', False),
        ("package-exclusion", 'exclude_packages=["fixture"]\n', False),
    ]:
        rows.append((label, checks(), configuration, 1, 2, waived))
    data = checks()
    for check in data:
        check["status"] = "passed"
        check["diagnostics"] = []
    rows.append(("empty-observations", data, "", 0, 0, False))
    data = checks()
    data[1]["status"] = "timed-out"
    rows.append(("incomplete-invocation", data, "", 1, 2, False))
    rows.append(("reversed-invocations", list(reversed(checks())), "", 1, 2, False))
    return rows


def policy_rows(policy):
    return policy["practice_failures"] + policy["practice_warnings"] + [row["finding"] for row in policy["waived_findings"]]


def validate(root, group_count, observation_count, waived):
    policy = json.loads((root / "analysis/policy_report.json").read_text())
    sarif = json.loads((root / "analysis/rqlens.sarif").read_text())
    assert json.loads((root / "policy.stdout.json").read_text()) == policy
    rows = [row for row in policy_rows(policy) if row.get("identity_version") == "exact-diagnostic-v1"]
    exported = [row for row in sarif["runs"][0]["results"] if row["ruleId"] == RULE]
    assert len(rows) == len(exported) == group_count
    expected = {row["id"]: row for row in rows}
    actual = {row["properties"]["rqlens_finding"]["id"]: row["properties"]["rqlens_finding"] for row in exported}
    assert actual == expected
    assert sum(len(row["observations"]) for row in rows) == observation_count
    assert policy["practice_evidence"] == sarif["runs"][0]["properties"]["practice_evidence"]
    assert policy["practice_evidence"]["scope_relation"] == "not_established"
    assert reconstruct(policy["practice_evidence"], expected) == json.loads((root / "analysis/rust_practices.json").read_text())
    for result in exported:
        row = result["properties"]["rqlens_finding"]
        assert result["partialFingerprints"]["rqlens/exact-diagnostic-v1"] == row["id"]
        level = {"block": "error", "warn": "warning", "review": "note", "info": "note"}.get(row.get("disposition"), row["severity"])
        assert result["level"] == level
        assert bool(result.get("suppressions")) == waived
        if "path" not in row:
            assert "locations" not in result
        else:
            location = result["locations"][0]["physicalLocation"]
            if row.get("line") is None:
                assert "region" not in location
            else:
                assert location["region"] == {"startLine": row["line"], "startColumn": row["column"]}
    evaluation, = policy["policy_rule_evaluations"]
    assert evaluation["status"] == "incomplete" and evaluation["observed"] is None
    included = sum(row["policy_scope"]["included"] for row in rows)
    assert evaluation["observed_findings"] == (0 if waived else included)
    assert evaluation["excluded"] == (0 if waived else group_count - included)
    assert policy["passed"] is False
    return sorted(expected)


def reconstruct(evidence, findings):
    document = copy.deepcopy(evidence["artifact_with_diagnostic_refs"])
    for index, check in enumerate(document["data"]["checks"]):
        if "diagnostics" not in check:
            continue
        originals = []
        for diagnostic_index, ref in enumerate(check["diagnostics"]):
            observation = findings[ref["finding_id"]]["observations"][ref["observation_index"]]
            assert observation["check_index"] == index and observation["diagnostic_index"] == diagnostic_index
            originals.append(observation["diagnostic"])
        check["diagnostics"] = originals
    return document


def malformed_cases(binary, root):
    path = root / "analysis/rust_practices.json"
    original = json.loads(path.read_text())
    prior_sarif = (root / "analysis/rqlens.sarif").read_bytes()
    prior_policy = (root / "analysis/policy_report.json").read_bytes()
    results = []
    for field, value in [("message", None), ("rule_id", ""), ("severity", "invalid"),
                         ("disposition", "invalid"), ("path", {}), ("line", 0), ("column", -1),
                         ("inventory-null", None), ("inventory-object", {}), ("inventory-non-object", [42])]:
        invalid = copy.deepcopy(original)
        if field.startswith("inventory-"):
            invalid["data"]["checks"][0]["diagnostics"] = value
        else:
            invalid["data"]["checks"][0]["diagnostics"][0][field] = value
        path.write_text(json.dumps(invalid))
        for command in ["check", "sarif"]:
            result = subprocess.run([str(binary), command], cwd=root, capture_output=True, text=True, timeout=120)
            (root / f"invalid-{field}-{command}.log").write_text(result.stdout + result.stderr)
            assert result.returncode != 0
        policy = json.loads((root / "analysis/policy_report.json").read_text())
        assert policy["artifact_status"]["rust_practices.json"]["status"] == "incompatible"
        assert policy["policy_rule_evaluations"][0]["status"] == "incomplete"
        assert (root / "analysis/rqlens.sarif").read_bytes() == prior_sarif
        results.append(field)
    path.write_text(json.dumps(original))
    restored = subprocess.run([str(binary), "check"], cwd=root, capture_output=True, text=True, timeout=120)
    assert restored.returncode == 1
    assert (root / "analysis/policy_report.json").read_bytes() == prior_policy
    return results


def run(binary, output):
    binary = binary.resolve(strict=True)
    output.mkdir(parents=True, exist_ok=False)
    results, identities = [], {}
    for name, data, configuration, groups, observations, waived in scenarios():
        root = output / name
        probe(binary, root, checks=data, extra_config=configuration)
        identities[name] = validate(root, groups, observations, waived)
        for command, file in [("check", "policy_report.json"), ("sarif", "rqlens.sarif")]:
            before = (root / "analysis" / file).read_bytes()
            process = subprocess.run([str(binary), command], cwd=root, capture_output=True, text=True, timeout=120)
            assert process.returncode == (1 if command == "check" else 0)
            assert (root / "analysis" / file).read_bytes() == before
        results.append({"case": name, "groups": groups, "observations": observations, "passed": True})
    assert identities["corroborating-observations"] == identities["reversed-invocations"]
    malformed = malformed_cases(binary, output / "corroborating-observations")
    result = {"binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(), "cases": results,
              "malformed_rejections": malformed, "same_input_exact_replays": len(results) * 2,
              "scope": "Practice projection only; unknown negative coverage and scope compatibility still abstain",
              "passed": True}
    (output / "results.json").write_text(json.dumps(result, indent=2) + "\n")
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/debug/rqlens"))
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = run(args.binary, args.output.resolve())
    print(f'{len(result["cases"])} projection cases and {len(result["malformed_rejections"])} malformed cases passed')
