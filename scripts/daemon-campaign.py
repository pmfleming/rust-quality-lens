#!/usr/bin/env python3
"""Capture immutable RQLens runs; compare evidence, never treat missing data as zero.

Uses each project's existing policy/feature configuration and Nix development
shell. It does not launch a daemon or change its source/configuration.
"""
import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tomllib

LENS = Path(__file__).resolve().parents[1]


def git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args], text=True).strip()


def revision(root):
    return {"head": git(root, "rev-parse", "HEAD"), "dirty": bool(git(root, "status", "--porcelain"))}


def override(text, section, key, value):
    """Replace a scalar in one TOML table, leaving all other policy intact."""
    lines = text.splitlines()
    start = 0
    if section:
        header = f"[{section}]"
        if header not in lines:
            lines += ["", header]
        start = lines.index(header) + 1
    end = next((i for i in range(start, len(lines)) if lines[i].lstrip().startswith("[")), len(lines))
    replacement = f"{key} = {json.dumps(value)}"
    for i in range(start, end):
        if re.match(rf"\s*{re.escape(key)}\s*=", lines[i]):
            lines[i] = replacement
            break
    else:
        lines.insert(end, replacement)
    return "\n".join(lines) + "\n"


def command(args, cwd, log, timeout=1800):
    profiles = log.parent / "profiles"
    profiles.mkdir(exist_ok=True)
    env = {**os.environ, "LLVM_PROFILE_FILE": str(profiles.resolve() / "%m_%p.profraw")}
    with log.open("w") as output:
        try:
            result = subprocess.run(args, cwd=cwd, env=env, stdout=output, stderr=subprocess.STDOUT, timeout=timeout)
            return {"command": args, "exit_code": result.returncode, "log": str(log)}
        except subprocess.TimeoutExpired:
            return {"command": args, "exit_code": None, "status": "timed-out", "log": str(log)}


def collect(root, destination, binary):
    before = revision(root)
    if before["dirty"]:
        raise RuntimeError(f"{root} is dirty; checkpoint source before capturing evidence")
    destination.mkdir(parents=True, exist_ok=False)
    text = (root / "rqlens.toml").read_text()
    text = override(text, "", "project_root", str(root))
    text = override(text, "", "output_dir", str(destination / "analysis"))
    text = override(text, "verification", "locked", True)
    config = destination / "rqlens.toml"
    config.write_text(text)
    # Check generated TOML before running tools. No project policy is weakened.
    tomllib.loads(text)
    results = []
    for args, log in [(["measure", "all"], "measure.log"), (["verify"], "verify.log")]:
        results.append(command([str(binary), *args, "--config", str(config)], root, destination / log))
    after = revision(root)
    provenance = {
        "project": root.name, "root": str(root), "before": before, "after": after,
        "lens": revision(LENS), "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "commands": results, "source_stable": before == after,
    }
    (destination / "run.json").write_text(json.dumps(provenance, indent=2) + "\n")
    summarize(destination)
    return 0 if before == after and all(row["exit_code"] == 0 for row in results) else 1


def relative(root, raw):
    path = Path(raw)
    if not path.is_absolute():
        return path.as_posix()
    try:
        return path.relative_to(root).as_posix()
    except ValueError:
        return None


def own_source(root, row):
    raw = row.get("path") or row.get("file_path") or ""
    path = relative(root, raw)
    return bool(path and path.startswith("src/"))


def production_function(root, row):
    if not own_source(root, row):
        return False
    path = relative(root, row.get("path", ""))
    parts = [*Path(path).parts[:-1], Path(path).stem, *row.get("name", "").split("::")]
    return not any(part == "tests" or part.endswith("_tests") for part in parts)


def distribution(rows, field):
    values = [row[field] for row in rows if isinstance(row.get(field), (int, float))]
    return {"count": len(values), "sum": round(sum(values), 2), "max": max(values),
            "mean": round(sum(values) / len(values), 2)} if values else None


