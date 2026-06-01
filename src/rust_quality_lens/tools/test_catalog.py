import argparse
import json
import re
import subprocess
import time
from collections import Counter
import os
from pathlib import Path
from typing import Any, Dict, List

from common import measurement_confidence, merge_confidences, rust_facts_for_paths, source_confidence
from lens_rules import RULESET_ID, RULESET_VERSION, classify_path
from report_modes import add_mode_argument, analysis_path, emit_report

DEFAULT_OUTPUT = Path("correctness_review.json")
VISIBILITY_OUTPUT = Path("target/analysis/correctness_review.json")
TEST_CATALOG_OUTPUT = Path("target/analysis/test_catalog.json")
DESCRIPTIONS_PATH = Path("scripts/test_descriptions.json")
MANIFEST_PATH = Path("Cargo.toml")


def output_path(path: Path) -> Path:
    return analysis_path(path)

def load_descriptions() -> Dict[str, str]:
    if not DESCRIPTIONS_PATH.exists():
        return {}
    try:
        payload = json.loads(DESCRIPTIONS_PATH.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return {}
    return {str(key): trim_description(str(value)) for key, value in payload.items()}


def trim_description(value: str) -> str:
    words = value.strip().split()
    if len(words) <= 9:
        return " ".join(words)
    return " ".join(words[:9]).rstrip(".,;:") + "."


def layer_for_path(path: Path) -> str:
    return classify_path(path)


def module_for_path(path: Path) -> str:
    if path.as_posix().startswith("src/"):
        without_prefix = path.with_suffix("").as_posix()[4:]
        if without_prefix.endswith("/mod"):
            without_prefix = without_prefix[:-4]
        return without_prefix.replace("/", "::")
    return path.stem


def title_from_name(name: str) -> str:
    tokens = [token for token in re.split(r"[_\W]+", name) if token]
    useful = [token for token in tokens if token not in {"test", "should", "when"}]
    if not useful:
        useful = tokens or ["behavior"]
    return trim_description(" ".join(["Checks"] + useful[:7]) + ".")


def description_for(path: Path, name: str, overrides: Dict[str, str]) -> str:
    keys = [
        f"{path.as_posix()}::{name}",
        name,
        path.name,
    ]
    for key in keys:
        if key in overrides:
            return overrides[key]
    return title_from_name(name)


def cargo_manifest_rust_targets() -> List[Path]:
    if not MANIFEST_PATH.exists():
        return []
    try:
        text = MANIFEST_PATH.read_text(encoding="utf-8")
    except OSError:
        return []
    targets: List[Path] = []
    for block in re.split(r"(?m)^\s*\[\[", text):
        if not block.startswith(("bin]]", "test]]", "bench]]", "example]]")):
            continue
        path_match = re.search(r'(?m)^\s*path\s*=\s*"([^"]+\.rs)"', block)
        if path_match:
            targets.append(Path(path_match.group(1)))
    lib_match = re.search(r'(?ms)^\s*\[lib\]\s+.*?^\s*path\s*=\s*"([^"]+\.rs)"', text)
    if lib_match:
        targets.append(Path(lib_match.group(1)))
    targets.extend(Path("src/bin").glob("*.rs"))
    return targets


def default_discovery_paths() -> List[Path]:
    env_roots = [
        Path(path)
        for path in os.environ.get("RQLENS_SOURCE_ROOTS", "").split(os.pathsep)
        if path
    ]
    roots = env_roots or [Path("src")]
    roots.extend(Path(path) for path in ("tests", "benches", "examples"))
    roots.extend(cargo_manifest_rust_targets())
    return dedupe_paths(roots)


def supplemental_discovery_paths() -> List[Path]:
    roots = [Path(path) for path in ("tests", "benches", "examples")]
    roots.extend(cargo_manifest_rust_targets())
    return roots


def dedupe_paths(paths: List[Path]) -> List[Path]:
    seen = set()
    result: List[Path] = []
    for path in paths:
        key = path.as_posix()
        if key in seen:
            continue
        seen.add(key)
        result.append(path)
    return result


def discover_tests(paths: List[Path] | None = None) -> tuple[List[Dict[str, Any]], Dict[str, object]]:
    overrides = load_descriptions()
    tests: List[Dict[str, Any]] = []
    discovery_paths = dedupe_paths(
        [*(paths or default_discovery_paths()), *supplemental_discovery_paths()]
    )
    facts = rust_facts_for_paths(discovery_paths)
    unsupported = [
        str(pattern)
        for fact in facts
        if isinstance(fact, dict)
        for pattern in fact.get("unsupported_patterns", [])
    ]
    confidence = merge_confidences(
        source_confidence(discovery_paths, facts=facts),
        measurement_confidence(unsupported_pattern=unsupported),
    )
    for fact in facts:
        if fact.get("parse_status") != "ok":
            continue
        path = Path(str(fact.get("path", "")))
        kind = test_kind_for_path(path)
        for raw_test in fact.get("tests", []):
            if not isinstance(raw_test, dict):
                continue
            name = str(raw_test.get("name", ""))
            qualified_name = str(raw_test.get("qualified_name", name))
            line = int(raw_test.get("line", 0))
            test_id = f"{path.as_posix()}::{qualified_name}"
            tests.append(
                {
                    "id": test_id,
                    "name": name,
                    "qualified_name": qualified_name,
                    "path": path.as_posix(),
                    "line": line,
                    "layer": layer_for_path(path),
                    "module": module_for_path(path),
                    "description": description_for(path, name, overrides),
                    "kind": kind,
                    "last_status": "unknown",
                    "last_duration": None,
                    "command": f"cargo test {name}",
                }
            )
    return tests, confidence


def test_kind_for_path(path: Path) -> str:
    normalized = path.as_posix()
    if normalized.startswith("tests/"):
        return "integration"
    if normalized.startswith("benches/"):
        return "benchmark"
    if normalized.startswith("examples/"):
        return "example"
    if normalized.startswith("src/bin/"):
        return "binary"
    return "inline"


def run_tests() -> Dict[str, Dict[str, Any]]:
    started = time.perf_counter()
    try:
        result = subprocess.run(["cargo", "test"], capture_output=True, text=True)
    except FileNotFoundError:
        return {}
    duration = time.perf_counter() - started
    statuses = parse_cargo_test_statuses(
        result.stdout,
        returncode=result.returncode,
        duration=duration,
        stderr=result.stderr,
    )
    return statuses


def parse_cargo_test_statuses(
    stdout: str,
    *,
    returncode: int,
    duration: float | None = None,
    stderr: str = "",
) -> Dict[str, Dict[str, Any]]:
    statuses: Dict[str, Dict[str, Any]] = {}
    current_path: str | None = None
    for line in stdout.splitlines():
        running_match = re.match(
            r"\s*Running\s+(?:(?:unittests|tests?)\s+)?(.+?\.rs)(?:\s+\(|$)",
            line,
        )
        if running_match:
            current_path = normalize_cargo_path(running_match.group(1))
            continue
        match = re.match(r"test\s+(.+?)\s+\.\.\.\s+(ok|FAILED|ignored)", line.strip())
        if not match:
            continue
        full_name = match.group(1)
        status = {"ok": "passed", "FAILED": "failed", "ignored": "skipped"}[match.group(2)]
        status_record = {"status": status, "duration": None}
        statuses[full_name] = status_record
        if current_path:
            statuses[f"{current_path}::{full_name}"] = status_record
    statuses["__run__"] = {
        "status": "passed" if returncode == 0 else "failed",
        "duration": duration,
        "stdout_tail": "\n".join(stdout.splitlines()[-40:]),
        "stderr_tail": "\n".join(stderr.splitlines()[-40:]),
    }
    return statuses


def normalize_cargo_path(value: str) -> str:
    return value.strip().strip('"').replace("\\", "/")


def attach_statuses(tests: List[Dict[str, Any]], statuses: Dict[str, Dict[str, Any]]) -> None:
    name_counts = Counter(str(item.get("name", "")) for item in tests)
    qualified_counts = Counter(str(item.get("qualified_name", "")) for item in tests)
    for item in tests:
        candidates = [
            str(item.get("id", "")),
            f"{item.get('path', '')}::{item.get('qualified_name', item.get('name', ''))}",
        ]
        qualified_name = str(item.get("qualified_name", ""))
        if qualified_name and qualified_counts[qualified_name] == 1:
            candidates.append(qualified_name)
        name = str(item.get("name", ""))
        if name and name_counts[name] == 1:
            candidates.append(name)

        status = next((statuses[key] for key in candidates if key in statuses), None)
        if status:
            item["last_status"] = status["status"]
            item["last_duration"] = status["duration"]


def build_payload(run: bool = False, paths: List[Path] | None = None) -> Dict[str, Any]:
    tests, confidence = discover_tests(paths)
    statuses = run_tests() if run else {}
    attach_statuses(tests, statuses)

    by_layer: Dict[str, Dict[str, int]] = {}
    for item in tests:
        layer = by_layer.setdefault(
            item["layer"],
            {"total": 0, "passed": 0, "failed": 0, "skipped": 0, "unknown": 0},
        )
        layer["total"] += 1
        layer[item["last_status"]] = layer.get(item["last_status"], 0) + 1

    summary = {
        "test_count": len(tests),
        "integration_count": sum(1 for item in tests if item["kind"] == "integration"),
        "inline_count": sum(1 for item in tests if item["kind"] == "inline"),
        "benchmark_count": sum(1 for item in tests if item["kind"] == "benchmark"),
        "example_count": sum(1 for item in tests if item["kind"] == "example"),
        "binary_count": sum(1 for item in tests if item["kind"] == "binary"),
        "layers": len(by_layer),
        "failed": sum(1 for item in tests if item["last_status"] == "failed"),
        "unknown": sum(1 for item in tests if item["last_status"] == "unknown"),
        "last_run": statuses.get("__run__"),
    }
    return {
        "version": 1,
        "generated_from": "scripts/test_catalog.py",
        "layer_ruleset": {"id": RULESET_ID, "version": RULESET_VERSION},
        "summary": summary,
        "measurement_confidence": confidence,
        "layers": [
            {
                "name": layer,
                **counts,
                "failed_ratio": (counts["failed"] / counts["total"]) if counts["total"] else 0.0,
            }
            for layer, counts in sorted(by_layer.items(), key=lambda entry: entry[0])
        ],
        "tests": tests,
    }


def render_cli(payload: object) -> str:
    data = payload if isinstance(payload, dict) else {}
    summary = data.get("summary", {})
    last_run = summary.get("last_run")
    last_run_status = last_run.get("status") if isinstance(last_run, dict) else None
    lines = [
        "Correctness Review",
        f"Tests: {summary.get('test_count', 0)}",
        f"Layers: {summary.get('layers', 0)}",
        f"Failed: {summary.get('failed', 0)}",
    ]
    if last_run_status:
        lines.append(f"Last run: {last_run_status}")
    return "\n".join(lines)


def requested_run_failed(payload: object, *, run: bool) -> bool:
    if not run:
        return False
    if not isinstance(payload, dict):
        return True
    summary = payload.get("summary")
    if not isinstance(summary, dict):
        return True
    last_run = summary.get("last_run")
    return not isinstance(last_run, dict) or last_run.get("status") != "passed"


def main() -> None:
    parser = argparse.ArgumentParser(description="Emit categorized correctness test catalog")
    parser.add_argument("--paths", nargs="+", default=None, help="Paths to analyze")
    parser.add_argument("--output", type=Path, default=None)
    parser.add_argument("--run", action="store_true", help="Run cargo test and attach status")
    add_mode_argument(parser)
    args = parser.parse_args()
    payload = build_payload(
        run=args.run,
        paths=[Path(path) for path in args.paths] if args.paths else None,
    )
    if args.mode == "visibility":
        test_catalog_output = output_path(TEST_CATALOG_OUTPUT)
        test_catalog_output.parent.mkdir(parents=True, exist_ok=True)
        test_catalog_output.write_text(json.dumps(payload["tests"], indent=2) + "\n", encoding="utf-8")
    emit_report(
        payload,
        mode=args.mode,
        output_path=args.output,
        visibility_path=VISIBILITY_OUTPUT,
        cli_renderer=render_cli,
        label="correctness review",
    )
    if requested_run_failed(payload, run=args.run):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
