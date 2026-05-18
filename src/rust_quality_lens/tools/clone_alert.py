import argparse
import bisect
import hashlib
import json
import re
import shutil
import subprocess
import sys
from collections import defaultdict
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import DefaultDict, Dict, Iterable, List, Sequence, Set, Tuple

from report_modes import add_mode_argument, emit_report

DEFAULT_OUTPUT = Path("clones.json")
VISIBILITY_OUTPUT = Path("target/analysis/clones.json")

RUST_KEYWORDS = {
    "Self",
    "as",
    "async",
    "await",
    "break",
    "const",
    "continue",
    "crate",
    "dyn",
    "else",
    "enum",
    "extern",
    "false",
    "fn",
    "for",
    "if",
    "impl",
    "in",
    "let",
    "loop",
    "match",
    "mod",
    "move",
    "mut",
    "pub",
    "ref",
    "return",
    "self",
    "static",
    "struct",
    "super",
    "trait",
    "true",
    "type",
    "unsafe",
    "use",
    "where",
    "while",
}

RUST_TOKEN_RE = re.compile(
    r"""
    (?P<comment>//[^\n]*|/\*[\s\S]*?\*/)
    |(?P<string>b?"(?:\\.|[^"\\])*")
    |(?P<char>b?'(?:\\.|[^'\\])')
    |(?P<number>\b(?:0x[0-9A-Fa-f_]+|0b[01_]+|0o[0-7_]+|\d[\d_]*(?:\.\d[\d_]*)?)\b)
    |(?P<ident>[A-Za-z_][A-Za-z0-9_]*)
    |(?P<punct>::|->|=>|==|!=|<=|>=|&&|\|\||[{}()[\];,.:+\-*/%&|^!<>=?])
    |(?P<space>\s+)
    """,
    re.VERBOSE,
)


@dataclass(frozen=True)
class CloneInstance:
    file_path: str
    start_line: int
    end_line: int
    snippet: str


@dataclass(frozen=True)
class CloneGroup:
    engine: str
    hash: str
    token_count: int
    instance_count: int
    file_count: int
    max_line_span: int
    score: float
    signals: str
    instances: List[CloneInstance]


