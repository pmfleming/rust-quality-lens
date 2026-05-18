import argparse
import json
import platform
import re
import sys
from dataclasses import asdict, dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Dict, Iterable, List, Optional, Sequence, Tuple

from report_modes import add_mode_argument, emit_report

DEFAULT_OUTPUT = Path("rust_escape_hatches.json")
VISIBILITY_OUTPUT = Path("target/analysis/rust_escape_hatches.json")


PATTERNS = {
    "unsafe_block": (r"\bunsafe\s*\{", 10.0),
    "unsafe_fn": (r"\bunsafe\s+fn\b", 10.0),
    "unsafe_impl": (r"\bunsafe\s+impl\b", 10.0),
    "unsafe_trait": (r"\bunsafe\s+trait\b", 10.0),
    "extern_block": (r"\b(?:unsafe\s+)?extern\s*(?:\"[^\"]+\")?\s*\{", 8.0),
    "extern_fn": (r"\b(?:unsafe\s+)?extern\s*(?:\"[^\"]+\")?\s+fn\b", 7.0),
    "static_mut": (r"\bstatic\s+mut\b", 14.0),
    "union": (r"\bunion\s+[A-Za-z_][A-Za-z0-9_]*", 12.0),
    "raw_borrow": (r"&\s*raw\s+(?:const|mut)\b", 6.0),
    "asm_macro": (r"\b(?:asm|global_asm)!\s*\(", 14.0),
    "transmute": (r"\btransmute(?:_copy)?\s*(?:::<[^>]+>)?\s*\(", 12.0),
    "maybe_uninit": (r"\bMaybeUninit\b", 5.0),
    "deref_impl": (r"\bimpl\b(?:\s*<[^{};]*>)?\s+(?:(?:::)?(?:std|core)::ops::)?Deref\s+for\b", 4.0),
    "deref_mut_impl": (r"\bimpl\b(?:\s*<[^{};]*>)?\s+(?:(?:::)?(?:std|core)::ops::)?DerefMut\s+for\b", 5.0),
    "glob_import": (r"::\s*\*", 2.0),
    "container_ref_return": (r"->\s*&\s*(?!mut\b)(?:'[A-Za-z_][A-Za-z0-9_]*\s+)?(?:(?:::)?[A-Za-z_][A-Za-z0-9_]*::)*(?:Vec|HashMap|BTreeMap|HashSet|BTreeSet|Option|Box|Rc|Arc|String)\s*(?:<|\b)", 3.0),
    "repr_escape": (r"#\s*\[\s*repr\s*\(\s*(?:C|packed|transparent|align)", 5.0),
    "linkage_escape": (r"#\s*\[\s*(?:no_mangle|export_name|link_name|link_section|used)\b", 8.0),
    "clippy_suppression": (r"#\s*!\s*\[\s*(?:allow|expect)\s*\([^)]*clippy::|#\s*\[\s*(?:allow|expect)\s*\([^)]*clippy::", 3.0),
    "lint_suppression": (r"#\s*!\s*\[\s*(?:allow|expect)\s*\(|#\s*\[\s*(?:allow|expect)\s*\(", 2.0),
}

ALLOW_ATTRIBUTE_PATTERN = re.compile(
    r"#\s*!?\s*\[\s*(?:allow|expect)\s*\(",
    re.MULTILINE,
)
CLIPPY_ALLOW_PATTERN = re.compile(
    r"#\s*!?\s*\[\s*(?:allow|expect)\s*\([^)]*clippy::",
    re.MULTILINE,
)

SIGNAL_LABELS = {
    "unsafe_block": "unsafe block",
    "unsafe_fn": "unsafe fn",
    "unsafe_impl": "unsafe impl",
    "unsafe_trait": "unsafe trait",
    "extern_block": "extern block",
    "extern_fn": "extern fn",
    "static_mut": "mutable static",
    "union": "union",
    "raw_borrow": "raw borrow",
    "asm_macro": "inline assembly",
    "transmute": "transmute",
    "maybe_uninit": "MaybeUninit",
    "deref_impl": "Deref impl",
    "deref_mut_impl": "DerefMut impl",
    "glob_import": "glob import",
    "container_ref_return": "container ref return",
    "repr_escape": "layout repr",
    "linkage_escape": "linkage attribute",
    "clippy_suppression": "Clippy suppression",
    "lint_suppression": "lint suppression",
}


@dataclass
class EscapeHatchRecord:
    module_name: str
    module_key: str
    path: str
    escape_hatch_score: float
    total_count: int
    unsafe_count: int
    ffi_count: int
    global_mutability_count: int
    raw_memory_count: int
    deref_coercion_count: int
    glob_import_count: int
    container_ref_return_count: int
    layout_linkage_count: int
    clippy_suppression_count: int
    lint_suppression_count: int
    allow_attribute_count: int
    clippy_allow_count: int
    counts: Dict[str, int]
    locations: List[Dict[str, object]]
    allow_locations: List[Dict[str, object]]
    signals: List[str]
    measured_at: str
    command: str
    host: str
    source: str = "static_rust_escape_hatches"
    mock: bool = False


