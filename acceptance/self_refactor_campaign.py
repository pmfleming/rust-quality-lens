#!/usr/bin/env python3
"""Frozen/candidate clone parity and timestamp-only static repeatability.

Uses unchanged repository configuration; writes only existing analysis outputs
and a fresh evidence directory. This is not whole-product determinism evidence.
"""
import argparse
import hashlib
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STATIC = {
    "hotspots": "hotspots.json",
    "clones": "clones.json",
    "locality": "locality_metrics.json",
    "leverage": "leverage_metrics.json",
    "escape-hatches": "rust_escape_hatches.json",
}


def normalized(value):
    if isinstance(value, dict):
        return {key: normalized(item) for key, item in value.items()
                if key not in {"generated_at", "measured_at"}}
    if isinstance(value, list):
        return [normalized(item) for item in value]
    return value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--frozen", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, default=ROOT / "target/debug/rqlens")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    binaries = {name: path.resolve() for name, path in
                [("frozen", args.frozen), ("candidate", args.candidate)]}
    results = {"binary_sha256": {name: hashlib.sha256(path.read_bytes()).hexdigest()
                                for name, path in binaries.items()},
               "normalization": "Only generated_at and measured_at removed recursively; arrays preserved",
               "clone_fixtures": [], "self_replays": [], "self_differential": []}

    def measure(binary, tool, config, artifact, label):
        process = subprocess.run([str(binaries[binary]), "measure", tool, "--config", str(config)],
                                 cwd=ROOT, capture_output=True, text=True, timeout=180)
        (args.output / f"{label}.log").write_text(process.stdout + process.stderr)
        if process.returncode:
            raise RuntimeError(f"{label}: measurement failed: {process.returncode}")
        content = artifact.read_bytes()
        (args.output / f"{label}.json").write_bytes(content)
        return normalized(json.loads(content))

    # Existing native CLI fixtures include duplicate bodies in source and tests.
    engines = set()
    for name, directory in [
        ("mini", ROOT / "tests/fixtures/mini_rust_project"),
        ("golden", ROOT / "tests/fixtures/golden_rust_project"),
        ("clone-confidence", ROOT / "target/test-fixtures/clone-confidence"),
    ]:
        config = directory / "rqlens.toml"
        artifact = directory / "target/analysis/clones.json"
        pair = [measure(binary, "clones", config, artifact, f"{name}-{binary}")
                for binary in binaries]
        if pair[0] != pair[1]:
            raise AssertionError(f"{name}: clone artifact differs")
        counts = {engine: sum(row["engine"] == engine for row in pair[0]["records"])
                  for engine in {row["engine"] for row in pair[0]["records"]}}
        engines.update(counts)
        results["clone_fixtures"].append({"fixture": name, "equal": True, "engines": counts})
    if not {"ast", "test-ast"} <= engines:
        raise AssertionError("clone parity lacks positive source/test AST evidence")

    for tool, filename in STATIC.items():
        config, artifact = ROOT / "rqlens.toml", ROOT / "target/analysis" / filename
        frozen = measure("frozen", tool, config, artifact, f"self-{tool}-frozen")
        pair = [measure("candidate", tool, config, artifact, f"self-{tool}-{index}")
                for index in (1, 2)]
        if frozen != pair[0] or pair[0] != pair[1]:
            raise AssertionError(f"{tool}: frozen/candidate/replay disagreement")
        results["self_differential"].append(tool)
        results["self_replays"].append(tool)
    (args.output / "results.json").write_text(json.dumps(results, indent=2) + "\n")
    print(json.dumps(results, indent=2))


if __name__ == "__main__":
    main()