class CloneAnalyzer:
    def __init__(
        self,
        min_tokens: int = 50,
        min_lines: int = 3,
        normalize: bool = True,
        top: int | None = None,
    ):
        self.min_tokens = min_tokens
        self.min_lines = min_lines
        self.normalize = normalize
        self.top = top

    def run(self, paths: Sequence[str], engine: str) -> List[CloneGroup]:
        if engine == "token":
            return self.find_token_clones(paths)
        if engine == "ast":
            return self.find_ast_clones(paths)
        if engine == "mir":
            print(
                "Warning: MIR clone analysis is experimental and not wired into the stable toolchain yet.",
                file=sys.stderr,
            )
            return []
        if engine == "all":
            groups = self.find_token_clones(paths)
            groups.extend(self.find_ast_clones(paths))
            return self._rank(groups)
        raise ValueError(f"Unsupported clone engine: {engine}")

    def tokenize(self, code: str) -> List[Tuple[str, str, int]]:
        line_starts = [0]
        for match in re.finditer(r"\n", code):
            line_starts.append(match.end())

        tokens: List[Tuple[str, str, int]] = []
        for match in RUST_TOKEN_RE.finditer(code):
            kind = match.lastgroup
            if kind is None or kind in {"comment", "space"}:
                continue

            value = match.group(kind)
            line = bisect.bisect_right(line_starts, match.start())

            if self.normalize:
                if kind == "ident" and value not in RUST_KEYWORDS:
                    value = "ID"
                elif kind in {"string", "char", "number"}:
                    value = "LIT"

            tokens.append((kind, value, line))
        return tokens

    def _iter_rust_files(self, paths: Sequence[str]) -> Iterable[Path]:
        for raw_path in paths:
            path = Path(raw_path)
            if path.is_file() and path.suffix == ".rs":
                yield path
                continue
            if path.is_dir():
                yield from path.rglob("*.rs")

    def _read_rust_inputs(
        self,
        paths: Sequence[str],
    ) -> Tuple[Dict[str, List[str]], Dict[str, List[Tuple[str, str, int]]]]:
        file_contents: Dict[str, List[str]] = {}
        file_tokens: Dict[str, List[Tuple[str, str, int]]] = {}
        seen_files: Set[str] = set()
        for rust_file in self._iter_rust_files(paths):
            file_key = str(rust_file)
            if file_key in seen_files:
                continue
            seen_files.add(file_key)
            try:
                content = rust_file.read_text(encoding="utf-8")
            except Exception as exc:
                print(f"Warning: Could not read {rust_file}: {exc}", file=sys.stderr)
                continue
            tokens = self.tokenize(content)
            file_contents[file_key] = content.splitlines()
            if len(tokens) >= self.min_tokens:
                file_tokens[file_key] = tokens
        return file_contents, file_tokens

    def _token_windows(
        self,
        file_tokens: Dict[str, List[Tuple[str, str, int]]],
    ) -> DefaultDict[str, List[Tuple[str, int, int, int]]]:
        windows: DefaultDict[str, List[Tuple[str, int, int, int]]] = defaultdict(list)
        for file_key, tokens in file_tokens.items():
            for start_idx in range(len(tokens) - self.min_tokens + 1):
                window = tokens[start_idx : start_idx + self.min_tokens]
                start_line = window[0][2]
                end_line = window[-1][2]
                if (end_line - start_line + 1) < self.min_lines:
                    continue
                token_str = "|".join(f"{kind}:{value}" for kind, value, _ in window)
                window_hash = hashlib.md5(token_str.encode("utf-8")).hexdigest()
                windows[window_hash].append((file_key, start_idx, start_line, end_line))
        return windows

    def _accepted_token_instances(
        self,
        matches: Sequence[Tuple[str, int, int, int]],
        file_contents: Dict[str, List[str]],
        globally_used: Dict[str, List[range]],
    ) -> Tuple[List[CloneInstance], List[Tuple[str, int]]]:
        instances: List[CloneInstance] = []
        accepted_matches: List[Tuple[str, int]] = []
        for file_path, start_idx, start_line, end_line in matches:
            candidate_range = range(start_idx, start_idx + self.min_tokens)
            if self._overlaps_existing(globally_used[file_path], candidate_range):
                continue
            instances.append(
                CloneInstance(
                    file_path=file_path,
                    start_line=start_line,
                    end_line=end_line,
                    snippet="\n".join(file_contents[file_path][start_line - 1 : end_line]),
                )
            )
            accepted_matches.append((file_path, start_idx))
        return instances, accepted_matches

    def _token_clone_group(
        self,
        window_hash: str,
        instances: Sequence[CloneInstance],
    ) -> CloneGroup:
        instance_count = len(instances)
        file_count = len({instance.file_path for instance in instances})
        max_line_span = max(
            instance.end_line - instance.start_line + 1 for instance in instances
        )
        return CloneGroup(
            engine="token",
            hash=window_hash,
            token_count=self.min_tokens,
            instance_count=instance_count,
            file_count=file_count,
            max_line_span=max_line_span,
            score=self.calculate_score(instance_count, self.min_tokens),
            signals=self.generate_signals(
                instance_count,
                file_count,
                self.min_tokens,
                max_line_span,
            ),
            instances=list(instances),
        )

    @staticmethod
    def _write_file_list(files: Sequence[str]) -> str:
        import tempfile

        with tempfile.NamedTemporaryFile(mode="w", delete=False, encoding="utf-8") as handle:
            for file_path in files:
                handle.write(f"{file_path}\n")
            return handle.name

    @staticmethod
    def _run_ast_hasher(temp_path: str) -> List[Dict[str, object]]:
        import os

        cmd = ["cargo", "run", "--quiet"]
        helper_manifest = os.environ.get("RQLENS_HELPER_MANIFEST")
        if helper_manifest:
            cmd.extend(["--manifest-path", helper_manifest])
        cmd.extend(["--bin", "ast_hasher", "--", temp_path])
        try:
            result = subprocess.run(cmd, capture_output=True, text=True, check=True)
        except Exception as exc:
            print(f"Warning: AST clone analysis failed: {exc}", file=sys.stderr)
            return []
        finally:
            try:
                os.remove(temp_path)
            except OSError:
                pass

        try:
            records = json.loads(result.stdout)
        except json.JSONDecodeError as exc:
            print(f"Warning: AST clone output was malformed: {exc}", file=sys.stderr)
            return []
        return records if isinstance(records, list) else []

    @staticmethod
    def _ast_instances_by_hash(records: Sequence[Dict[str, object]]) -> DefaultDict[str, List[CloneInstance]]:
        groups_by_hash: DefaultDict[str, List[CloneInstance]] = defaultdict(list)
        for record in records:
            start_line = int(record.get("start_line", 0))
            end_line = int(record.get("end_line", start_line))
            if start_line <= 0 or end_line < start_line:
                continue
            groups_by_hash[str(record["ast_hash"])].append(
                CloneInstance(
                    file_path=str(record["file"]),
                    start_line=start_line,
                    end_line=end_line,
                    snippet=f"fn {record['name']} (...)",
                )
            )
        return groups_by_hash

    def _ast_clone_group(
        self,
        ast_hash: str,
        instances: Sequence[CloneInstance],
    ) -> CloneGroup:
        file_count = len({instance.file_path for instance in instances})
        max_line_span = max(
            instance.end_line - instance.start_line + 1 for instance in instances
        )
        token_count = max(self.min_tokens, max_line_span * 8)
        signals = [f"cross-file x{file_count}" if file_count >= 2 else "same-file repeat"]
        signals.append("ast-normalized")
        if len(instances) >= 4:
            signals.append(f"high reuse x{len(instances)}")
        return CloneGroup(
            engine="ast",
            hash=ast_hash,
            token_count=token_count,
            instance_count=len(instances),
            file_count=file_count,
            max_line_span=max_line_span,
            score=self.calculate_score(len(instances), token_count),
            signals=", ".join(signals),
            instances=list(instances),
        )

    def find_token_clones(self, paths: Sequence[str]) -> List[CloneGroup]:
        file_contents, file_tokens = self._read_rust_inputs(paths)
        windows = self._token_windows(file_tokens)
        groups: List[CloneGroup] = []
        globally_used: Dict[str, List[range]] = defaultdict(list)

        for window_hash, matches in sorted(
            windows.items(), key=lambda item: len(item[1]), reverse=True
        ):
            if len(matches) < 2:
                continue

            instances, accepted_matches = self._accepted_token_instances(
                matches,
                file_contents,
                globally_used,
            )
            if len(instances) < 2:
                continue

            for file_path, start_idx in accepted_matches:
                globally_used[file_path].append(range(start_idx, start_idx + self.min_tokens))

            merged_instances = self._merge_instances(instances)
            if len(merged_instances) < 2:
                continue

            groups.append(self._token_clone_group(window_hash, merged_instances))

        return self._rank(groups)

    def find_ast_clones(self, paths: Sequence[str]) -> List[CloneGroup]:
        all_files = [str(path) for path in self._iter_rust_files(paths)]
        if not all_files:
            return []
        if not shutil.which("cargo"):
            print("Warning: cargo not found; skipping AST clone analysis.", file=sys.stderr)
            return []

        records = self._run_ast_hasher(self._write_file_list(all_files))
        groups_by_hash = self._ast_instances_by_hash(records)

        groups: List[CloneGroup] = []
        for ast_hash, instances in groups_by_hash.items():
            if len(instances) < 2:
                continue
            groups.append(self._ast_clone_group(ast_hash, instances))

        return self._rank(groups)

    def _rank(self, groups: List[CloneGroup]) -> List[CloneGroup]:
        ranked = sorted(groups, key=lambda group: (-group.score, group.engine, group.hash))
        if self.top is not None:
            return ranked[: self.top]
        return ranked

    def calculate_score(self, count: int, tokens: int) -> float:
        return round((count * tokens) / 10.0, 2)

    def generate_signals(
        self, count: int, file_count: int, tokens: int, max_line_span: int
    ) -> str:
        signals: List[str] = []
        if file_count >= 2:
            signals.append(f"cross-file x{file_count}")
        else:
            signals.append("same-file repeat")
        if count >= 4:
            signals.append(f"high reuse x{count}")
        if tokens >= 80:
            signals.append(f"long window {tokens} tokens")
        if max_line_span >= 12:
            signals.append(f"wide span {max_line_span} lines")
        return ", ".join(signals) if signals else "watch"

    @staticmethod
    def _merge_instances(instances: Sequence[CloneInstance]) -> List[CloneInstance]:
        grouped: DefaultDict[str, List[CloneInstance]] = defaultdict(list)
        for instance in instances:
            grouped[instance.file_path].append(instance)

        merged_instances: List[CloneInstance] = []
        for file_path, file_instances in grouped.items():
            sorted_instances = sorted(
                file_instances,
                key=lambda instance: (instance.start_line, instance.end_line),
            )
            current = sorted_instances[0]

            for candidate in sorted_instances[1:]:
                if candidate.start_line <= current.end_line:
                    current = CloneInstance(
                        file_path=file_path,
                        start_line=current.start_line,
                        end_line=max(current.end_line, candidate.end_line),
                        snippet=current.snippet,
                    )
                    continue

                merged_instances.append(current)
                current = candidate

            merged_instances.append(current)

        return sorted(
            merged_instances,
            key=lambda instance: (instance.file_path, instance.start_line, instance.end_line),
        )

    @staticmethod
    def _overlaps_existing(ranges: Sequence[range], candidate: range) -> bool:
        for existing in ranges:
            if candidate.start < existing.stop and existing.start < candidate.stop:
                return True
        return False