def summarize(destination):
    run = json.loads((destination / "run.json").read_text())
    root = Path(run["root"])
    documents = {path.name: json.loads(path.read_text()) for path in (destination / "analysis").glob("*.json")}
    status = {name: {"tool": document.get("tool"), "confidence": document.get("measurement_confidence"),
                     "fingerprint": document.get("input_fingerprint")}
              for name, document in documents.items() if "tool" in document}
    hotspots = documents.get("hotspots.json", {}).get("records")
    functions = [row for row in hotspots or [] if row.get("kind") == "function" and production_function(root, row)]
    modules = [row for row in hotspots or [] if row.get("kind") == "module" and own_source(root, row)]
    topology = {}
    for filename, metric in [("locality_metrics.json", "locality_score"), ("leverage_metrics.json", "leverage_score")]:
        rows = documents.get(filename, {}).get("records")
        topology[metric] = None if rows is None else {
            row["module_id"]: {"path": relative(root, row.get("path", "")), metric: row.get(metric),
                               "outbound": row.get("outbound_dependencies"), "inbound": row.get("inbound_dependencies", row.get("reach"))}
            for row in rows if own_source(root, row)}
    clones = documents.get("clones.json", {}).get("records")
    own_clones = [row for row in clones or [] if any(own_source(root, item) for item in row.get("instances", []))]
    escapes = documents.get("rust_escape_hatches.json", {}).get("records")
    practices = documents.get("rust_practices.json", {}).get("data", {})
    reliability = documents.get("reliability_findings.json", {}).get("records")
    escape_counts = Counter()
    for row in escapes or []:
        if own_source(root, row):
            escape_counts.update(row.get("counts", {}))
    summary = {
        "schema_version": 2, "run": run,
        "scope": "src/ functions excluding tests and *_tests modules/files by naming convention, NOT cfg-aware production classification; src LOC includes tests; dependencies remain in raw evidence",
        "artifact_status": status,
        "src_nonblank_lines": None if hotspots is None else sum(row.get("sloc", 0) for row in modules),
        "production_functions": None if hotspots is None else len(functions),
        "complexity": {field: distribution(functions, field) for field in ["cognitive_complexity", "cyclomatic_complexity", "score"]},
        "effort": {"status": "unsupported", "reason": "hotspot score is a heuristic, not a calibrated effort estimate"},
        "top_functions": sorted(functions, key=lambda row: row.get("cognitive_complexity") or 0, reverse=True)[:20],
        "topology": topology,
        "clone_groups_touching_src": None if clones is None else dict(Counter(row.get("engine") for row in own_clones)),
        "escape_records_in_src": None if escapes is None else sum(own_source(root, row) for row in escapes),
        "escape_counts_in_src": None if escapes is None else dict(escape_counts),
        "production_panic_findings_in_src": None if reliability is None else sum(
            own_source(root, row) and row.get("kind") == "panic-path" and row.get("scope") == "production"
            for row in reliability),
        "verification": practices.get("summary"),
        "compiler_dead_code_diagnostics": [diagnostic for check in practices.get("checks", []) for diagnostic in check.get("diagnostics", []) if diagnostic.get("rule_id") == "dead_code"],
        "dead_code_scope": "compiler diagnostics only; no whole-program dead-code claim",
    }
    (destination / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    return summary


def compare(before, after):
    old = summarize(before)
    new = summarize(after)
    for label, summary in [("before", old), ("after", new)]:
        if summary["run"].get("source_stable") is not True:
            raise RuntimeError(f"{label}: source changed during capture; recapture before comparing")
    policies = []
    for directory in [before, after]:
        policy = tomllib.loads((directory / "rqlens.toml").read_text())
        policy.pop("output_dir", None)
        policies.append(policy)
    if policies[0] != policies[1]:
        raise RuntimeError("Measurement configuration differs; recapture with identical scope and policy")
    same_lens = old["run"]["binary_sha256"] == new["run"]["binary_sha256"]
    required = ["hotspots.json", "clones.json", "rust_escape_hatches.json", "locality_metrics.json", "leverage_metrics.json", "reliability_findings.json"]
    warnings = [f"{label}: {name} is missing or partial" for label, summary in [("before", old), ("after", new)]
                for name in required if (summary["artifact_status"].get(name, {}).get("confidence") or {}).get("complete") is not True]
    warnings += [f"{label}: command did not succeed: {row['command']}"
                 for label, summary in [("before", old), ("after", new)]
                 for row in summary["run"]["commands"] if row.get("exit_code") != 0]
    warnings += [f"{label}: verification is missing or nonconformant"
                 for label, summary in [("before", old), ("after", new)]
                 if (summary.get("verification") or {}).get("conformant") is not True]
    result = {"same_lens_binary": same_lens, "comparison_status": "provisional" if warnings else "observed",
              "warnings": warnings, "before": str(before), "after": str(after), "deltas": {},
              "verification": {"before": old["verification"], "after": new["verification"]}}
    if not same_lens:
        raise RuntimeError("Remeasure both source revisions with the same lens binary before comparing")
    for metric in ["src_nonblank_lines", "production_functions", "escape_records_in_src", "production_panic_findings_in_src"]:
        a, b = old[metric], new[metric]
        result["deltas"][metric] = None if a is None or b is None else b - a
    result["complexity"] = {key: {"before": old["complexity"][key], "after": new["complexity"][key]} for key in old["complexity"]}
    result["clone_groups_touching_src"] = {"before": old["clone_groups_touching_src"], "after": new["clone_groups_touching_src"]}
    result["escape_counts_in_src"] = {"before": old["escape_counts_in_src"], "after": new["escape_counts_in_src"]}
    result["topology_changes"] = {}
    for metric in old["topology"]:
        a, b = old["topology"][metric], new["topology"][metric]
        result["topology_changes"][metric] = None if a is None or b is None else {
            key: {"before": a.get(key), "after": b.get(key)} for key in sorted(a.keys() | b.keys()) if a.get(key) != b.get(key)}
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="action", required=True)
    capture = sub.add_parser("capture")
    capture.add_argument("project", type=Path)
    capture.add_argument("destination", type=Path)
    capture.add_argument("--binary", type=Path, default=LENS / "target/debug/rqlens")
    capture.add_argument("--inside-shell", action="store_true", help=argparse.SUPPRESS)
    report = sub.add_parser("summarize")
    report.add_argument("destination", type=Path)
    delta = sub.add_parser("compare")
    delta.add_argument("before", type=Path)
    delta.add_argument("after", type=Path)
    args = parser.parse_args()
    if args.action == "capture":
        root, destination, binary = args.project.resolve(), args.destination.resolve(), args.binary.resolve()
        if args.inside_shell:
            return collect(root, destination, binary)
        return subprocess.call(["nix", "develop", "--offline", str(root), "--command", sys.executable,
                                str(Path(__file__).resolve()), "capture", str(root), str(destination),
                                "--binary", str(binary), "--inside-shell"])
    if args.action == "summarize":
        summarize(args.destination)
    else:
        print(json.dumps(compare(args.before, args.after), indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
