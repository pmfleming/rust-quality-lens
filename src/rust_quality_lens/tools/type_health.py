import argparse
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Dict, List, Optional, Sequence, Set, Tuple

from common import (
    source_confidence,
    provenance,
    rust_facts_for_paths,
)
from report_modes import add_mode_argument, emit_report
from risk_model import bounded_score, inverse_risk, over_free, tool_calibration, tool_score_metadata

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
    shape: str = ""
    field_count: int = 0
    variant_count: int = 0
    variant_field_count: int = 0
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
    measurement_confidence: Dict[str, object] = field(default_factory=dict)
    risk_model_id: str = ""
    risk_model_version: int = 0
    risk_calibration: str = ""
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

        facts = rust_facts_for_paths(paths)
        confidence = source_confidence(paths, facts=facts)
        for fact in facts:
            if fact.get("parse_status") != "ok":
                continue
            for raw_record in fact.get("types", []):
                if not isinstance(raw_record, dict):
                    continue
                declarations.append(
                    TypeRecord(
                        type_name=str(raw_record.get("type_name", "")),
                        qualified_name=str(raw_record.get("qualified_name", "")),
                        module_key=str(raw_record.get("module_key", "")),
                        path=str(raw_record.get("path", "")),
                        line=int(raw_record.get("line", 0)),
                        kind=str(raw_record.get("kind", "")),
                        shape=str(raw_record.get("shape", "")),
                        field_count=int(raw_record.get("field_count", 0)),
                        variant_count=int(raw_record.get("variant_count", 0)),
                        variant_field_count=int(raw_record.get("variant_field_count", 0)),
                        declaration_span=int(raw_record.get("declaration_span", 0)),
                    )
                )

            for raw_impl in fact.get("impls", []):
                if not isinstance(raw_impl, dict):
                    continue
                key = (str(raw_impl.get("module_key", "")), str(raw_impl.get("type_name", "")))
                stats = impls.setdefault(key, ImplStats())
                stats.impl_block_count += 1
                stats.method_count += int(raw_impl.get("method_count", 0))
                stats.impl_files.add(str(raw_impl.get("path", "")))

        rows: List[Dict] = []
        run_provenance = provenance()
        score_metadata = tool_score_metadata("type_health")

        for record in declarations:
            stats = impls.get((record.module_key, record.type_name), ImplStats())
            record.method_count = stats.method_count
            record.impl_block_count = stats.impl_block_count
            record.impl_files = sorted(stats.impl_files)
            record.impl_file_count = len(record.impl_files)
            record.structural_risk, record.signals = self._risk(record)
            record.structural_score = round(
                inverse_risk(record.structural_risk, cap=tool_calibration("type_health")["score_cap"]),
                2,
            )
            record.measured_at = run_provenance["measured_at"]
            record.command = run_provenance["command"]
            record.host = run_provenance["host"]
            record.measurement_confidence = confidence
            record.risk_model_id = str(score_metadata["risk_model_id"])
            record.risk_model_version = int(score_metadata["risk_model_version"])
            record.risk_calibration = str(score_metadata["risk_calibration"])
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

    def _risk(self, record: TypeRecord) -> Tuple[float, List[str]]:
        calibration = tool_calibration("type_health")
        field_pressure = over_free(record.field_count, **calibration["field_pressure"])
        variant_pressure = over_free(record.variant_count, **calibration["variant_pressure"])
        variant_payload_pressure = over_free(
            record.variant_field_count,
            **calibration["variant_payload_pressure"],
        )
        method_pressure = over_free(record.method_count, **calibration["method_pressure"])
        impl_calibration = calibration["impl_spread_pressure"]
        impl_spread_pressure = min(
            impl_calibration["cap"],
            max(0, record.impl_file_count - impl_calibration["file_free"])
            * impl_calibration["file_weight"]
            + max(0, record.impl_block_count - impl_calibration["block_free"])
            * impl_calibration["block_weight"],
        )
        declaration_pressure = over_free(
            record.declaration_span,
            **calibration["declaration_pressure"],
        )
        risk = bounded_score(
            field_pressure
            + variant_pressure
            + variant_payload_pressure
            + method_pressure
            + impl_spread_pressure
            + declaration_pressure,
            cap=calibration["score_cap"],
        )
        signal_thresholds = calibration["signals"]
        signals = []
        if (
            record.kind == "struct"
            and record.shape == "tuple"
            and record.field_count >= signal_thresholds["tuple_struct_fields"]
        ):
            signals.append(f"wide tuple struct {record.field_count} fields")
        elif (
            record.kind == "struct"
            and record.field_count >= signal_thresholds["wide_struct_fields"]
        ):
            signals.append(f"wide struct {record.field_count} fields")
        if record.kind == "struct" and record.shape == "unit":
            signals.append("unit struct")
        if record.variant_count >= signal_thresholds["large_enum_variants"]:
            signals.append(f"large enum {record.variant_count} variants")
        if record.variant_field_count >= signal_thresholds["enum_payload_fields"]:
            signals.append(f"enum payload surface {record.variant_field_count} fields")
        if record.method_count >= signal_thresholds["broad_methods"]:
            signals.append(f"broad method surface {record.method_count}")
        if record.impl_file_count >= signal_thresholds["impl_files"]:
            signals.append(f"impl spread {record.impl_file_count} files")
        if record.impl_block_count >= signal_thresholds["impl_blocks"]:
            signals.append(f"many impl blocks {record.impl_block_count}")
        if record.declaration_span >= signal_thresholds["large_declaration_lines"]:
            signals.append(f"large declaration {record.declaration_span} lines")
        return round(risk, 2), signals or ["stable"]


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
