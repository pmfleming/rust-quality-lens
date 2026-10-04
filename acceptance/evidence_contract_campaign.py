#!/usr/bin/env python3
"""Adversarial native policy-ingestion cases, not producer/detector certification."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import subprocess


FILES = {
    "reliability_findings.json": ("reliability", "records", []),
    "rust_practices.json": ("practices", "data", {"checks": []}),
    "architecture_rules.json": ("architecture-rules", "data", {"violations": []}),
    "test_quality.json": ("test-quality", "data", {"findings": []}),
}


def cases():
    rows = []
    for rule, file in [("rust.reliability.unwrap", "reliability_findings.json"),
                       ("rust.security.rustsec", "rust_practices.json")]:
        for mutation in ["clean", "violation", "missing", "corrupt", "stale",
                         "partial", "unsupported-schema"]:
            rows.append((rule, "error", file, mutation))
    for mutation in ["absent-check", "skipped", "unavailable", "timed-out", "mixed-skipped"]:
        rows.append(("rust.security.rustsec", "error", "rust_practices.json", mutation))
    for rule in ["rust.unknown.typo", "rust.reliability.unreachable",
                 "rust.compiler.diagnostic", "external.other.rule"]:
        rows.append((rule, "error", "rust_practices.json", "unsupported-rule"))
    for rule, mutation in [("boundary", "architecture-clean"), ("boundary", "architecture-disabled"),
                           ("rust.reliability.unwrap", "architecture-collision")]:
        rows.append((rule, "error", "architecture_rules.json", mutation))
    rows.append(("rust.unknown.typo", "off", "rust_practices.json", "missing"))
    rows.append(("rust.security.rustsec", "warning", "rust_practices.json", "skipped"))
    return rows


def expected(level, mutation):
    if level == "off":
        return "disabled", True
    status = {"clean": "passed", "violation": "exceeded", "architecture-clean": "passed"}.get(mutation, "incomplete")
    return status, level != "error" or status == "passed"


def mutate(documents, rule, file, mutation):
    check = {"rule_id": rule, "severity": "error", "status": "passed", "title": "fixture"}
    documents["rust_practices.json"]["data"]["checks"] = [check]
    document = documents[file]
    if mutation == "violation":
        if file == "rust_practices.json":
            check["status"] = "failed"
        else:
            document["records"] = [{"rule_id": rule, "severity": "error", "scope": "production",
                                    "path": "src/lib.rs", "line": 1, "message": "fixture"}]
    elif mutation == "missing":
        del documents[file]
    elif mutation == "corrupt":
        documents[file] = "{broken"
    elif mutation == "stale":
        document["input_fingerprint"]["digest"] = "stale-fixture"
    elif mutation == "partial":
        document["measurement_confidence"] = {"complete": False, "partial": True}
    elif mutation == "unsupported-schema":
        document["schema_version"] = 999
    elif mutation == "absent-check":
        document["data"]["checks"] = []
    elif mutation in {"skipped", "unavailable", "timed-out"}:
        check["status"] = mutation
    elif mutation == "mixed-skipped":
        document["data"]["checks"].append(dict(check, status="skipped"))


def run(binary, output):
    binary = binary.resolve(strict=True)
    output.mkdir(parents=True, exist_ok=False)
    results = []
    for index, (rule, level, file, mutation) in enumerate(cases()):
        root = output / str(index)
        (root / "src").mkdir(parents=True)
        (root / "src/lib.rs").write_text("pub fn example() {}\n")
        (root / "Cargo.toml").write_text('[package]\nname="evidence-fixture"\nversion="0.1.0"\nedition="2024"\n')
        (root / "rqlens.toml").write_text(
            'source_roots=["src"]\noutput_dir="analysis"\n[rust]\nidentity_resolution="disabled"\n'
            f'[policy.rules."{rule}"]\nlevel="{level}"\nmax=0\n')
        if mutation.startswith("architecture-"):
            with (root / "rqlens.toml").open("a") as config:
                architecture_level = "off" if mutation == "architecture-disabled" else "error"
                config.write(f'[[architecture.rules]]\nid="{rule}"\nfrom=["*"]\ndeny=["*"]\nlevel="{architecture_level}"\n')
        with (root / "measure.log").open("w") as log:
            subprocess.run([str(binary), "measure", "reliability"], cwd=root,
                           stdout=log, stderr=subprocess.STDOUT, timeout=120, check=True)
        seed = json.loads((root / "analysis/reliability_findings.json").read_text())
        documents = {}
        for name, (tool, key, payload) in FILES.items():
            documents[name] = {
                "schema_version": seed["schema_version"], "risk_model_version": seed["risk_model_version"],
                "tool": tool, "input_fingerprint": copy.deepcopy(seed["input_fingerprint"]),
                "measurement_confidence": {"complete": True, "partial": False}, key: copy.deepcopy(payload),
            }
        mutate(documents, rule, file, mutation)
        for name in FILES:
            path = root / "analysis" / name
            if name not in documents:
                path.unlink(missing_ok=True)
            else:
                data = documents[name]
                path.write_text(data if isinstance(data, str) else json.dumps(data))
        process = subprocess.run([str(binary), "check"], cwd=root, capture_output=True,
                                 text=True, timeout=120)
        (root / "stdout.json").write_text(process.stdout)
        (root / "stderr.log").write_text(process.stderr)
        report = json.loads((root / "analysis/policy_report.json").read_text())
        stdout = json.loads(process.stdout)
        evaluation, = report["policy_rule_evaluations"]
        status, passed = expected(level, mutation)
        complete = status in {"passed", "exceeded"}
        checks = {
            "status": evaluation["status"] == status,
            "exit": (process.returncode == 0) == passed,
            "passed": report["passed"] == passed,
            "cli_json_agreement": stdout == report,
            "completeness": evaluation.get("evaluation_complete") == complete,
            "known_count": not complete or evaluation.get("observed") == int(mutation == "violation"),
            "unknown_count": complete or evaluation.get("observed") is None,
            "unknown_excess": complete or evaluation.get("excess") is None,
            "reason": status != "incomplete" or bool(evaluation.get("reason")),
            "no_invented_violation": status != "incomplete" or report["policy_rule_violations"] == [],
        }
        results.append({"case": index, "rule": rule, "level": level, "mutation": mutation,
                        "expected_status": status, "actual_status": evaluation["status"],
                        "checks": checks, "passed": all(checks.values())})
    summary = {"binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
               "scope": "Synthetic policy-ingestion fixtures with native current fingerprints; not detector/backend certification",
               "cases": results, "passed": all(row["passed"] for row in results)}
    (output / "results.json").write_text(json.dumps(summary, indent=2) + "\n")
    return summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/debug/rqlens"))
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = run(args.binary, args.output.resolve())
    print(f'{sum(row["passed"] for row in result["cases"])}/{len(result["cases"])} cases passed')
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
