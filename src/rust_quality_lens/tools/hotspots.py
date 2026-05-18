import argparse
import json
import math
import os
import shutil
import subprocess
import sys
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Dict, List, Optional

from report_modes import add_mode_argument, emit_report

DEFAULT_OUTPUT = Path("hotspots.json")
VISIBILITY_OUTPUT = Path("target/analysis/hotspots.json")


@dataclass
class CodeMetrics:
    name: str
    kind: str
    start_line: int
    end_line: int
    cognitive: float
    cyclomatic: float
    mi: float
    effort: float
    sloc: float
    ploc: float
    cloc: float
    bugs: float
    abc_mag: float
    nom_fn: int
    nom_cl: int
    quality_score: float = 0.0
    cognitive_score: float = 0.0
    cyclomatic_score: float = 0.0
    maintainability_score: float = 0.0
    effort_score: float = 0.0
    size_score: float = 0.0
    score: float = 0.0
    signals: str = ""
    abc_density: float = 0.0
    complexity_density: float = 0.0

    @property
    def short_name(self) -> str:
        if self.kind == "unit":
            return os.path.basename(self.name)
        if "::" in self.name:
            return self.name.split("::")[-1]
        return os.path.basename(self.name) if os.sep in self.name else self.name


class HotspotAnalyzer:
    def __init__(self, top: Optional[int], scope: str, include_anonymous: bool):
        self.top = top
        self.scope = scope
        self.include_anonymous = include_anonymous

    def _extract_metric(
        self, metrics: Dict, group: str, name: str, default: float = 0.0
    ) -> float:
        return float(metrics.get(group, {}).get(name, default))

    def process_node(self, node: Dict) -> CodeMetrics:
        m = node.get("metrics", {})
        metrics = CodeMetrics(
            name=node.get("name", "unknown"),
            kind=node.get("kind", "unknown"),
            start_line=node.get("start_line", 0),
            end_line=node.get("end_line", 0),
            cognitive=self._extract_metric(m, "cognitive", "sum"),
            cyclomatic=self._extract_metric(m, "cyclomatic", "sum"),
            mi=self._extract_metric(m, "mi", "mi_visual_studio", 100.0),
            effort=self._extract_metric(m, "halstead", "effort"),
            sloc=self._extract_metric(m, "loc", "sloc"),
            ploc=self._extract_metric(m, "loc", "ploc"),
            cloc=self._extract_metric(m, "loc", "cloc"),
            bugs=self._extract_metric(m, "halstead", "bugs"),
            abc_mag=self._extract_metric(m, "abc", "magnitude"),
            nom_fn=int(self._extract_metric(m, "nom", "functions")),
            nom_cl=int(self._extract_metric(m, "nom", "closures")),
        )
        components = self.calculate_quality_components(metrics)
        metrics.cognitive_score = components["cognitive_score"]
        metrics.cyclomatic_score = components["cyclomatic_score"]
        metrics.maintainability_score = components["maintainability_score"]
        metrics.effort_score = components["effort_score"]
        metrics.quality_score = self.calculate_quality_score(metrics)
        metrics.size_score = self.calculate_size_score(metrics)
        metrics.score = metrics.quality_score
        metrics.signals = self.generate_signals(metrics)

        if metrics.sloc > 0:
            metrics.abc_density = metrics.abc_mag / metrics.sloc
            metrics.complexity_density = metrics.quality_score / metrics.sloc

        return metrics

    def calculate_quality_score(self, m: CodeMetrics) -> float:
        components = self.calculate_quality_components(m)
        score = sum(components.values()) * 1.12
        return round(score, 2)

    def calculate_quality_components(self, m: CodeMetrics) -> Dict[str, float]:
        cognitive = min(260.0, m.cognitive * 3.7)
        cyclomatic = min(220.0, m.cyclomatic * 2.0)
        maintainability = min(150.0, max(0.0, 65.0 - m.mi) * 1.2)
        effort = min(60.0, math.log1p(max(0.0, m.effort)) * 4.0)
        return {
            "cognitive_score": round(cognitive, 2),
            "cyclomatic_score": round(cyclomatic, 2),
            "maintainability_score": round(maintainability, 2),
            "effort_score": round(effort, 2),
        }

    def calculate_size_score(self, m: CodeMetrics) -> float:
        return round(min(20.0, m.sloc / 10.0), 2)

    def generate_signals(self, m: CodeMetrics) -> str:
        signals = []
        if m.cognitive >= 8:
            signals.append(f"high cognitive={m.cognitive}")
        if m.cyclomatic >= 12:
            signals.append(f"high cyclomatic={m.cyclomatic}")
        if m.mi < 40:
            signals.append(f"low MI={m.mi:.1f}")
        if m.effort >= 15000:
            signals.append(f"high effort={m.effort:,.0f}")
        if m.sloc >= 150:
            signals.append(f"large size={m.sloc} sloc")
        return ", ".join(signals) if signals else "stable"

    def flatten(self, node: Dict, results: List[CodeMetrics]) -> None:
        results.append(self.process_node(node))
        for child in node.get("spaces", []):
            self.flatten(child, results)

    def run(self, paths: List[str]) -> List[CodeMetrics]:
        if not shutil.which("rust-code-analysis-cli"):
            print("Error: 'rust-code-analysis-cli' not found in PATH.", file=sys.stderr)
            sys.exit(1)

        all_data = []
        for path in paths:
            all_data.extend(self._run_single_path(path))

        scope_map = {"files": ["unit"], "functions": ["function"]}
        target_kinds = scope_map.get(self.scope, ["unit", "function"])
        filtered = [m for m in all_data if m.kind in target_kinds]
        if not self.include_anonymous:
            filtered = [m for m in filtered if m.name != "<anonymous>"]

        ranked = sorted(filtered, key=lambda item: (-item.score, item.name))
        if self.top is not None:
            return ranked[: self.top]
        return ranked

    def _run_single_path(self, path: str) -> List[CodeMetrics]:
        cmd = [
            "rust-code-analysis-cli",
            "--metrics",
            "--paths",
            path,
            "--output-format",
            "json",
        ]
        try:
            result = subprocess.run(cmd, capture_output=True, text=True, check=True)
        except Exception as exc:
            print(f"Analysis failed for {path}: {exc}", file=sys.stderr)
            sys.exit(1)

        results: List[CodeMetrics] = []
        for line in result.stdout.splitlines():
            if not line.strip():
                continue
            clean_line = line.replace('"N1":', '"halstead_N1":').replace(
                '"N2":', '"halstead_N2":'
            )
            try:
                self.flatten(json.loads(clean_line), results)
            except json.JSONDecodeError:
                print(
                    f"Warning: Skipping malformed CLI output line: {clean_line[:50]}...",
                    file=sys.stderr,
                )
        return results


