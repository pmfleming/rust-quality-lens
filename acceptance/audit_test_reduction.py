#!/usr/bin/env python3
"""Audit a one-time registered-test reduction, not a permanent cap on new tests.

Rust inventories come from cargo test --workspace --locked -- --list (including
ignored tests). Python inventories come from unittest discovery in two suites.
Coverage comparison groups generic LLVM instantiations by source span. It covers
unchanged regions outside inline test modules/test-only files; it is not branch,
MC/DC, all-platform or exhaustive behavior certification.
"""
import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def rust_ids(path):
    lines = path.read_text().splitlines()
    ids = [line.removesuffix(": test") for line in lines if line.endswith(": test")]
    declared = sum(int(match.group(1)) for line in lines
                   if (match := re.fullmatch(r"(\d+) tests?, \d+ benchmarks?", line)))
    if not ids or len(ids) != declared or len(ids) != len(set(ids)):
        raise ValueError(f"incomplete, duplicate or ambiguous Rust test inventory: {path}")
    return sorted(ids)


def python_ids(directory):
    # Separate processes avoid imports/discovery state leaking between suites.
    program = '''import json, unittest
suite = unittest.TestLoader().discover(%r, pattern='test_*.py')
def ids(suite):
    for item in suite:
        if isinstance(item, unittest.TestSuite): yield from ids(item)
        else: yield item.id()
print(json.dumps(list(ids(suite))))
''' % directory
    ids = json.loads(subprocess.check_output([sys.executable, "-c", program], cwd=ROOT, text=True))
    if not ids or len(ids) != len(set(ids)) or any("_FailedTest" in item for item in ids):
        raise ValueError(f"failed or ambiguous Python discovery: {directory}")
    return sorted(ids)


def coverage(path, source_before):
    document = json.loads(path.read_text())
    if len(document["data"]) != 1:
        raise ValueError("expected a single aggregate LLVM coverage export")
    data = document["data"][0]
    functions, regions, sources = {}, {}, {}
    for function in data["functions"]:
        file = Path(function["filenames"][0])
        try:
            relative = file.relative_to(ROOT)
        except ValueError:
            continue
        if (relative.parts[0] not in {"src", "rust_helpers"}
                or file.name in {"tests.rs", "test_support.rs"}
                or file.stem.endswith("_tests") or "tests" in relative.parts):
            continue
        if relative not in sources:
            saved = source_before / relative
            text = saved.read_text() if saved.exists() else file.read_text()
            marker = re.search(r"#\[cfg\(test\)\]\s*mod tests\s*\{", text)
            prefix = text[:marker.start()] if marker else text
            if not file.read_text().startswith(prefix):
                raise ValueError(f"changed compared source prefix: {relative}")
            sources[relative] = prefix.count("\n") + 1
        limit = sources[relative]
        spans = function["regions"]
        if not spans or spans[0][0] >= limit:
            continue
        key = (str(relative), tuple(spans[0][:4]))
        functions[key] = functions.get(key, False) or function["count"] > 0
        for span in spans:
            # File ID 0 belongs to the primary source; kind 0 is a code region.
            if span[5] == 0 and span[0] < limit and span[7] == 0:
                key = (str(relative), tuple(span[:4]))
                regions[key] = regions.get(key, False) or span[4] > 0
    if not functions or not regions:
        raise ValueError("empty coverage comparison scope")
    return data["totals"], functions, regions


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    before = {"rust": rust_ids(args.evidence / "rust-before.txt")}
    after = {"rust": rust_ids(args.evidence / "rust-after.txt")}
    for directory in ["acceptance", "scripts"]:
        before[directory] = json.loads((args.evidence / f"{directory}-before.json").read_text())
        after[directory] = python_ids(directory)
    starting = sum(map(len, before.values()))
    final = sum(map(len, after.values()))
    target = round(starting * 0.67)
    if final != target:
        raise ValueError(f"expected rounded target {target}, discovered {final}")
    baseline = coverage(args.evidence / "coverage-before.json", args.evidence / "source-before")
    candidate = coverage(args.evidence / "coverage-confirmed.json", args.evidence / "source-before")
    retained = {}
    for name, old, new in zip(["function_spans", "code_regions"], baseline[1:], candidate[1:]):
        if old.keys() != new.keys():
            raise ValueError(f"{name}: compared source inventory changed")
        lost = [key for key, covered in old.items() if covered and not new[key]]
        retained[name] = {"compared": len(old), "covered_before": sum(old.values()),
                          "covered_after": sum(new.values()), "lost": lost}
        if lost:
            raise ValueError(f"{name}: coverage lost: {lost}")
    result = {
        "baseline": "Working tree after self-refactoring-2, not pristine HEAD 13889de",
        "count_scope": "Pinned Linux: Rust workspace including ignored tests; acceptance and scripts unittest suites. Fixture inputs/campaign scenarios are not registered tests.",
        "starting_count": starting, "target": target, "final_count": final,
        "reduction_percent": (starting - final) / starting * 100,
        "before": before, "after": after,
        "coverage_scope": "Unchanged source prefixes before inline test modules, excluding test-only files; generic instantiations grouped by source span. Includes small cfg(test) fixture helpers outside those modules. No branch/MC/DC certification.",
        "retained_coverage": retained,
        "whole_workspace_coverage": {"before": baseline[0], "after": candidate[0]},
        "coverage_sha256": {name: hashlib.sha256((args.evidence / name).read_bytes()).hexdigest()
                            for name in ["coverage-before.json", "coverage-confirmed.json"]},
        "baseline_test_source_sha256": {
            str(file.relative_to(args.evidence / "source-before")): hashlib.sha256(file.read_bytes()).hexdigest()
            for file in sorted((args.evidence / "source-before").rglob("*")) if file.is_file()},
        "raw_evidence": str(args.evidence),
        "limits": ["One-time count target, not a limit on future essential tests.",
                   "Most removed registrations are consolidated cases, not deleted assertions.",
                   "Coverage retention is scoped evidence, not proof of behavioral equivalence.",
                   "Saved edited source prefixes are checked; unarchived production files are assumed unchanged during this test-only pass.",
                   "Trust/release acceptance and cross-platform certification remain open."],
    }
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(f"{starting} -> {final} tests ({result['reduction_percent']:.2f}% reduction)")
    print(json.dumps(retained, indent=2))


if __name__ == "__main__":
    main()
