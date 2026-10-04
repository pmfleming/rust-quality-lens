#!/usr/bin/env python3
"""Probe nested-diagnostic agreement and evidence retention (not E4/E6 certification)."""
import argparse
import copy
import json
from pathlib import Path
import subprocess

from evidence_contract_campaign import FILES


def projection_agrees(policy_rows, sarif_rows):
    return (len(policy_rows) == len(sarif_rows) and all(row is not None for row in sarif_rows)
            and sorted(policy_rows, key=lambda row: row.get("id", ""))
            == sorted(sarif_rows, key=lambda row: row.get("id", "")))


def probe(binary, output, *, checks=None, extra_config=""):
    binary = binary.resolve(strict=True)
    output.mkdir(parents=True, exist_ok=False)
    (output / "src").mkdir()
    (output / "src/lib.rs").write_text("pub fn example() {}\n")
    (output / "Cargo.toml").write_text('[package]\nname="reporter-probe"\nversion="0.1.0"\nedition="2024"\n')
    (output / "rqlens.toml").write_text(
        'source_roots=["src"]\noutput_dir="analysis"\n[rust]\nidentity_resolution="disabled"\n'
        '[policy.rules."rust.compiler.diagnostic"]\nlevel="error"\nmax=0\n' + extra_config)
    with (output / "measure.log").open("w") as log:
        subprocess.run([str(binary), "measure", "reliability"], cwd=output, stdout=log,
                       stderr=subprocess.STDOUT, timeout=120, check=True)
    seed = json.loads((output / "analysis/reliability_findings.json").read_text())
    diagnostic = {"rule_id": "rust.compiler.diagnostic", "severity": "error",
                  "path": "src/lib.rs", "line": 1, "column": 1, "message": "shared observation"}
    for file, (tool, key, payload) in FILES.items():
        document = {"schema_version": seed["schema_version"], "risk_model_version": seed["risk_model_version"],
                    "tool": tool, "input_fingerprint": seed["input_fingerprint"],
                    "measurement_confidence": {"complete": True, "partial": False}, key: copy.deepcopy(payload)}
        if file == "rust_practices.json":
            document[key]["checks"] = checks if checks is not None else [
                {"rule_id": f"rust.official.{engine}", "title": engine, "status": "failed", "severity": "error",
                 "tool": "cargo", "tool_version": "fixture-1", "diagnostics": [diagnostic],
                 "evidence": {"command": ["cargo", engine], "scope": "same fixture"}}
                for engine in ["cargo-check", "clippy"]]
        (output / "analysis" / file).write_text(json.dumps(document))
    process = subprocess.run([str(binary), "check"], cwd=output, capture_output=True, text=True, timeout=120)
    (output / "policy.stdout.json").write_text(process.stdout)
    (output / "policy.stderr.log").write_text(process.stderr)
    with (output / "sarif.log").open("w") as log:
        subprocess.run([str(binary), "sarif"], cwd=output, stdout=log,
                       stderr=subprocess.STDOUT, timeout=120, check=True)
    policy = json.loads((output / "analysis/policy_report.json").read_text())
    sarif = json.loads((output / "analysis/rqlens.sarif").read_text())
    rows = [row for row in sarif["runs"][0]["results"] if row["ruleId"] == diagnostic["rule_id"]]
    policy_rows = (policy["practice_failures"] + policy["practice_warnings"]
                   + [item["finding"] for item in policy["waived_findings"]])
    diagnostic_rows = [row for row in policy_rows if row.get("rule_id") == diagnostic["rule_id"]]
    sarif_rows = [row.get("properties", {}).get("rqlens_finding") for row in rows]
    result = {
        "scope": "Synthetic nested diagnostics; not engine accuracy, applicability or same-scope disagreement certification",
        "projection_agreement": projection_agrees(diagnostic_rows, sarif_rows),
        "invocation_evidence_agreement": policy.get("practice_evidence") is not None and policy.get("practice_evidence") == sarif["runs"][0]["properties"].get("practice_evidence"),
        "retained_observations": sum(len(row.get("observations", [])) for row in diagnostic_rows),
        "input_observations": sum(len(check.get("diagnostics", [])) for check in checks) if checks is not None else 2,
        "policy_evaluation": policy["policy_rule_evaluations"][0],
        "sarif_diagnostic_results": rows,
        "criterion_status": {"E4": "open", "E6": "open"},
        "limitations": ["Per-rule compiler enablement/negative coverage remains unsupported",
                        "Exact observation equality does not establish independent corroboration or compatible scopes",
                        "Missing output alone does not establish engine disagreement"],
    }
    (output / "results.json").write_text(json.dumps(result, indent=2) + "\n")
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/debug/rqlens"))
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = probe(args.binary, args.output.resolve())
    print(json.dumps(result, indent=2))
