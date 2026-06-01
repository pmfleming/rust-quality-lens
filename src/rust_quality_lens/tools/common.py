from __future__ import annotations

import json
import os
import platform
import re
import subprocess
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Dict, Iterable, List, Optional, Sequence


def iter_rust_files(paths: Sequence[str | Path]) -> Iterable[Path]:
    seen: set[Path] = set()
    for raw_path in paths:
        path = Path(raw_path)
        if path.is_file() and path.suffix == ".rs":
            candidates: Iterable[Path] = [path]
        elif path.is_dir():
            candidates = path.rglob("*.rs")
        else:
            candidates = []

        for candidate in candidates:
            resolved = candidate.resolve()
            if resolved in seen:
                continue
            seen.add(resolved)
            yield candidate


def module_key_for_path(path: Path, source_root: str | None = None) -> str:
    source_roots = [source_root] if source_root else source_roots_from_env()
    rel_path = path
    for root in source_roots:
        try:
            rel_path = path.relative_to(root)
            break
        except ValueError:
            continue

    parts = list(rel_path.with_suffix("").parts)
    if parts and parts[-1] == "mod":
        parts = parts[:-1]
    return "::".join(parts)


def source_roots_from_env(default: str = "src") -> List[str]:
    return [
        root
        for root in os.environ.get("RQLENS_SOURCE_ROOTS", default).split(os.pathsep)
        if root
    ]


def provenance() -> Dict[str, str]:
    return {
        "measured_at": datetime.now(timezone.utc).isoformat(),
        "command": " ".join(sys.argv),
        "host": platform.node(),
    }


def measurement_confidence(
    *,
    missing_input: Sequence[str] | None = None,
    stale_input: Sequence[str] | None = None,
    unsupported_pattern: Sequence[str] | None = None,
) -> Dict[str, object]:
    missing = list(missing_input or [])
    stale = list(stale_input or [])
    unsupported = list(unsupported_pattern or [])
    complete = not missing and not stale and not unsupported
    return {
        "complete": complete,
        "partial": not complete,
        "missing_input": missing,
        "stale_input": stale,
        "unsupported_pattern": unsupported,
    }


def source_confidence(
    paths: Sequence[str | Path],
    *,
    facts: Sequence[Dict[str, object]] | None = None,
) -> Dict[str, object]:
    files = [str(path) for path in iter_rust_files(paths)]
    missing_input = [] if files else ["no Rust source files matched the configured paths"]
    unsupported = []
    if facts is not None:
        fact_paths = {str(fact.get("path", "")) for fact in facts if isinstance(fact, dict)}
        if files and not fact_paths:
            missing_input.append("Rust syntax fact extraction returned no files")
        for fact in facts:
            if not isinstance(fact, dict):
                continue
            status = str(fact.get("parse_status", "ok"))
            if status != "ok":
                unsupported.append(f"{fact.get('path', '<unknown>')}: {status}")
    return measurement_confidence(
        missing_input=missing_input,
        unsupported_pattern=unsupported,
    )


def confidence_for_artifacts(
    artifacts: Sequence[str | Path],
    *,
    source_paths: Sequence[str | Path],
    required: bool = True,
) -> Dict[str, object]:
    source_files = [Path(path) for path in iter_rust_files(source_paths)]
    newest_source = max((path.stat().st_mtime for path in source_files if path.exists()), default=None)
    missing_input: List[str] = []
    stale_input: List[str] = []
    for artifact in artifacts:
        path = Path(artifact)
        if not path.exists():
            if required:
                missing_input.append(str(path))
            continue
        if newest_source is not None and path.stat().st_mtime < newest_source:
            stale_input.append(str(path))
    return measurement_confidence(missing_input=missing_input, stale_input=stale_input)


def merge_confidences(*items: Dict[str, object]) -> Dict[str, object]:
    missing: List[str] = []
    stale: List[str] = []
    unsupported: List[str] = []
    for item in items:
        missing.extend(str(value) for value in item.get("missing_input", []))
        stale.extend(str(value) for value in item.get("stale_input", []))
        unsupported.extend(str(value) for value in item.get("unsupported_pattern", []))
    return measurement_confidence(
        missing_input=_dedupe(missing),
        stale_input=_dedupe(stale),
        unsupported_pattern=_dedupe(unsupported),
    )


def _dedupe(values: Sequence[str]) -> List[str]:
    return list(dict.fromkeys(values))


