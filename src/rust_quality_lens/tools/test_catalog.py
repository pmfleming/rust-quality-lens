import argparse
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path
from typing import Any, Dict, Iterable, List, Optional

from report_modes import add_mode_argument, emit_report

DEFAULT_OUTPUT = Path("correctness_review.json")
VISIBILITY_OUTPUT = Path("target/analysis/correctness_review.json")
TEST_CATALOG_OUTPUT = Path("target/analysis/test_catalog.json")
DESCRIPTIONS_PATH = Path("scripts/test_descriptions.json")
MANIFEST_PATH = Path("Cargo.toml")


def output_path(path: Path) -> Path:
    output_dir = os.environ.get("RQLENS_OUTPUT_DIR")
    if output_dir and not path.is_absolute() and "target/analysis" in path.as_posix():
        return Path(output_dir) / path.name
    return path

LAYER_RULES = [
    ("App Shell and State", ("app_state", "app_tests.rs")),
    ("Commands and Transactions", ("commands", "transactions", "transaction_tests.rs")),
    ("Domain Model", ("domain/tab", "domain\\tab", "domain/panes", "tab_tests.rs", "tab_manager_tests.rs")),
    ("Buffer and Text Storage", ("domain/buffer", "buffer_tests.rs", "piece_tree_tests.rs")),
    ("Services and Persistence", ("services", "file_service_tests.rs", "file_controller_tests.rs", "session_store_tests.rs")),
    ("Search", ("search", "search_tests.rs")),
    ("UI and Editor Interaction", ("ui", "editor", "tab_strip", "tab_drag")),
    ("Startup and Settings", ("startup", "settings", "startup_tests.rs", "settings_store_tests.rs")),
]

FILE_DESCRIPTIONS = {
    "app_tests.rs": "Checks app state behavior.",
    "buffer_tests.rs": "Validates buffer metadata.",
    "file_controller_tests.rs": "Covers open/save controller flows.",
    "file_service_tests.rs": "Checks file service IO.",
    "piece_tree_tests.rs": "Validates piece-tree editing behavior.",
    "search_tests.rs": "Verifies search scopes and matches.",
    "session_store_tests.rs": "Checks saved workspace restoration.",
    "settings_store_tests.rs": "Checks settings persistence.",
    "startup_tests.rs": "Validates startup argument parsing.",
    "tab_manager_tests.rs": "Checks shared tab ordering.",
    "tab_tests.rs": "Validates workspace tab behavior.",
    "transaction_tests.rs": "Checks transaction grouping behavior.",
}


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
    normalized = path.as_posix()
    for layer, needles in LAYER_RULES:
        if any(needle.replace("\\", "/") in normalized for needle in needles):
            return layer
    return "Domain Model" if normalized.startswith("tests/") else "App Shell and State"


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
    if path.name in FILE_DESCRIPTIONS:
        return FILE_DESCRIPTIONS[path.name]
    return title_from_name(name)


def iter_rust_files(paths: Iterable[Path]) -> Iterable[Path]:
    seen = set()
    for root in paths:
        if not root.exists():
            continue
        if root.is_file() and root.suffix == ".rs":
            normalized = root.as_posix()
            if normalized not in seen:
                seen.add(normalized)
                yield root
        else:
            for path in root.rglob("*.rs"):
                normalized = path.as_posix()
                if normalized not in seen:
                    seen.add(normalized)
                    yield path


def cargo_manifest_rust_targets() -> List[Path]:
    if not MANIFEST_PATH.exists():
        return []
    try:
        text = MANIFEST_PATH.read_text(encoding="utf-8")
    except OSError:
        return []
    targets: List[Path] = []
    for block in re.split(r"(?m)^\s*\[\[", text):
        if not block.startswith(("bin]]", "test]]")):
            continue
        path_match = re.search(r'(?m)^\s*path\s*=\s*"([^"]+\.rs)"', block)
        if path_match:
            targets.append(Path(path_match.group(1)))
    targets.extend(Path("src/bin").glob("*.rs"))
    return targets


def discover_tests() -> List[Dict[str, Any]]:
    overrides = load_descriptions()
    tests: List[Dict[str, Any]] = []
    for path in sorted(iter_rust_files([Path("tests"), Path("src"), *cargo_manifest_rust_targets()])):
        try:
            lines = path.read_text(encoding="utf-8").splitlines()
        except OSError:
            continue
        kind = "integration" if path.as_posix().startswith("tests/") else "inline"
        pending_test_line: Optional[int] = None
        for line_index, raw_line in enumerate(lines, start=1):
            stripped = raw_line.strip()
            if re.match(r"#\[(?:test|tokio::test|async_std::test)\]", stripped):
                pending_test_line = line_index
                continue
            if pending_test_line is None:
                continue
            if not stripped or stripped.startswith("//") or re.match(r"#\[[^\]]+\]", stripped):
                continue
            match = re.match(r"(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+([A-Za-z0-9_]+)", stripped)
            if not match:
                pending_test_line = None
                continue
            name = match.group(1)
            line = pending_test_line
            pending_test_line = None
            test_id = f"{path.as_posix()}::{name}"
            tests.append(
                {
                    "id": test_id,
                    "name": name,
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
    return tests


def run_tests() -> Dict[str, Dict[str, Any]]:
    started = time.perf_counter()
    try:
        result = subprocess.run(["cargo", "test"], capture_output=True, text=True)
    except FileNotFoundError:
        return {}
    duration = time.perf_counter() - started
    statuses: Dict[str, Dict[str, Any]] = {}
    for line in result.stdout.splitlines():
        match = re.match(r"test\s+(.+?)\s+\.\.\.\s+(ok|FAILED|ignored)", line.strip())
        if not match:
            continue
        full_name = match.group(1)
        name = full_name.split("::")[-1]
        status = {"ok": "passed", "FAILED": "failed", "ignored": "skipped"}[match.group(2)]
        statuses[name] = {"status": status, "duration": None}
    statuses["__run__"] = {
        "status": "passed" if result.returncode == 0 else "failed",
        "duration": duration,
        "stdout_tail": "\n".join(result.stdout.splitlines()[-40:]),
        "stderr_tail": "\n".join(result.stderr.splitlines()[-40:]),
    }
    return statuses


def build_payload(run: bool = False) -> Dict[str, Any]:
    tests = discover_tests()
    statuses = run_tests() if run else {}
    for item in tests:
        status = statuses.get(item["name"])
        if status:
            item["last_status"] = status["status"]
            item["last_duration"] = status["duration"]

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
        "layers": len(by_layer),
        "failed": sum(1 for item in tests if item["last_status"] == "failed"),
        "unknown": sum(1 for item in tests if item["last_status"] == "unknown"),
        "last_run": statuses.get("__run__"),
    }
    return {
        "version": 1,
        "generated_from": "scripts/test_catalog.py",
        "summary": summary,
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
    parser.add_argument("--output", type=Path, default=None)
    parser.add_argument("--run", action="store_true", help="Run cargo test and attach status")
    add_mode_argument(parser)
    args = parser.parse_args()
    payload = build_payload(run=args.run)
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
