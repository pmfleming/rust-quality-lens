from __future__ import annotations

from pathlib import Path
from typing import Iterable, Sequence


RULESET_ID = "rqlens.generic_layers"
RULESET_VERSION = 1
DEFAULT_LAYER = "Unclassified"

LAYER_RULES: tuple[tuple[str, tuple[str, ...]], ...] = (
    (
        "Interface",
        (
            "api",
            "cli",
            "controller",
            "controllers",
            "handler",
            "handlers",
            "http",
            "route",
            "routes",
            "ui",
            "web",
        ),
    ),
    (
        "Application",
        (
            "app",
            "application",
            "command",
            "commands",
            "service",
            "services",
            "use_case",
            "use_cases",
            "workflow",
            "workflows",
        ),
    ),
    (
        "Domain",
        (
            "core",
            "domain",
            "entity",
            "entities",
            "model",
            "models",
        ),
    ),
    (
        "Infrastructure",
        (
            "adapter",
            "adapters",
            "database",
            "db",
            "file",
            "fs",
            "infra",
            "infrastructure",
            "io",
            "persistence",
            "repository",
            "storage",
        ),
    ),
    (
        "Tests",
        (
            "spec",
            "specs",
            "test",
            "tests",
        ),
    ),
)

LAYER_COLORS = {
    "interface": "#569cd6",
    "application": "#d7ba7d",
    "domain": "#4ec9b0",
    "infrastructure": "#c586c0",
    "tests": "#9cdcfe",
    "unclassified": "#808080",
    "default": "#808080",
}

LAYER_ORDER = {
    "interface": 0,
    "application": 1,
    "domain": 2,
    "infrastructure": 3,
    "tests": 4,
    "unclassified": 1,
    "default": 1,
}


def layer_key(layer_name: str) -> str:
    return layer_name.lower().replace(" ", "_")


def classify_path(path: Path | str) -> str:
    normalized = _segments(path)
    for layer_name, needles in LAYER_RULES:
        if _matches_any(normalized, needles):
            return layer_name
    return DEFAULT_LAYER


def classify_module(module_key: str) -> str:
    return classify_path(module_key.replace("::", "/"))


def layer_rank(layer_name: str) -> int:
    return LAYER_ORDER.get(layer_key(layer_name), LAYER_ORDER["default"])


def layer_color(layer_name: str) -> str:
    return LAYER_COLORS.get(layer_key(layer_name), LAYER_COLORS["default"])


def _segments(path: Path | str) -> tuple[str, ...]:
    value = path.as_posix() if isinstance(path, Path) else str(path).replace("\\", "/")
    return tuple(segment.lower() for segment in value.split("/") if segment)


def _matches_any(segments: Sequence[str], needles: Iterable[str]) -> bool:
    for needle in needles:
        normalized = needle.lower().replace("\\", "/")
        needle_segments = tuple(segment for segment in normalized.split("/") if segment)
        if len(needle_segments) == 1:
            if needle_segments[0] in segments:
                return True
            continue
        if _contains_subsequence(segments, needle_segments):
            return True
    return False


def _contains_subsequence(segments: Sequence[str], needle: Sequence[str]) -> bool:
    if len(needle) > len(segments):
        return False
    limit = len(segments) - len(needle) + 1
    for index in range(limit):
        if tuple(segments[index : index + len(needle)]) == tuple(needle):
            return True
    return False