def matching_brace(source: str, open_index: int) -> Optional[int]:
    if open_index < 0 or open_index >= len(source) or source[open_index] != "{":
        return None

    depth = 0
    for index in range(open_index, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return index
    return None


def mask_char(result: List[str], ch: str) -> None:
    result.append("\n" if ch == "\n" else " ")


def strip_comments_and_strings(source: str) -> str:
    result: List[str] = []
    index = 0
    state = "code"
    raw_hashes = 0

    while index < len(source):
        if state == "code":
            index, state, raw_hashes = _consume_code(source, index, result)
        elif state == "line_comment":
            index, state, raw_hashes = _consume_line_comment(source, index, result)
        elif state == "block_comment":
            index, state, raw_hashes = _consume_block_comment(source, index, result)
        elif state in {"string", "char"}:
            index, state, raw_hashes = _consume_quoted(source, index, result, state)
        else:
            index, state, raw_hashes = _consume_raw_string(
                source,
                index,
                result,
                raw_hashes,
            )

    return "".join(result)


def _consume_code(source: str, index: int, result: List[str]) -> tuple[int, str, int]:
    ch = source[index]
    nxt = source[index + 1] if index + 1 < len(source) else ""
    raw_match = re.match(r"r(#+)?\"", source[index:])

    if ch == "/" and nxt == "/":
        result.extend("  ")
        return index + 2, "line_comment", 0
    if ch == "/" and nxt == "*":
        result.extend("  ")
        return index + 2, "block_comment", 0
    if raw_match:
        raw_hashes = len(raw_match.group(1) or "")
        result.extend(" " * (raw_hashes + 2))
        return index + raw_hashes + 2, "raw_string", raw_hashes
    if ch == '"':
        result.append(" ")
        return index + 1, "string", 0
    if ch == "'" and re.match(r"'(?:\\.|[^'\\\n])'", source[index:]):
        result.append(" ")
        return index + 1, "char", 0

    result.append(ch)
    return index + 1, "code", 0


def _consume_line_comment(source: str, index: int, result: List[str]) -> tuple[int, str, int]:
    ch = source[index]
    mask_char(result, ch)
    return index + 1, "code" if ch == "\n" else "line_comment", 0


def _consume_block_comment(source: str, index: int, result: List[str]) -> tuple[int, str, int]:
    ch = source[index]
    nxt = source[index + 1] if index + 1 < len(source) else ""
    if ch == "*" and nxt == "/":
        result.extend("  ")
        return index + 2, "code", 0

    mask_char(result, ch)
    return index + 1, "block_comment", 0


def _consume_quoted(
    source: str,
    index: int,
    result: List[str],
    quote: str,
) -> tuple[int, str, int]:
    ch = source[index]
    if ch == "\\":
        result.extend("  ")
        return index + 2, quote, 0

    mask_char(result, ch)
    terminator = '"' if quote == "string" else "'"
    return index + 1, "code" if ch == terminator else quote, 0


def _consume_raw_string(
    source: str,
    index: int,
    result: List[str],
    raw_hashes: int,
) -> tuple[int, str, int]:
    terminator = '"' + ("#" * raw_hashes)
    if source.startswith(terminator, index):
        result.extend(" " * len(terminator))
        return index + len(terminator), "code", 0

    mask_char(result, source[index])
    return index + 1, "raw_string", raw_hashes


def run_helper_json(binary: str, files: Sequence[str], warning_label: str) -> List[Dict[str, object]]:
    if not files:
        return []

    temp_path = _write_lines_temp(files)
    cmd = ["cargo", "run", "--quiet"]
    helper_manifest = os.environ.get("RQLENS_HELPER_MANIFEST")
    if helper_manifest:
        cmd.extend(["--manifest-path", helper_manifest])
    cmd.extend(["--bin", binary, "--", temp_path])

    try:
        result = subprocess.run(cmd, capture_output=True, text=True, check=True)
    except subprocess.CalledProcessError as exc:
        print(
            f"Warning: {warning_label} failed with exit {exc.returncode}: {' '.join(cmd)}",
            file=sys.stderr,
        )
        _print_process_tail("stderr", exc.stderr)
        _print_process_tail("stdout", exc.stdout)
        return []
    except Exception as exc:
        print(f"Warning: {warning_label} failed: {exc}", file=sys.stderr)
        return []
    finally:
        try:
            os.remove(temp_path)
        except OSError:
            pass

    try:
        records = json.loads(result.stdout)
    except json.JSONDecodeError as exc:
        print(f"Warning: {warning_label} output was malformed: {exc}", file=sys.stderr)
        return []
    return records if isinstance(records, list) else []


def rust_facts_for_paths(paths: Sequence[str | Path]) -> List[Dict[str, object]]:
    files = [str(path) for path in iter_rust_files(paths)]
    return run_helper_json("rust_facts", files, "Rust syntax fact extraction")


def _print_process_tail(label: str, text: str | None, *, limit: int = 4000) -> None:
    if not text:
        return
    trimmed = text.strip()
    if len(trimmed) > limit:
        trimmed = f"...{trimmed[-limit:]}"
    print(f"{label}:\n{trimmed}", file=sys.stderr)


def _write_lines_temp(lines: Sequence[str]) -> str:
    with tempfile.NamedTemporaryFile(mode="w", delete=False, encoding="utf-8") as handle:
        for line in lines:
            handle.write(f"{line}\n")
        return handle.name
