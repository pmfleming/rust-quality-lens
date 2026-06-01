import argparse
import os
import re
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Dict, List, Optional, Sequence, Set

from common import provenance
from map import ArchitectureMapper
from report_modes import add_mode_argument, emit_report
from risk_model import bounded_score, inverse_risk, tool_calibration, tool_score_metadata

DEFAULT_OUTPUT = Path("locality_metrics.json")
VISIBILITY_OUTPUT = Path("target/analysis/locality_metrics.json")


@dataclass
class CodeLocalityMetrics:
    module_name: str
    module_key: str
    path: str
    locality_score: float
    code_locality_score: float
    locality_risk: float
    non_locality_risk: float
    outbound_dependencies: int
    inbound_dependencies: int
    far_dependencies: int
    layer_violations: int
    hidden_coupling_count: int
    module_static_count: int
    singleton_access_count: int
    public_function_count: int
    explicit_public_function_count: int
    self_method_count: int
    interface_explicitness_ratio: float
    churn: int
    commit_count: int
    contributor_count: int
    has_inline_tests: bool
    external_test_refs: int
    test_locality: str
    signals: List[str]
    signal_weights: Dict[str, float]
    measured_at: str
    command: str
    host: str
    measurement_confidence: Dict[str, object]
    risk_model_id: str
    risk_model_version: int
    risk_calibration: str
    source: str = "static_code_git"
    mock: bool = False


