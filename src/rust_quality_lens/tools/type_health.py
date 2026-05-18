import argparse
import platform
import re
import sys
from dataclasses import asdict, dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Dict, Iterable, List, Optional, Sequence, Set, Tuple

from report_modes import add_mode_argument, emit_report

DEFAULT_OUTPUT = Path("type_health.json")
VISIBILITY_OUTPUT = Path("target/analysis/type_health.json")


@dataclass
class TypeRecord:
    type_name: str
    qualified_name: str
    module_key: str
    path: str
    line: int
    kind: str
    field_count: int = 0
    variant_count: int = 0
    declaration_span: int = 0
    method_count: int = 0
    impl_block_count: int = 0
    impl_file_count: int = 0
    impl_files: List[str] = field(default_factory=list)
    structural_risk: float = 0.0
    structural_score: float = 100.0
    signals: List[str] = field(default_factory=list)
    measured_at: str = ""
    command: str = ""
    host: str = ""
    source: str = "static_type_health"
    mock: bool = False


@dataclass
class ImplStats:
    method_count: int = 0
    impl_block_count: int = 0
    impl_files: Set[str] = field(default_factory=set)


class TypeHealthAnalyzer:
    def __init__(self, top: Optional[int] = None) -> None:
        self.top = top

    def run(self, paths: Sequence[str]) -> List[Dict]:
        declarations: List[TypeRecord] = []
        impls: Dict[str, ImplStats] = {}

        for path in self._iter_rust_files(paths):
            source = path.read_text(encoding="utf-8")
            searchable = strip_comments_and_strings(source)
            module_key = module_key_for_path(path)
            rel_path = path.as_posix()

            for record in self._type_declarations(searchable, module_key, rel_path):
                declarations.append(record)

            for type_name, method_count in self._impl_blocks(searchable):
                stats = impls.setdefault(type_name, ImplStats())
                stats.impl_block_count += 1
                stats.method_count += method_count
                stats.impl_files.add(rel_path)

        rows: List[Dict] = []
        provenance = {
            "measured_at": datetime.now(timezone.utc).isoformat(),
            "command": " ".join(sys.argv),
            "host": platform.node(),
        }

        for record in declarations:
            stats = impls.get(record.type_name, ImplStats())
            record.method_count = stats.method_count
            record.impl_block_count = stats.impl_block_count
            record.impl_files = sorted(stats.impl_files)
            record.impl_file_count = len(record.impl_files)
            record.structural_risk, record.signals = self._risk(record)
            record.structural_score = round(max(0.0, 100.0 - record.structural_risk), 2)
            record.measured_at = provenance["measured_at"]
            record.command = provenance["command"]
            record.host = provenance["host"]
            rows.append(asdict(record))

        rows.sort(
            key=lambda item: (
                -float(item["structural_risk"]),
                -int(item["method_count"]),
                -int(item["field_count"]),
                item["qualified_name"],
            )
        )
        if self.top is not None:
            return rows[: self.top]
        return rows

    def _iter_rust_files(self, paths: Sequence[str]) -> Iterable[Path]:
        seen: Set[Path] = set()
        for raw_path in paths:
            path = Path(raw_path)
            if path.is_file() and path.suffix == ".rs":
                candidates = [path]
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

    def _type_declarations(
        self, source: str, module_key: str, path: str
    ) -> Iterable[TypeRecord]:
        pattern = re.compile(
            r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?P<kind>struct|enum)\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)[^{;]*\{",
            re.MULTILINE,
        )
        for match in pattern.finditer(source):
            kind = match.group("kind")
            name = match.group("name")
            open_index = source.find("{", match.start())
            close_index = matching_brace(source, open_index)
            if close_index is None:
                continue
            body = source[open_index + 1 : close_index]
            start_line = source.count("\n", 0, match.start()) + 1
            end_line = source.count("\n", 0, close_index) + 1
            field_count = self._field_count(body) if kind == "struct" else 0
            variant_count = self._variant_count(body) if kind == "enum" else 0
            yield TypeRecord(
                type_name=name,
                qualified_name=f"{module_key}::{name}",
                module_key=module_key,
                path=path,
                line=start_line,
                kind=kind,
                field_count=field_count,
                variant_count=variant_count,
                declaration_span=end_line - start_line + 1,
            )

    def _field_count(self, body: str) -> int:
        fields = 0
        for line in body.splitlines():
            if re.match(r"^\s*(?:pub(?:\([^)]*\))?\s+)?[A-Za-z_][A-Za-z0-9_]*\s*:", line):
                fields += 1
        return fields

    def _variant_count(self, body: str) -> int:
        variants = 0
        for line in body.splitlines():
            if re.match(r"^\s*[A-Z][A-Za-z0-9_]*(?:\s*[({,]|$)", line):
                variants += 1
        return variants

    def _impl_blocks(self, source: str) -> Iterable[Tuple[str, int]]:
        pattern = re.compile(r"^\s*impl(?:<[^>{}]*>)?\s+(?P<head>[^{]+)\{", re.MULTILINE)
        for match in pattern.finditer(source):
            head = match.group("head").strip()
            type_name = self._impl_type_name(head)
            if not type_name:
                continue
            open_index = source.find("{", match.start())
            close_index = matching_brace(source, open_index)
            if close_index is None:
                continue
            body = source[open_index + 1 : close_index]
            method_count = len(
                re.findall(
                    r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+[A-Za-z_][A-Za-z0-9_]*",
                    body,
                    re.MULTILINE,
                )
            )
            yield type_name, method_count

    def _impl_type_name(self, head: str) -> Optional[str]:
        if " for " in head:
            raw = head.rsplit(" for ", 1)[1]
        else:
            raw = head
        raw = raw.strip().lstrip("&")
        match = re.match(r"([A-Za-z_][A-Za-z0-9_]*)", raw)
        return match.group(1) if match else None

    def _risk(self, record: TypeRecord) -> Tuple[float, List[str]]:
        field_pressure = min(35.0, max(0, record.field_count - 8) * 2.5)
        variant_pressure = min(28.0, max(0, record.variant_count - 8) * 1.8)
        method_pressure = min(32.0, max(0, record.method_count - 12) * 0.85)
        impl_spread_pressure = min(
            24.0,
            max(0, record.impl_file_count - 2) * 4.5
            + max(0, record.impl_block_count - 4) * 1.2,
        )
        declaration_pressure = min(12.0, max(0, record.declaration_span - 30) * 0.35)
        risk = min(
            100.0,
            field_pressure
            + variant_pressure
            + method_pressure
            + impl_spread_pressure
            + declaration_pressure,
        )
        signals = []
        if record.field_count >= 16:
            signals.append(f"wide struct {record.field_count} fields")
        if record.variant_count >= 12:
            signals.append(f"large enum {record.variant_count} variants")
        if record.method_count >= 20:
            signals.append(f"broad method surface {record.method_count}")
        if record.impl_file_count >= 4:
            signals.append(f"impl spread {record.impl_file_count} files")
        if record.impl_block_count >= 6:
            signals.append(f"many impl blocks {record.impl_block_count}")
        if record.declaration_span >= 45:
            signals.append(f"large declaration {record.declaration_span} lines")
        return round(risk, 2), signals or ["stable"]


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


def strip_comments_and_strings(source: str) -> str:
    result: List[str] = []
    index = 0
    state = "code"
    raw_hashes = 0
    while index < len(source):
        ch = source[index]
        nxt = source[index + 1] if index + 1 < len(source) else ""

        if state == "code":
            raw_match = re.match(r"r(#+)?\"", source[index:])
            if ch == "/" and nxt == "/":
                result.extend("  ")
                index += 2
                state = "line_comment"
            elif ch == "/" and nxt == "*":
                result.extend("  ")
                index += 2
                state = "block_comment"
            elif raw_match:
                hashes = raw_match.group(1) or ""
                raw_hashes = len(hashes)
                result.extend(" " * (2 + raw_hashes))
                index += 2 + raw_hashes
                state = "raw_string"
            elif ch == '"':
                result.append(" ")
                index += 1
                state = "string"
            elif ch == "'" and re.match(r"'(?:\\.|[^'\\\n])'", source[index:]):
                result.append(" ")
                index += 1
                state = "char"
            else:
                result.append(ch)
                index += 1
        elif state == "line_comment":
            result.append("\n" if ch == "\n" else " ")
            index += 1
            if ch == "\n":
                state = "code"
        elif state == "block_comment":
            if ch == "*" and nxt == "/":
                result.extend("  ")
                index += 2
                state = "code"
            else:
                result.append("\n" if ch == "\n" else " ")
                index += 1
        elif state == "string":
            if ch == "\\":
                result.extend("  ")
                index += 2
            else:
                result.append("\n" if ch == "\n" else " ")
                index += 1
                if ch == '"':
                    state = "code"
        elif state == "char":
            if ch == "\\":
                result.extend("  ")
                index += 2
            else:
                result.append("\n" if ch == "\n" else " ")
                index += 1
                if ch == "'":
                    state = "code"
        elif state == "raw_string":
            terminator = '"' + ("#" * raw_hashes)
            if source.startswith(terminator, index):
                result.extend(" " * len(terminator))
                index += len(terminator)
                state = "code"
            else:
                result.append("\n" if ch == "\n" else " ")
                index += 1
    return "".join(result)


def module_key_for_path(path: Path) -> str:
    try:
        rel = path.relative_to("src")
    except ValueError:
        rel = path
    parts = list(rel.with_suffix("").parts)
    if parts and parts[-1] == "mod":
        parts = parts[:-1]
    return "::".join(parts)


def render_cli(payload: object) -> str:
    rows = payload if isinstance(payload, list) else []
    lines = ["Type Health"]
    if not rows:
        lines.append("No type records found.")
        return "\n".join(lines)
    for index, item in enumerate(rows[:10], start=1):
        lines.append(
            f"{index:>2}. {item['qualified_name']} | risk={item['structural_risk']:.1f} | fields={item['field_count']} | methods={item['method_count']} | impl_files={item['impl_file_count']}"
        )
    if len(rows) > 10:
        lines.append(f"... and {len(rows) - 10} more types.")
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser(description="Emit Rust type structural health metrics")
    parser.add_argument("--paths", nargs="+", default=["src"], help="Paths to analyze")
    parser.add_argument(
        "--top",
        type=int,
        default=None,
        help="Limit the number of ranked records. Defaults to all records.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help=f"Optional output JSON path. Example: {DEFAULT_OUTPUT}",
    )
    add_mode_argument(parser)
    args = parser.parse_args()
    payload = TypeHealthAnalyzer(top=args.top).run(args.paths)
    emit_report(
        payload,
        mode=args.mode,
        output_path=args.output,
        visibility_path=VISIBILITY_OUTPUT,
        cli_renderer=render_cli,
        label="type health",
    )


if __name__ == "__main__":
    main()