class RustEscapeHatchAnalyzer:
    def __init__(self, top: Optional[int] = None):
        self.top = top

    def run(self, paths: Sequence[str]) -> List[Dict]:
        rows = [asdict(self._record_for_file(path)) for path in self._iter_rust_files(paths)]
        rows = [row for row in rows if row["total_count"] > 0]
        rows.sort(
            key=lambda item: (
                -float(item["escape_hatch_score"]),
                -int(item["total_count"]),
                item["module_key"],
            )
        )
        if self.top is not None:
            return rows[: self.top]
        return rows

    def _iter_rust_files(self, paths: Sequence[str]) -> Iterable[Path]:
        seen = set()
        for raw_path in paths:
            path = Path(raw_path)
            candidates: Iterable[Path]
            if path.is_file() and path.suffix == ".rs":
                candidates = [path]
            elif path.is_dir():
                candidates = path.rglob("*.rs")
            else:
                candidates = []
            for candidate in candidates:
                resolved = candidate.resolve()
                if resolved not in seen:
                    seen.add(resolved)
                    yield candidate

    def _record_for_file(self, path: Path) -> EscapeHatchRecord:
        source = path.read_text(encoding="utf-8")
        searchable = strip_comments_and_strings(source)
        counts: Dict[str, int] = {}
        locations: List[Dict[str, object]] = []
        score = 0.0

        for key, (pattern, weight) in PATTERNS.items():
            matches = list(re.finditer(pattern, searchable, re.MULTILINE))
            if not matches:
                counts[key] = 0
                continue
            counts[key] = len(matches)
            score += len(matches) * weight
            for match in matches[:20]:
                locations.append(
                    {
                        "kind": key,
                        "label": SIGNAL_LABELS[key],
                        "line": searchable.count("\n", 0, match.start()) + 1,
                    }
                )

        unsafe_count = sum(
            counts[key]
            for key in ["unsafe_block", "unsafe_fn", "unsafe_impl", "unsafe_trait"]
        )
        ffi_count = counts["extern_block"] + counts["extern_fn"]
        global_mutability_count = counts["static_mut"]
        raw_memory_count = (
            counts["union"]
            + counts["raw_borrow"]
            + counts["asm_macro"]
            + counts["transmute"]
            + counts["maybe_uninit"]
        )
        deref_coercion_count = counts["deref_impl"] + counts["deref_mut_impl"]
        glob_import_count = counts["glob_import"]
        container_ref_return_count = counts["container_ref_return"]
        layout_linkage_count = counts["repr_escape"] + counts["linkage_escape"]
        clippy_suppression_count = counts["clippy_suppression"]
        lint_suppression_count = counts["lint_suppression"]
        allow_matches = list(ALLOW_ATTRIBUTE_PATTERN.finditer(searchable))
        allow_attribute_count = len(allow_matches)
        clippy_allow_count = len(list(CLIPPY_ALLOW_PATTERN.finditer(searchable)))
        source_lines = source.splitlines()
        allow_locations = []
        for match in allow_matches:
            line = searchable.count("\n", 0, match.start()) + 1
            snippet = source_lines[line - 1].strip() if line - 1 < len(source_lines) else ""
            allow_locations.append(
                {
                    "kind": "allow_attribute",
                    "label": "allow/expect attribute",
                    "line": line,
                    "snippet": snippet,
                }
            )
        signals = [
            f"{SIGNAL_LABELS[key]} {count}"
            for key, count in counts.items()
            if count > 0
        ]
        if allow_attribute_count:
            signals.append(f"allow/expect attributes {allow_attribute_count}")
        if not signals:
            signals = ["stable"]

        return EscapeHatchRecord(
            module_name=module_key_for_path(path),
            module_key=module_key_for_path(path),
            path=path.as_posix(),
            escape_hatch_score=round(score, 2),
            total_count=sum(counts.values()),
            unsafe_count=unsafe_count,
            ffi_count=ffi_count,
            global_mutability_count=global_mutability_count,
            raw_memory_count=raw_memory_count,
            deref_coercion_count=deref_coercion_count,
            glob_import_count=glob_import_count,
            container_ref_return_count=container_ref_return_count,
            layout_linkage_count=layout_linkage_count,
            clippy_suppression_count=clippy_suppression_count,
            lint_suppression_count=lint_suppression_count,
            allow_attribute_count=allow_attribute_count,
            clippy_allow_count=clippy_allow_count,
            counts=counts,
            locations=sorted(locations, key=lambda item: (int(item["line"]), str(item["kind"]))),
            allow_locations=allow_locations,
            signals=signals,
            measured_at=datetime.now(timezone.utc).isoformat(),
            command=" ".join(sys.argv),
            host=platform.node(),
        )


