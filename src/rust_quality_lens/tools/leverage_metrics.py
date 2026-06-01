import argparse
import os
import re
import shutil
import sys
from pathlib import Path
from typing import Dict, Iterable, List, Optional, Sequence, Set

from common import iter_rust_files, provenance, run_helper_json
from map import ArchitectureMapper
from report_modes import add_mode_argument, emit_report
from risk_model import bounded_score, inverse_risk, tool_calibration, tool_score_metadata

DEFAULT_OUTPUT = Path("leverage_metrics.json")
VISIBILITY_OUTPUT = Path("target/analysis/leverage_metrics.json")


class LeverageAnalyzer:
    def __init__(self, top: Optional[int]):
        self.top = top

    def _iter_rust_files(self, paths: Sequence[str]) -> Iterable[Path]:
        yield from iter_rust_files(paths)

    def run(self, paths: Sequence[str]) -> List[Dict]:
        source_roots = list(paths) if paths else ["src"]
        mapper = ArchitectureMapper(
            project_name=os.environ.get("RQLENS_PROJECT_NAME", "Rust Project"),
            source_roots=source_roots,
        )
        mapper.extract_dependencies()
        mapper.gather_git_history()
        confidence = mapper.measurement_confidence()

        style_records = {
            item.get("module_key") or item.get("module_name"): item
            for item in self._style_records(paths)
        }
        rows = [
            self._module_record(mapper, module_key, style_records.get(module_key, {}), confidence)
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

        return run_helper_json("leverage_ast", all_files, "Leverage AST style analysis")

    def _module_record(
        self, mapper: ArchitectureMapper, module_key: str, style: Dict, confidence: Dict[str, object]
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
        run_provenance = provenance()
        calibration = tool_calibration("leverage")
        score_metadata = tool_score_metadata("leverage")

        reach = len(inbound)
        pressure = calibration["pressure_scale"]
        pressure_scale = pressure["base"] + min(
            pressure["cap"],
            reach / pressure["reach_full_scale"] * pressure["cap"],
        )
        reach_calibration = calibration["reach_score"]
        reach_score = min(
            reach_calibration["cap"],
            reach * reach_calibration["reach_weight"]
            + caller_area_count * reach_calibration["caller_area_weight"],
        )
        invariant_ratio = public_types / max(1, public_types + public_functions)
        invariant = calibration["invariant_score"]
        invariant_score = min(
            invariant["cap"],
            public_types * invariant["public_type_weight"]
            + invariant_ratio * invariant["ratio_weight"],
        )
        leaf_fit_bonus = (
            calibration["leaf_fit_bonus"]
            if reach <= 1 and divergence_count == 0 and unsafe_blocks == 0
            else 0.0
        )
        ripple = calibration["ripple_penalty"]
        ripple_penalty = (
            min(
                ripple["cap"],
                max(0.0, avg_cochanged - ripple["avg_cochanged_free"])
                * ripple["avg_cochanged_weight"]
                + max(0, cochanged_count - ripple["cochanged_free"])
                * ripple["cochanged_weight"],
            )
            * pressure_scale
        )
        divergence = calibration["divergence_penalty"]
        unsafe = calibration["unsafe_penalty"]
        divergence_penalty = min(
            divergence["cap"],
            divergence_count * divergence["weight"],
        )
        unsafe_penalty = min(unsafe["cap"], unsafe_blocks * unsafe["weight"])
        surface = calibration["surface_penalty"]
        surface_penalty = (
            surface["penalty"]
            if reach >= surface["reach_threshold"]
            and public_types == 0
            and public_functions >= surface["public_function_threshold"]
            else 0.0
        )
        leverage_score = bounded_score(
            calibration["base_score"]
            + reach_score
            + invariant_score
            + leaf_fit_bonus
            - ripple_penalty
            - divergence_penalty
            - unsafe_penalty
            - surface_penalty,
            cap=calibration["score_cap"],
        )
        leverage_risk = inverse_risk(leverage_score, cap=calibration["score_cap"])
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
            "measurement_confidence": confidence,
            "risk_model_id": score_metadata["risk_model_id"],
            "risk_model_version": score_metadata["risk_model_version"],
            "risk_calibration": score_metadata["risk_calibration"],
            "signals": signals,
            "source": "architecture_static_git",
            "measured_at": run_provenance["measured_at"],
            "command": run_provenance["command"],
            "host": run_provenance["host"],
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
        calibration = tool_calibration("leverage")
        signals = []
        if leaf_fit_bonus:
            signals.append("self-contained leaf")
        if reach >= 5:
            signals.append(f"high reach {reach}")
        if caller_area_count >= 2:
            signals.append(f"cross-area callers {caller_area_count}")
        if public_types >= 3:
            signals.append(f"invariant surface {public_types} public types")
        elif (
            public_types == 0
            and public_functions >= calibration["surface_penalty"]["public_function_threshold"] - 2
        ):
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
