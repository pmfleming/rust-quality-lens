import argparse
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Dict, Iterable, List, Optional, Sequence, Set

from map import ArchitectureMapper
from report_modes import add_mode_argument, emit_report

DEFAULT_OUTPUT = Path("leverage_metrics.json")
VISIBILITY_OUTPUT = Path("target/analysis/leverage_metrics.json")


class LeverageAnalyzer:
    def __init__(self, top: Optional[int]):
        self.top = top

    def _iter_rust_files(self, paths: Sequence[str]) -> Iterable[Path]:
        for raw_path in paths:
            path = Path(raw_path)
            if path.is_file() and path.suffix == ".rs":
                yield path
                continue
            if path.is_dir():
                yield from path.rglob("*.rs")

    def run(self, paths: Sequence[str]) -> List[Dict]:
        mapper = ArchitectureMapper()
        mapper.extract_dependencies(paths[0] if paths else "src")
        mapper.gather_git_history()

        style_records = {
            item.get("module_key") or item.get("module_name"): item
            for item in self._style_records(paths)
        }
        rows = [
            self._module_record(mapper, module_key, style_records.get(module_key, {}))
            for module_key in sorted(mapper.module_paths)
        ]
        ranked = sorted(
            rows,
            key=lambda item: (
                -float(item.get("leverage_risk", 0.0)),
                item.get("module_key", ""),
            ),
        )
        if self.top is not None:
            return ranked[: self.top]
        return ranked

    def _style_records(self, paths: Sequence[str]) -> List[Dict]:
        all_files = [str(path) for path in self._iter_rust_files(paths)]
        if not all_files:
            return []
        if not shutil.which("cargo"):
            print("Warning: cargo not found; skipping leverage AST style analysis.", file=sys.stderr)
            return []

        with tempfile.NamedTemporaryFile(mode="w", delete=False, encoding="utf-8") as f:
            for file_path in all_files:
                f.write(f"{file_path}\n")
            temp_path = f.name

        cmd = ["cargo", "run", "--quiet"]
        helper_manifest = os.environ.get("RQLENS_HELPER_MANIFEST")
        if helper_manifest:
            cmd.extend(["--manifest-path", helper_manifest])
        cmd.extend(["--bin", "leverage_ast", "--", temp_path])
        try:
            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                check=True,
            )
        except Exception as exc:
            print(f"Warning: Leverage AST style analysis failed: {exc}", file=sys.stderr)
            return []
        finally:
            try:
                os.remove(temp_path)
            except OSError:
                pass

        try:
            records = json.loads(result.stdout)
        except json.JSONDecodeError as exc:
            print(f"Warning: Leverage AST output was malformed: {exc}", file=sys.stderr)
            return []
        return records if isinstance(records, list) else []

    def _module_record(
        self, mapper: ArchitectureMapper, module_key: str, style: Dict
    ) -> Dict:
        inbound = mapper.reverse_dependencies.get(module_key, set())
        outbound = mapper.dependencies.get(module_key, set())
        source = mapper.module_sources.get(module_key, "")
        public_types = self._public_type_count(source)
        public_functions = self._public_function_count(source)
        caller_area_count = len({mapper.layer_name(caller) for caller in inbound})
        divergence_count = self._divergence_count(mapper, module_key, inbound)
        git = mapper.git_history.get(module_key, {})
        avg_cochanged = float(git.get("avg_cochanged_modules", 0.0))
        cochanged_count = int(git.get("cochanged_module_count", 0))
        unsafe_blocks = int(style.get("unsafe_blocks", 0))

        reach = len(inbound)
        pressure_scale = 0.35 + min(0.65, reach / 6.0 * 0.65)
        reach_score = min(22.0, reach * 2.5 + caller_area_count * 4.0)
        invariant_ratio = public_types / max(1, public_types + public_functions)
        invariant_score = min(18.0, public_types * 3.0 + invariant_ratio * 8.0)
        leaf_fit_bonus = 14.0 if reach <= 1 and divergence_count == 0 and unsafe_blocks == 0 else 0.0
        ripple_penalty = (
            min(
                24.0,
                max(0.0, avg_cochanged - 2.0) * 1.1
                + max(0, cochanged_count - 12) * 0.35,
            )
            * pressure_scale
        )
        divergence_penalty = min(28.0, divergence_count * 9.0)
        unsafe_penalty = min(20.0, unsafe_blocks * 4.0)
        surface_penalty = (
            8.0 if reach >= 3 and public_types == 0 and public_functions >= 6 else 0.0
        )
        leverage_score = max(
            0.0,
            min(
                100.0,
                68.0
                + reach_score
                + invariant_score
                + leaf_fit_bonus
                - ripple_penalty
                - divergence_penalty
                - unsafe_penalty
                - surface_penalty,
            ),
        )
        leverage_risk = max(0.0, min(100.0, 100.0 - leverage_score))
        signals = self._signals(
            reach=reach,
            caller_area_count=caller_area_count,
            public_types=public_types,
            public_functions=public_functions,
            divergence_count=divergence_count,
            avg_cochanged=avg_cochanged,
            unsafe_blocks=unsafe_blocks,
            leaf_fit_bonus=leaf_fit_bonus,
            surface_penalty=surface_penalty,
            style=style,
        )

        record = {
            "module_name": module_key,
            "module_key": module_key,
            "path": self._path_for_module(mapper, module_key),
            "leverage_score": round(leverage_score, 2),
            "total_leverage_score": round(leverage_score, 2),
            "leverage_risk": round(leverage_risk, 2),
            "reach": reach,
            "caller_area_count": caller_area_count,
            "outbound_dependencies": len(outbound),
            "public_type_count": public_types,
            "public_function_count": public_functions,
            "invariant_surface": public_types + public_functions,
            "invariant_type_ratio": round(invariant_ratio, 3),
            "divergence_count": divergence_count,
            "avg_cochanged_modules": round(avg_cochanged, 2),
            "cochanged_module_count": cochanged_count,
            "style_leverage_score": float(style.get("total_leverage_score", 0.0)),
            "heap_allocating_type_count": int(style.get("heap_allocating_type_count", 0)),
            "inline_type_count": int(style.get("inline_type_count", 0)),
            "iterator_method_count": int(style.get("iterator_method_count", 0)),
            "for_loop_count": int(style.get("for_loop_count", 0)),
            "indirection_ratio": float(style.get("indirection_ratio", 0.0)),
            "iterator_leverage_score": float(style.get("iterator_leverage_score", 0.0)),
            "unsafe_blocks": unsafe_blocks,
            "leaf_fit_bonus": round(leaf_fit_bonus, 2),
            "surface_penalty": round(surface_penalty, 2),
            "pressure_scale": round(pressure_scale, 3),
            "parse_status": style.get("parse_status", "not_measured"),
            "signals": signals,
            "source": "architecture_static_git",
            "measured_at": datetime.now(timezone.utc).isoformat(),
            "command": " ".join(sys.argv),
            "host": platform.node(),
            "mock": False,
        }
        return record

    def _public_type_count(self, source: str) -> int:
        return len(
            re.findall(
                r"^\s*pub(?:\([^)]*\))?\s+(?:struct|enum|trait)\s+\w+",
                source,
                re.MULTILINE,
            )
        )

    def _public_function_count(self, source: str) -> int:
        return len(
            re.findall(
                r"^\s*pub(?:\([^)]*\))?\s+(?:async\s+)?fn\s+\w+",
                source,
                re.MULTILINE,
            )
        )

    def _defined_function_names(self, source: str) -> Set[str]:
        return set(
            re.findall(
                r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+([a-zA-Z0-9_]+)",
                source,
                re.MULTILINE,
            )
        )

    def _exported_function_names(self, source: str) -> Set[str]:
        return set(
            re.findall(
                r"^\s*pub(?:\([^)]*\))?\s+(?:async\s+)?fn\s+([a-zA-Z0-9_]+)",
                source,
                re.MULTILINE,
            )
        )

    def _divergence_count(
        self, mapper: ArchitectureMapper, module_key: str, inbound: Set[str]
    ) -> int:
        target_tail = module_key.split("::")[-1]
        target_exports = self._exported_function_names(mapper.module_sources.get(module_key, ""))
        divergent_callers = 0
        for caller in inbound:
            caller_source = mapper.module_sources.get(caller, "")
            caller_functions = self._defined_function_names(caller_source)
            mirrored_name = any(
                target_tail in name or any(export in name for export in target_exports)
                for name in caller_functions
            )
            reexport_or_alias = re.search(
                rf"use\s+crate::.*{re.escape(module_key.replace('::', '::'))}.*\s+as\s+",
                caller_source,
            )
            if mirrored_name or reexport_or_alias:
                divergent_callers += 1
        return divergent_callers

    def _signals(
        self,
        *,
        reach: int,
        caller_area_count: int,
        public_types: int,
        public_functions: int,
        divergence_count: int,
        avg_cochanged: float,
        unsafe_blocks: int,
        leaf_fit_bonus: float,
        surface_penalty: float,
        style: Dict,
    ) -> List[str]:
        signals = []
        if leaf_fit_bonus:
            signals.append("self-contained leaf")
        if reach >= 5:
            signals.append(f"high reach {reach}")
        if caller_area_count >= 2:
            signals.append(f"cross-area callers {caller_area_count}")
        if public_types >= 3:
            signals.append(f"invariant surface {public_types} public types")
        elif public_types == 0 and public_functions >= 4:
            signals.append("function-heavy surface")
        if surface_penalty:
            signals.append("shared function-heavy surface")
        if divergence_count:
            signals.append(f"divergence pressure {divergence_count}")
        if avg_cochanged >= 1.5:
            signals.append(f"co-change ripple {avg_cochanged:.1f}")
        if unsafe_blocks:
            signals.append(f"unsafe surface {unsafe_blocks}")
        style_signals = style.get("signals", [])
        if isinstance(style_signals, list):
            signals.extend(str(signal) for signal in style_signals[:2])
        return signals or ["stable"]

    def _path_for_module(self, mapper: ArchitectureMapper, module_key: str) -> str:
        path = mapper.mod_to_file.get(module_key, "")
        try:
            return Path(path).relative_to(Path.cwd()).as_posix()
        except ValueError:
            return Path(path).as_posix()


def render_cli(payload: object) -> str:
    rows = payload if isinstance(payload, list) else []
    lines = ["Leverage Metrics"]
    if not rows:
        lines.append("No leverage metrics found.")
        return "\n".join(lines)

    for index, item in enumerate(rows[:10], start=1):
        lines.append(
            f"{index:>2}. {item.get('module_key') or item['module_name']} | risk={item['leverage_risk']:.1f} | score={item['leverage_score']:.1f} | reach={item['reach']} | divergence={item['divergence_count']} | ripple={item['avg_cochanged_modules']:.1f}"
        )

    if len(rows) > 10:
        lines.append(f"... and {len(rows) - 10} more modules.")

    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Emit architecture leverage metrics as JSON"
    )
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
    analyzer = LeverageAnalyzer(top=args.top)
    payload = analyzer.run(args.paths)

    emit_report(
        payload,
        mode=args.mode,
        output_path=args.output,
        visibility_path=VISIBILITY_OUTPUT,
        cli_renderer=render_cli,
        label="leverage",
    )


if __name__ == "__main__":
    main()
