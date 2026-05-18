from __future__ import annotations

import argparse
import os
from pathlib import Path
import runpy
import sys
from typing import Sequence

from .config import LensConfig


TOOLS_DIR = Path(__file__).resolve().parent / "tools"

QUALITY_TOOLS = {
    "hotspots": ("hotspots", "hotspots.json"),
    "clones": ("clone_alert", "clones.json"),
    "escape-hatches": ("rust_escape_hatches", "rust_escape_hatches.json"),
    "type-health": ("type_health", "type_health.json"),
    "locality": ("locality_bench", "locality_metrics.json"),
    "leverage": ("leverage_metrics", "leverage_metrics.json"),
}

MEASURE_TOOLS = {
    **QUALITY_TOOLS,
    "correctness": ("test_catalog", "correctness_review.json"),
    "correctness-run": ("test_catalog", "correctness_review.json"),
    "map": ("map", "map.json"),
}


def run_tool(module_name: str, argv: Sequence[str], config: LensConfig) -> None:
    old_argv = sys.argv[:]
    old_cwd = Path.cwd()
    old_path = sys.path[:]
    old_env = {
        "RQLENS_OUTPUT_DIR": os.environ.get("RQLENS_OUTPUT_DIR"),
        "RQLENS_HELPER_MANIFEST": os.environ.get("RQLENS_HELPER_MANIFEST"),
        "RQLENS_PROJECT_NAME": os.environ.get("RQLENS_PROJECT_NAME"),
        "RQLENS_SOURCE_ROOTS": os.environ.get("RQLENS_SOURCE_ROOTS"),
    }
    try:
        os.chdir(config.project_root)
        sys.path.insert(0, str(TOOLS_DIR))
        os.environ["RQLENS_OUTPUT_DIR"] = str(config.output_dir)
        os.environ["RQLENS_HELPER_MANIFEST"] = str(config.helper_manifest)
        os.environ["RQLENS_PROJECT_NAME"] = config.project_name
        os.environ["RQLENS_SOURCE_ROOTS"] = os.pathsep.join(config.source_roots)
        sys.argv = [module_name, *argv]
        runpy.run_module(module_name, run_name="__main__")
    finally:
        sys.argv = old_argv
        sys.path[:] = old_path
        os.chdir(old_cwd)
        for key, value in old_env.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value


def measure(args: argparse.Namespace) -> None:
    config = LensConfig.load(args.config)
    selected = list(MEASURE_TOOLS) if args.tool == "all" else [args.tool]
    config.output_dir.mkdir(parents=True, exist_ok=True)

    for tool in selected:
        module_name, file_name = MEASURE_TOOLS[tool]
        output_path = config.output_dir / file_name
        argv = ["--mode", "visibility", "--output", str(output_path)]
        if tool in {"hotspots", "clones", "escape-hatches", "type-health", "locality", "leverage"}:
            argv.extend(["--paths", *config.source_roots])
        if tool == "hotspots":
            argv.extend(["--scope", "all"])
        if tool == "correctness-run":
            argv.append("--run")
        run_tool(module_name, argv, config)


def catalog(args: argparse.Namespace) -> None:
    config = LensConfig.load(args.config)
    tasks = []
    for tool, (_, file_name) in QUALITY_TOOLS.items():
        tasks.append(
            {
                "id": f"quality.{tool}",
                "category": "quality",
                "title": tool.replace("-", " ").title(),
                "output_artifacts": [str(config.output_dir / file_name)],
            }
        )
    tasks.extend(
        [
            {
                "id": "correctness.catalog",
                "category": "correctness",
                "title": "Correctness Catalog",
                "output_artifacts": [
                    str(config.output_dir / "correctness_review.json"),
                    str(config.output_dir / "test_catalog.json"),
                ],
            },
            {
                "id": "correctness.all",
                "category": "correctness",
                "title": "All Tests",
                "output_artifacts": [str(config.output_dir / "correctness_review.json")],
            },
            {
                "id": "map.architecture",
                "category": "map",
                "title": "Architecture Map",
                "output_artifacts": [str(config.output_dir / "map.json")],
            },
        ]
    )
    payload = {"version": 1, "project_name": config.project_name, "tasks": tasks}
    import json

    print(json.dumps(payload, indent=2))


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Reusable Rust measurement JSON producers")
    subcommands = parser.add_subparsers(dest="command", required=True)

    measure_parser = subcommands.add_parser("measure", help="Run quality measurements")
    measure_parser.add_argument(
        "tool",
        nargs="?",
        default="all",
        choices=["all", *MEASURE_TOOLS.keys()],
        help="Quality measurement to run",
    )
    measure_parser.add_argument("--config", type=Path, default=None)
    measure_parser.set_defaults(func=measure)

    catalog_parser = subcommands.add_parser("catalog", help="Print the quality task catalog")
    catalog_parser.add_argument("--config", type=Path, default=None)
    catalog_parser.set_defaults(func=catalog)

    return parser


def main() -> None:
    parser = build_parser()
    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
