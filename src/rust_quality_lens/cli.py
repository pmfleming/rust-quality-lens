from __future__ import annotations

import argparse
import os
from pathlib import Path
import runpy
import sys
from typing import Sequence

from .config import LensConfig


TOOLS_DIR = Path(__file__).resolve().parent / "tools"
TOOL_ENV_KEYS = (
    "RQLENS_OUTPUT_DIR",
    "RQLENS_HELPER_MANIFEST",
    "RQLENS_PROJECT_NAME",
    "RQLENS_SOURCE_ROOTS",
)
PATH_AWARE_TOOLS = {
    "hotspots",
    "clones",
    "escape-hatches",
    "type-health",
    "locality",
    "leverage",
}

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

TASK_DEFINITIONS = [
    {
        "tool": "hotspots",
        "id": "quality.hotspots",
        "category": "quality",
        "subcategory": "hotspots",
        "title": "Hotspots",
        "description": "Ranks complexity risk without SLOC-only scoring.",
    },
    {
        "tool": "clones",
        "id": "quality.clones",
        "category": "quality",
        "subcategory": "clones",
        "title": "Clones",
        "description": "Finds repeated token and AST-like code structures.",
    },
    {
        "tool": "escape-hatches",
        "id": "quality.escape_hatches",
        "aliases": ["quality.escape-hatches"],
        "category": "quality",
        "subcategory": "safety",
        "title": "Rust Escape Hatches",
        "description": "Tracks unsafe, FFI, raw memory, globals, glob imports, and lint suppressions.",
    },
    {
        "tool": "type-health",
        "id": "quality.type_health",
        "aliases": ["quality.type-health"],
        "category": "quality",
        "subcategory": "structure",
        "title": "Type Health",
        "description": "Ranks wide structs, large enums, broad method surfaces, and impl spread.",
    },
    {
        "tool": "locality",
        "id": "quality.locality_dynamic",
        "aliases": ["quality.locality"],
        "category": "quality",
        "subcategory": "locality",
        "title": "Code Locality",
        "description": "Measures dependency spread, hidden coupling, interface explicitness, and change locality.",
    },
    {
        "tool": "leverage",
        "id": "quality.locality_leverage",
        "aliases": ["quality.leverage"],
        "category": "quality",
        "subcategory": "leverage",
        "title": "Architecture Leverage",
        "description": "Measures reach, invariant surface, divergence pressure, and co-change ripple.",
    },
    {
        "tool": "correctness",
        "id": "correctness.catalog",
        "category": "correctness",
        "subcategory": "tests",
        "title": "Correctness Catalog",
        "description": "Discovers Rust tests and groups them by architecture layer.",
        "outputs": ["correctness_review.json", "test_catalog.json"],
    },
    {
        "tool": "correctness-run",
        "id": "correctness.all",
        "category": "correctness",
        "subcategory": "tests",
        "title": "All Tests",
        "description": "Runs the full Rust test suite and attaches status to the correctness catalog.",
        "expensive": True,
    },
    {
        "tool": "map",
        "id": "map.architecture",
        "category": "map",
        "subcategory": "architecture",
        "title": "Architecture Map",
        "description": "Builds module health, dependency, and risk map data.",
        "depends_on": [
            "quality.hotspots",
            "correctness.catalog",
            "quality.locality_dynamic",
            "quality.locality_leverage",
        ],
    },
]


def run_tool(module_name: str, argv: Sequence[str], config: LensConfig) -> None:
    old_argv = sys.argv[:]
    old_cwd = Path.cwd()
    old_path = sys.path[:]
    old_env = {key: os.environ.get(key) for key in TOOL_ENV_KEYS}
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
        run_tool(module_name, tool_argv(tool, config.output_dir / file_name, config), config)


def tool_argv(tool: str, output_path: Path, config: LensConfig) -> list[str]:
    argv = ["--mode", "visibility", "--output", str(output_path)]
    if tool in PATH_AWARE_TOOLS:
        argv.extend(["--paths", *config.source_roots])
    if tool == "hotspots":
        argv.extend(["--scope", "all"])
    if tool == "correctness-run":
        argv.append("--run")
    return argv


def catalog(args: argparse.Namespace) -> None:
    config = LensConfig.load(args.config)
    tasks = [catalog_task(definition, config) for definition in TASK_DEFINITIONS]
    payload = {
        "version": 1,
        "project_name": config.project_name,
        "analysis_root": str(config.output_dir),
        "categories": [
            {"id": "quality", "title": "Quality Review"},
            {"id": "correctness", "title": "Correctness Review"},
            {"id": "map", "title": "Map"},
        ],
        "tasks": tasks,
    }
    import json

    print(json.dumps(payload, indent=2))


def catalog_task(definition: dict[str, object], config: LensConfig) -> dict[str, object]:
    tool = str(definition["tool"])
    _, default_file_name = MEASURE_TOOLS[tool]
    output_files = [str(item) for item in definition.get("outputs", [default_file_name])]
    output_artifacts = [f"target/analysis/{file_name}" for file_name in output_files]
    task = {
        "id": definition["id"],
        "category": definition["category"],
        "subcategory": definition["subcategory"],
        "title": definition["title"],
        "description": definition["description"],
        "commands": [["rqlens", "measure", tool]],
        "output_artifacts": output_artifacts,
        "absolute_output_artifacts": [str(config.output_dir / file_name) for file_name in output_files],
        "depends_on": definition.get("depends_on", []),
        "expensive": bool(definition.get("expensive", False)),
        "supports_individual_run": True,
        "lens": "rust-quality-lens",
        "tool": tool,
    }
    if "aliases" in definition:
        task["aliases"] = definition["aliases"]
    return task


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
