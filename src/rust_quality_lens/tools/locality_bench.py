import argparse
import re
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Dict, List, Optional, Sequence, Set

from common import provenance
from map import ArchitectureMapper
from report_modes import add_mode_argument, emit_report

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
    source: str = "static_code_git"
    mock: bool = False


class CodeLocalityAnalyzer:
    def __init__(self, top: Optional[int] = None):
        self.top = top

    def run(self, paths: Sequence[str]) -> List[Dict]:
        mapper = ArchitectureMapper()
        mapper.extract_dependencies(paths[0] if paths else "src")
        mapper.gather_test_support()
        mapper.gather_git_history()

        rows = [
            asdict(self._metrics_for_module(mapper, module_key))
            for module_key in sorted(mapper.module_paths)
        ]
        ranked = sorted(rows, key=lambda item: (item["locality_score"], item["module_key"]))
        if self.top is not None:
            return ranked[: self.top]
        return ranked

    def _metrics_for_module(
        self, mapper: ArchitectureMapper, module_key: str
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
        score = max(0.0, min(100.0, 100.0 - risk))

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
        dependency_spread = min(
            48.0,
            far_dependencies * 9.0
            + layer_violations * 16.0
            + max(0, outbound_count - 5) * 3.0
            + max(0, inbound_count - 12) * 0.75,
        )
        hidden_coupling = min(24.0, hidden_coupling_count * 8.0)
        interface_penalty = (
            10.0
            if interface_explicitness_ratio < 0.25 and outbound_count + inbound_count >= 4
            else 0.0
        )
        test_distance = 0.0 if has_inline_tests else 0.5 if has_tests else 1.0
        change_spread = min(18.0, churn / 160.0 + max(0, contributor_count - 3) * 2.0)
        return min(
            100.0,
            dependency_spread
            + hidden_coupling
            + interface_penalty
            + test_distance
            + change_spread,
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
        if far_dependencies:
            signals[f"far dependencies {far_dependencies}"] = far_dependencies * 9.0
        if layer_violations:
            signals[f"layer violations {layer_violations}"] = layer_violations * 16.0
        if outbound_count >= 6:
            signals[f"broad outbound surface {outbound_count}"] = max(1, outbound_count - 5) * 3.0
        if inbound_count >= 12:
            signals[f"shared by many modules {inbound_count}"] = max(1, inbound_count - 12) * 0.75
        hidden_count = int(hidden_coupling["hidden_coupling_count"])
        if hidden_count:
            signals[f"hidden coupling signals {hidden_count}"] = min(24.0, hidden_count * 8.0)
        explicitness = float(interface["interface_explicitness_ratio"])
        if explicitness < 0.25 and outbound_count + inbound_count >= 4:
            signals[f"low explicit interface {explicitness:.2f}"] = 10.0
        if not has_tests:
            signals["no nearby tests"] = 1.0
        elif not has_inline_tests:
            signals["external tests only"] = 0.5
        if churn >= 400:
            signals[f"high churn {churn}"] = min(18.0, churn / 160.0)
        if contributor_count >= 4:
            signals[f"many contributors {contributor_count}"] = max(1, contributor_count - 3) * 2.0
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