def render_cli(payload: object) -> str:
    groups = payload if isinstance(payload, list) else []
    lines = ["Clone Alert"]
    if not groups:
        lines.append("No significant clones detected.")
        return "\n".join(lines)

    for index, group in enumerate(groups[:5], start=1):
        lines.append(
            f"{index:>2}. [{group.get('engine', 'token')}] Clone Group {group['hash'][:8]} | score={group['score']:.2f} | tokens={group['token_count']} | instances={group['instance_count']} | files={group['file_count']}"
        )
        for instance in group["instances"]:
            lines.append(
                f"    - {instance['file_path']}:{instance['start_line']}-{instance['end_line']}"
            )

    if len(groups) > 5:
        lines.append(f"... and {len(groups) - 5} more groups.")

    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Detect token-based Rust code clones and emit report data"
    )
    parser.add_argument("--paths", nargs="+", default=["src"], help="Paths to analyze")
    parser.add_argument(
        "--min-tokens",
        type=int,
        default=50,
        help="Minimum token sequence length used for clone windows",
    )
    parser.add_argument(
        "--min-lines",
        type=int,
        default=3,
        help="Minimum line span for a reported clone window",
    )
    parser.add_argument(
        "--top",
        type=int,
        default=None,
        help="Limit the number of ranked clone groups. Defaults to all groups.",
    )
    parser.add_argument(
        "--no-normalize",
        action="store_false",
        dest="normalize",
        help="Disable identifier/literal normalization and only detect near-exact clones",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help=f"Optional output JSON path. Example: {DEFAULT_OUTPUT}",
    )
    parser.add_argument(
        "--engine",
        choices=["token", "ast", "mir", "all"],
        default="token",
        help="Clone analysis engine. 'all' combines the supported engines.",
    )
    add_mode_argument(parser)

    args = parser.parse_args()
    analyzer = CloneAnalyzer(
        min_tokens=args.min_tokens,
        min_lines=args.min_lines,
        normalize=args.normalize,
        top=args.top,
    )
    payload = [asdict(group) for group in analyzer.run(args.paths, args.engine)]
    emit_report(
        payload,
        mode=args.mode,
        output_path=args.output,
        visibility_path=VISIBILITY_OUTPUT,
        cli_renderer=render_cli,
        label="clone",
    )


if __name__ == "__main__":
    main()