def render_cli(payload: object) -> str:
    rows = payload if isinstance(payload, list) else []
    lines = ["Hotspots"]
    for index, item in enumerate(rows[:10], start=1):
        lines.append(
            f"{index:>2}. {item['name']} | score={item['score']:.2f} | cognitive={item['cognitive']:.1f} | cyclomatic={item['cyclomatic']:.1f}"
        )
    if not rows:
        lines.append("No hotspots found.")
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Emit Rust complexity hotspot metrics as JSON"
    )
    parser.add_argument("--paths", nargs="+", default=["src"], help="Paths to analyze")
    parser.add_argument(
        "--top",
        type=int,
        default=None,
        help="Limit the number of ranked hotspot records. Defaults to all records.",
    )
    parser.add_argument(
        "--scope",
        choices=["all", "files", "functions"],
        default="all",
        help="Metric scope to include in the JSON output",
    )
    parser.add_argument(
        "--include-anonymous", action="store_true", help="Include anonymous functions"
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help=f"Optional output JSON path. Example: {DEFAULT_OUTPUT}",
    )
    add_mode_argument(parser)

    args = parser.parse_args()
    analyzer = HotspotAnalyzer(
        top=args.top, scope=args.scope, include_anonymous=args.include_anonymous
    )
    payload = [asdict(metric) for metric in analyzer.run(args.paths)]
    emit_report(
        payload,
        mode=args.mode,
        output_path=args.output,
        visibility_path=VISIBILITY_OUTPUT,
        cli_renderer=render_cli,
        label="hotspot",
    )


if __name__ == "__main__":
    main()