class CodeLocalityAnalyzer:
    def __init__(self, top: Optional[int] = None):
        self.top = top

    def run(self, paths: Sequence[str]) -> List[Dict]:
        source_roots = list(paths) if paths else ["src"]
        mapper = ArchitectureMapper(
            project_name=os.environ.get("RQLENS_PROJECT_NAME", "Rust Project"),
            source_roots=source_roots,
        )
        mapper.extract_dependencies()
        mapper.gather_test_support()
        mapper.gather_git_history()
        confidence = mapper.measurement_confidence()

        rows = [
            asdict(self._metrics_for_module(mapper, module_key, confidence))
            for module_key in sorted(mapper.module_paths)
        ]
        ranked = sorted(rows, key=lambda item: (item["locality_score"], item["module_key"]))
        if self.top is not None:
            return ranked[: self.top]
        return ranked

    def _metrics_for_module(
        self, mapper: ArchitectureMapper, module_key: str, confidence: Dict[str, object]
    ) -> CodeLocalityMetrics:
        outbound = mapper.dependencies.get(module_key, set())
        inbound = mapper.reverse_dependencies.get(module_key, set())
        far_dependencies = self._far_dependency_count(module_key, outbound)
        layer_violations = mapper._count_layer_violations(module_key)
        source = mapper.module_sources.get(module_key, "")
        hidden_coupling = self._hidden_coupling(source)
        interface = self._interface_explicitness(source)
        tests = mapper.test_support.get(module_key, {})
        git = mapper.git_history.get(module_key, {})
        has_inline_tests = bool(tests.get("has_inline_tests", False))
        external_refs = tests.get("external_refs", [])
        external_test_refs = len(external_refs) if isinstance(external_refs, list) else 0
        has_tests = has_inline_tests or external_test_refs > 0
        test_locality = self._test_locality(has_inline_tests, external_test_refs)
        churn = int(git.get("churn", 0))
        commit_count = int(git.get("commits", 0))
        contributor_count = int(git.get("contributor_count", 0))

        risk = self._risk_score(
            outbound_count=len(outbound),
            inbound_count=len(inbound),
            far_dependencies=far_dependencies,
            layer_violations=layer_violations,
            churn=churn,
            contributor_count=contributor_count,
            hidden_coupling_count=hidden_coupling["hidden_coupling_count"],
            interface_explicitness_ratio=interface["interface_explicitness_ratio"],
            has_inline_tests=has_inline_tests,
            has_tests=has_tests,
        )
        score = inverse_risk(risk, cap=tool_calibration("locality")["score_cap"])

        signals = self._signals(
            far_dependencies=far_dependencies,
            layer_violations=layer_violations,
            outbound_count=len(outbound),
            inbound_count=len(inbound),
            churn=churn,
            contributor_count=contributor_count,
            hidden_coupling=hidden_coupling,
            interface=interface,
            has_inline_tests=has_inline_tests,
            has_tests=has_tests,
        )
        provenance = self._provenance()
        score_metadata = tool_score_metadata("locality")

        return CodeLocalityMetrics(
            module_name=module_key,
            module_key=module_key,
            path=self._path_for_module(mapper, module_key),
            locality_score=score,
            code_locality_score=score,
            locality_risk=risk,
            non_locality_risk=risk,
            outbound_dependencies=len(outbound),
            inbound_dependencies=len(inbound),
            far_dependencies=far_dependencies,
            layer_violations=layer_violations,
            hidden_coupling_count=hidden_coupling["hidden_coupling_count"],
            module_static_count=hidden_coupling["module_static_count"],
            singleton_access_count=hidden_coupling["singleton_access_count"],
            public_function_count=interface["public_function_count"],
            explicit_public_function_count=interface["explicit_public_function_count"],
            self_method_count=interface["self_method_count"],
            interface_explicitness_ratio=interface["interface_explicitness_ratio"],
            churn=churn,
            commit_count=commit_count,
            contributor_count=contributor_count,
            has_inline_tests=has_inline_tests,
            external_test_refs=external_test_refs,
            test_locality=test_locality,
            signals=list(signals.keys()),
            signal_weights=signals,
            measured_at=provenance["measured_at"],
            command=provenance["command"],
            host=provenance["host"],
            measurement_confidence=confidence,
            risk_model_id=str(score_metadata["risk_model_id"]),
            risk_model_version=int(score_metadata["risk_model_version"]),
            risk_calibration=str(score_metadata["risk_calibration"]),
        )

    def _risk_score(
        self,
        *,
        outbound_count: int,
        inbound_count: int,
        far_dependencies: int,
        layer_violations: int,
        churn: int,
        contributor_count: int,
        hidden_coupling_count: int,
        interface_explicitness_ratio: float,
        has_inline_tests: bool,
        has_tests: bool,
    ) -> float:
        calibration = tool_calibration("locality")
        dependency = calibration["dependency_spread"]
        dependency_spread = min(
            dependency["cap"],
            far_dependencies * dependency["far_dependency_weight"]
            + layer_violations * dependency["layer_violation_weight"]
            + max(0, outbound_count - dependency["outbound_free"]) * dependency["outbound_weight"]
            + max(0, inbound_count - dependency["inbound_free"]) * dependency["inbound_weight"],
        )
        hidden_calibration = calibration["hidden_coupling"]
        hidden_coupling = min(
            hidden_calibration["cap"],
            hidden_coupling_count * hidden_calibration["weight"],
        )
        interface = calibration["interface_penalty"]
        interface_penalty = (
            interface["penalty"]
            if interface_explicitness_ratio < interface["explicitness_threshold"]
            and outbound_count + inbound_count >= interface["coupling_threshold"]
            else 0.0
        )
        test_calibration = calibration["test_distance"]
        test_distance = 0.0 if has_inline_tests else test_calibration["external_only"] if has_tests else test_calibration["missing"]
        change = calibration["change_spread"]
        change_spread = min(
            change["cap"],
            churn / change["churn_divisor"]
            + max(0, contributor_count - change["contributor_free"])
            * change["contributor_weight"],
        )
        return bounded_score(
            dependency_spread
            + hidden_coupling
            + interface_penalty
            + test_distance
            + change_spread,
            cap=calibration["score_cap"],
        )

    def _signals(
        self,
        *,
        far_dependencies: int,
        layer_violations: int,
        outbound_count: int,
        inbound_count: int,
        churn: int,
        contributor_count: int,
        hidden_coupling: Dict[str, int],
        interface: Dict[str, float],
        has_inline_tests: bool,
        has_tests: bool,
    ) -> Dict[str, float]:
        signals = {}
        calibration = tool_calibration("locality")
        dependency = calibration["dependency_spread"]
        if far_dependencies:
            signals[f"far dependencies {far_dependencies}"] = (
                far_dependencies * dependency["far_dependency_weight"]
            )
        if layer_violations:
            signals[f"layer violations {layer_violations}"] = (
                layer_violations * dependency["layer_violation_weight"]
            )
        if outbound_count > dependency["outbound_free"]:
            signals[f"broad outbound surface {outbound_count}"] = (
                max(1, outbound_count - dependency["outbound_free"])
                * dependency["outbound_weight"]
            )
        if inbound_count >= dependency["inbound_free"]:
            signals[f"shared by many modules {inbound_count}"] = (
                max(1, inbound_count - dependency["inbound_free"])
                * dependency["inbound_weight"]
            )
        hidden_count = int(hidden_coupling["hidden_coupling_count"])
        hidden_calibration = calibration["hidden_coupling"]
        if hidden_count:
            signals[f"hidden coupling signals {hidden_count}"] = min(
                hidden_calibration["cap"],
                hidden_count * hidden_calibration["weight"],
            )
        explicitness = float(interface["interface_explicitness_ratio"])
        interface_calibration = calibration["interface_penalty"]
        if (
            explicitness < interface_calibration["explicitness_threshold"]
            and outbound_count + inbound_count >= interface_calibration["coupling_threshold"]
        ):
            signals[f"low explicit interface {explicitness:.2f}"] = interface_calibration["penalty"]
        if not has_tests:
            signals["no nearby tests"] = calibration["test_distance"]["missing"]
        elif not has_inline_tests:
            signals["external tests only"] = calibration["test_distance"]["external_only"]
        change = calibration["change_spread"]
        signal_thresholds = calibration["signals"]
        if churn >= signal_thresholds["high_churn"]:
            signals[f"high churn {churn}"] = min(change["cap"], churn / change["churn_divisor"])
        if contributor_count >= signal_thresholds["many_contributors"]:
            signals[f"many contributors {contributor_count}"] = (
                max(1, contributor_count - change["contributor_free"])
                * change["contributor_weight"]
            )
        return signals

    def _far_dependency_count(self, module_key: str, outbound: Set[str]) -> int:
        return sum(1 for dependency in outbound if not self._is_near_dependency(module_key, dependency))

    def _is_near_dependency(self, module_key: str, dependency: str) -> bool:
        if dependency == module_key:
            return True
        if dependency.startswith(f"{module_key}::") or module_key.startswith(f"{dependency}::"):
            return True
        module_parent = module_key.rsplit("::", 1)[0] if "::" in module_key else module_key
        dependency_parent = dependency.rsplit("::", 1)[0] if "::" in dependency else dependency
        return module_parent == dependency_parent

    def _hidden_coupling(self, source: str) -> Dict[str, int]:
        module_statics = len(
            re.findall(
                r"^\s*(?:pub(?:\([^)]*\))?\s+)?static\s+",
                source,
                re.MULTILINE,
            )
        )
        singleton_accesses = len(
            re.findall(
                r"\b(?:thread_local!|lazy_static!|OnceCell|OnceLock|get_or_init|global|singleton|instance)\b",
                source,
            )
        )
        return {
            "hidden_coupling_count": module_statics + singleton_accesses,
            "module_static_count": module_statics,
            "singleton_access_count": singleton_accesses,
        }

    def _interface_explicitness(self, source: str) -> Dict[str, float]:
        public_functions = re.findall(
            r"^\s*pub(?:\([^)]*\))?\s+(?:async\s+)?fn\s+\w+\s*\(([^)]*)\)",
            source,
            re.MULTILINE,
        )
        explicit_functions = 0
        self_methods = 0
        for raw_args in public_functions:
            args = raw_args.strip()
            if re.search(r"(^|,)\s*&?\s*(?:mut\s+)?self\b", args):
                self_methods += 1
            elif args:
                explicit_functions += 1
        public_count = len(public_functions)
        ratio = explicit_functions / public_count if public_count else 1.0
        return {
            "public_function_count": public_count,
            "explicit_public_function_count": explicit_functions,
            "self_method_count": self_methods,
            "interface_explicitness_ratio": ratio,
        }

    def _path_for_module(self, mapper: ArchitectureMapper, module_key: str) -> str:
        path = mapper.mod_to_file.get(module_key, "")
        try:
            return Path(path).relative_to(Path.cwd()).as_posix()
        except ValueError:
            return Path(path).as_posix()

    @staticmethod
    def _test_locality(has_inline_tests: bool, external_test_refs: int) -> str:
        if has_inline_tests:
            return "inline"
        if external_test_refs:
            return "external"
        return "none"

    def _provenance(self) -> Dict[str, str]:
        return provenance()


def render_cli(payload: object) -> str:
    rows = payload if isinstance(payload, list) else []
    lines = ["Code Locality Metrics"]
    if not rows:
        lines.append("No code locality metrics found.")
        return "\n".join(lines)

    for index, item in enumerate(rows[:10], start=1):
        lines.append(
            f"{index:>2}. {item['module_key']} | risk={item['non_locality_risk']:.1f} | score={item['locality_score']:.1f} | far={item['far_dependencies']} | hidden={item['hidden_coupling_count']} | deps={item['outbound_dependencies']}/{item['inbound_dependencies']} | tests={item['test_locality']}"
        )

    if len(rows) > 10:
        lines.append(f"... and {len(rows) - 10} more modules.")

    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser(description="Emit static code locality metrics as JSON")
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
    analyzer = CodeLocalityAnalyzer(top=args.top)
    try:
        payload = analyzer.run(args.paths)
    except Exception as exc:
        print(f"Error: code locality analysis failed: {exc}", file=sys.stderr)
        raise

    emit_report(
        payload,
        mode=args.mode,
        output_path=args.output,
        visibility_path=VISIBILITY_OUTPUT,
        cli_renderer=render_cli,
        label="code locality",
    )


if __name__ == "__main__":
    main()
