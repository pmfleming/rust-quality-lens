import argparse
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Dict, List, Optional, Sequence

from common import (
    source_confidence,
    provenance,
    rust_facts_for_paths,
)
from report_modes import add_mode_argument, emit_report
from risk_model import tool_score_metadata, tool_score_weights

DEFAULT_OUTPUT = Path("rust_escape_hatches.json")
VISIBILITY_OUTPUT = Path("target/analysis/rust_escape_hatches.json")


WEIGHTS = tool_score_weights("escape_hatches")

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
    scoring_counts: Dict[str, int]
    locations: List[Dict[str, object]]
    allow_locations: List[Dict[str, object]]
    signals: List[str]
    measured_at: str
    command: str
    host: str
    measurement_confidence: Dict[str, object]
    risk_model_id: str
    risk_model_version: int
    risk_calibration: str
    source: str = "static_rust_escape_hatches"
    mock: bool = False


class RustEscapeHatchAnalyzer:
    def __init__(self, top: Optional[int] = None):
        self.top = top

    def run(self, paths: Sequence[str]) -> List[Dict]:
        facts = rust_facts_for_paths(paths)
        confidence = source_confidence(paths, facts=facts)
        rows = [asdict(self._record_for_fact(fact, confidence)) for fact in facts]
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

    def _record_for_fact(
        self, fact: Dict[str, object], confidence: Dict[str, object]
    ) -> EscapeHatchRecord:
        module_key = str(fact.get("module_key", ""))
        run_provenance = provenance()
        raw_counts = fact.get("escape_counts", {})
        score_metadata = tool_score_metadata("escape_hatches")
        counts: Dict[str, int] = {
            key: int(raw_counts.get(key, 0)) if isinstance(raw_counts, dict) else 0
            for key in WEIGHTS
        }
        locations: List[Dict[str, object]] = []
        scoring_counts = self._scoring_counts(counts)
        score = sum(scoring_counts[key] * weight for key, weight in WEIGHTS.items())

        raw_locations = fact.get("escape_locations", [])
        if isinstance(raw_locations, list):
            for raw_location in raw_locations:
                if not isinstance(raw_location, dict):
                    continue
                key = str(raw_location.get("kind", ""))
                if key not in SIGNAL_LABELS:
                    continue
                locations.append(
                    {
                        "kind": key,
                        "label": SIGNAL_LABELS[key],
                        "line": int(raw_location.get("line", 0)),
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
        allow_attribute_count = lint_suppression_count + clippy_suppression_count
        clippy_allow_count = clippy_suppression_count
        allow_locations = [
            {
                "kind": "allow_attribute",
                "label": "allow/expect attribute",
                "line": item["line"],
                "snippet": "",
            }
            for item in locations
            if item["kind"] in {"lint_suppression", "clippy_suppression"}
        ]
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
            module_name=module_key,
            module_key=module_key,
            path=str(fact.get("path", "")),
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
            scoring_counts=scoring_counts,
            locations=sorted(locations, key=lambda item: (int(item["line"]), str(item["kind"]))),
            allow_locations=allow_locations,
            signals=signals,
            measured_at=run_provenance["measured_at"],
            command=run_provenance["command"],
            host=run_provenance["host"],
            measurement_confidence=confidence,
            risk_model_id=str(score_metadata["risk_model_id"]),
            risk_model_version=int(score_metadata["risk_model_version"]),
            risk_calibration=str(score_metadata["risk_calibration"]),
        )

    @staticmethod
    def _scoring_counts(counts: Dict[str, int]) -> Dict[str, int]:
        scoring_counts = dict(counts)
        if scoring_counts.get("clippy_suppression", 0):
            scoring_counts["lint_suppression"] = 0
        return scoring_counts


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