def mask_char(result: List[str], ch: str) -> None:
    result.append("\n" if ch == "\n" else " ")


def consume_code(source: str, index: int, result: List[str]) -> Tuple[int, str, int]:
    ch = source[index]
    nxt = source[index + 1] if index + 1 < len(source) else ""
    if ch == "/" and nxt == "/":
        result.extend("  ")
        return index + 2, "line_comment", 0
    if ch == "/" and nxt == "*":
        result.extend("  ")
        return index + 2, "block_comment", 0

    raw_match = re.match(r"r(#+)\"", source[index:])
    if raw_match:
        raw_hashes = len(raw_match.group(1))
        result.extend(" " * (raw_hashes + 2))
        return index + raw_hashes + 2, "raw_string", raw_hashes
    if ch == '"':
        result.append(" ")
        return index + 1, "string", 0
    if ch == "'":
        result.append(" ")
        return index + 1, "char", 0

    result.append(ch)
    return index + 1, "code", 0


def consume_line_comment(source: str, index: int, result: List[str]) -> Tuple[int, str, int]:
    ch = source[index]
    if ch == "\n":
        result.append("\n")
        return index + 1, "code", 0
    result.append(" ")
    return index + 1, "line_comment", 0


def consume_block_comment(source: str, index: int, result: List[str]) -> Tuple[int, str, int]:
    ch = source[index]
    nxt = source[index + 1] if index + 1 < len(source) else ""
    if ch == "*" and nxt == "/":
        result.extend("  ")
        return index + 2, "code", 0
    mask_char(result, ch)
    return index + 1, "block_comment", 0


def consume_quoted(source: str, index: int, result: List[str], quote: str) -> Tuple[int, str, int]:
    ch = source[index]
    if ch == "\\":
        result.extend("  ")
        return index + 2, quote, 0
    mask_char(result, ch)
    terminator = '"' if quote == "string" else "'"
    return index + 1, "code" if ch == terminator else quote, 0


def consume_raw_string(
    source: str,
    index: int,
    result: List[str],
    raw_hashes: int,
) -> Tuple[int, str, int]:
    terminator = '"' + ("#" * raw_hashes)
    if source.startswith(terminator, index):
        result.extend(" " * len(terminator))
        return index + len(terminator), "code", 0
    mask_char(result, source[index])
    return index + 1, "raw_string", raw_hashes


def strip_comments_and_strings(source: str) -> str:
    result: List[str] = []
    index = 0
    state = "code"
    raw_hashes = 0
    while index < len(source):
        if state == "code":
            index, state, raw_hashes = consume_code(source, index, result)
        elif state == "line_comment":
            index, state, raw_hashes = consume_line_comment(source, index, result)
        elif state == "block_comment":
            index, state, raw_hashes = consume_block_comment(source, index, result)
        elif state in {"string", "char"}:
            index, state, raw_hashes = consume_quoted(source, index, result, state)
        else:
            index, state, raw_hashes = consume_raw_string(
                source,
                index,
                result,
                raw_hashes,
            )
    return "".join(result)


def module_key_for_path(path: Path) -> str:
    try:
        rel_path = path.relative_to("src")
    except ValueError:
        rel_path = path
    module = rel_path.as_posix().replace("/", "::").removesuffix(".rs")
    if module.endswith("::mod"):
        module = module[:-5]
    return module


def render_cli(payload: object) -> str:
    rows = payload if isinstance(payload, list) else []
    lines = ["Rust Escape Hatches"]
    if not rows:
        lines.append("No escape hatch usage found.")
        return "\n".join(lines)

    for index, item in enumerate(rows[:10], start=1):
        lines.append(
            f"{index:>2}. {item['module_key']} | score={item['escape_hatch_score']:.1f} | total={item['total_count']} | unsafe={item['unsafe_count']} | raw={item['raw_memory_count']} | deref={item['deref_coercion_count']} | glob={item['glob_import_count']} | container_refs={item['container_ref_return_count']} | ffi={item['ffi_count']} | allow_expect={item.get('allow_attribute_count', 0)}"
        )
    if len(rows) > 10:
        lines.append(f"... and {len(rows) - 10} more modules.")
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Emit Rust escape hatch usage as JSON"
    )
    parser.add_argument("--paths", nargs="+", default=["src"], help="Paths to analyze")
    parser.add_argument(
        "--top",
        type=int,
        default=None,
        help="Limit ranked records. Defaults to all files with usage.",
    )
    parser.add_argument("--output", type=Path, default=None)
    add_mode_argument(parser)
    args = parser.parse_args()

    payload = RustEscapeHatchAnalyzer(top=args.top).run(args.paths)
    emit_report(
        payload,
        mode=args.mode,
        output_path=args.output,
        visibility_path=VISIBILITY_OUTPUT,
        cli_renderer=render_cli,
        label="rust escape hatches",
    )


if __name__ == "__main__":
    main()
