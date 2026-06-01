from __future__ import annotations

from typing import Any, Dict, Optional


RISK_MODEL: Dict[str, Any] = {
    "id": "rqlens.architecture_risk",
    "version": 1,
    "raw_fact_contract": {
        "description": "Raw facts are preserved in module evidence and input artifacts. Risk fields are derived from this model.",
        "map_inputs": [
            "dependency graph",
            "public API counts",
            "hotspots.json",
            "correctness_review.json",
            "locality_metrics.json",
            "leverage_metrics.json",
            "git history",
            "optional slowspots.json",
        ],
    },
    "categories": {
        "maintainability": {
            "complexity": {"weight": 1.0},
            "sloc": {"weight": 0.12, "cap": 70.0},
            "public_api": {"weight": 2.5, "cap": 30.0},
            "dependencies": {
                "outbound_weight": 4.0,
                "inbound_weight": 1.0,
                "cap": 35.0,
            },
        },
        "change": {
            "churn": {"divisor": 12.0, "cap": 160.0},
            "commit_count": {"weight": 2.5, "cap": 100.0},
            "contributors": {"weight": 14.0, "cap": 80.0},
            "defect_commits": {"weight": 18.0, "cap": 90.0},
            "missing_test_evidence": 90.0,
        },
        "correctness": {
            "any_failed_tests": 140.0,
            "failed_tests": {"weight": 45.0, "cap": 120.0},
            "unknown_tests": {"weight": 4.0, "cap": 80.0},
            "skipped_tests": {"weight": 10.0, "cap": 40.0},
            "missing_test_evidence": 90.0,
        },
        "performance": {
            "perf_score": {"weight": 1.0},
            "mean_ms": {"weight": 2.5, "cap": 120.0},
            "variance": {"weight": 180.0, "cap": 90.0},
        },
        "architectural": {
            "outbound_dependencies": {"weight": 10.0, "cap": 120.0},
            "inbound_dependencies": {"weight": 8.0, "cap": 120.0},
            "layer_violations": {"weight": 32.0, "cap": 120.0},
            "cycle_member": 110.0,
            "large_module_sloc_threshold": 250,
            "large_module_penalty": 60.0,
        },
    },
    "classification": {
        "warn_total_score": 300.0,
        "bad_total_score": 600.0,
        "warn_color_score": 350.0,
        "bad_color_score": 700.0,
    },
    "tool_scores": {
        "escape_hatches": {
            "weights": {
                "unsafe_block": 10.0,
                "unsafe_fn": 10.0,
                "unsafe_impl": 10.0,
                "unsafe_trait": 10.0,
                "extern_block": 8.0,
                "extern_fn": 7.0,
                "static_mut": 14.0,
                "union": 12.0,
                "raw_borrow": 6.0,
                "asm_macro": 14.0,
                "transmute": 12.0,
                "maybe_uninit": 5.0,
                "deref_impl": 4.0,
                "deref_mut_impl": 5.0,
                "glob_import": 2.0,
                "container_ref_return": 3.0,
                "repr_escape": 5.0,
                "linkage_escape": 8.0,
                "clippy_suppression": 3.0,
                "lint_suppression": 2.0,
            },
            "dedupe": {
                "clippy_suppression_zeroes": "lint_suppression",
            },
        },
        "type_health": {
            "score_cap": 100.0,
            "field_pressure": {"free": 8, "weight": 2.5, "cap": 35.0},
            "variant_pressure": {"free": 8, "weight": 1.8, "cap": 28.0},
            "variant_payload_pressure": {"free": 10, "weight": 1.4, "cap": 18.0},
            "method_pressure": {"free": 12, "weight": 0.85, "cap": 32.0},
            "impl_spread_pressure": {
                "file_free": 2,
                "file_weight": 4.5,
                "block_free": 4,
                "block_weight": 1.2,
                "cap": 24.0,
            },
            "declaration_pressure": {"free": 30, "weight": 0.35, "cap": 12.0},
            "signals": {
                "tuple_struct_fields": 8,
                "wide_struct_fields": 16,
                "large_enum_variants": 12,
                "enum_payload_fields": 12,
                "broad_methods": 20,
                "impl_files": 4,
                "impl_blocks": 6,
                "large_declaration_lines": 45,
            },
        },
        "locality": {
            "score_cap": 100.0,
            "dependency_spread": {
                "far_dependency_weight": 9.0,
                "layer_violation_weight": 16.0,
                "outbound_free": 5,
                "outbound_weight": 3.0,
                "inbound_free": 12,
                "inbound_weight": 0.75,
                "cap": 48.0,
            },
            "hidden_coupling": {"weight": 8.0, "cap": 24.0},
            "interface_penalty": {
                "explicitness_threshold": 0.25,
                "coupling_threshold": 4,
                "penalty": 10.0,
            },
            "test_distance": {
                "external_only": 0.5,
                "missing": 1.0,
            },
            "change_spread": {
                "churn_divisor": 160.0,
                "contributor_free": 3,
                "contributor_weight": 2.0,
                "cap": 18.0,
            },
            "signals": {
                "high_churn": 400,
                "many_contributors": 4,
            },
        },
        "leverage": {
            "score_cap": 100.0,
            "base_score": 68.0,
            "pressure_scale": {
                "base": 0.35,
                "cap": 0.65,
                "reach_full_scale": 6.0,
            },
            "reach_score": {
                "reach_weight": 2.5,
                "caller_area_weight": 4.0,
                "cap": 22.0,
            },
            "invariant_score": {
                "public_type_weight": 3.0,
                "ratio_weight": 8.0,
                "cap": 18.0,
            },
            "leaf_fit_bonus": 14.0,
            "ripple_penalty": {
                "avg_cochanged_free": 2.0,
                "avg_cochanged_weight": 1.1,
                "cochanged_free": 12,
                "cochanged_weight": 0.35,
                "cap": 24.0,
            },
            "divergence_penalty": {"weight": 9.0, "cap": 28.0},
            "unsafe_penalty": {"weight": 4.0, "cap": 20.0},
            "surface_penalty": {
                "reach_threshold": 3,
                "public_function_threshold": 6,
                "penalty": 8.0,
            },
        },
        "hotspots": {
            "quality_multiplier": 1.12,
            "cognitive": {"weight": 3.7, "cap": 260.0, "signal_threshold": 8},
            "cyclomatic": {"weight": 2.0, "cap": 220.0, "signal_threshold": 12},
            "maintainability": {
                "baseline": 65.0,
                "weight": 1.2,
                "cap": 150.0,
                "signal_threshold": 40.0,
            },
            "effort": {
                "weight": 4.0,
                "cap": 60.0,
                "signal_threshold": 15000,
            },
            "size": {"sloc_divisor": 10.0, "cap": 20.0, "signal_threshold": 150},
        },
    },
}


def model_id() -> str:
    return str(RISK_MODEL["id"])


def model_version() -> int:
    return int(RISK_MODEL["version"])


def model_weights() -> Dict[str, Any]:
    return RISK_MODEL["categories"]


def model_classification() -> Dict[str, float]:
    return RISK_MODEL["classification"]


def tool_calibration(tool: str) -> Dict[str, Any]:
    return RISK_MODEL["tool_scores"][tool]


def tool_score_weights(tool: str) -> Dict[str, float]:
    weights = tool_calibration(tool).get("weights", {})
    return {str(key): float(value) for key, value in weights.items()}


def tool_score_metadata(tool: str) -> Dict[str, object]:
    return {
        "risk_model_id": model_id(),
        "risk_model_version": model_version(),
        "risk_calibration": tool,
    }


def capped(value: float, cap: float) -> float:
    return min(float(cap), float(value))


def over_free(value: float, *, free: float, weight: float, cap: float) -> float:
    return capped(max(0.0, float(value) - float(free)) * float(weight), cap)


def bounded_score(value: float, *, cap: float = 100.0) -> float:
    return max(0.0, min(float(cap), float(value)))


def inverse_risk(score: float, *, cap: float = 100.0) -> float:
    return bounded_score(float(cap) - float(score), cap=cap)


def architecture_risk_scores(values: Dict[str, Any]) -> Dict[str, Optional[float]]:
    unknown_categories = values["unknown_categories"]
    weights = model_weights()
    maintainability = None
    if "maintainability" not in unknown_categories:
        maintainability_weights = weights["maintainability"]
        dependency_weights = maintainability_weights["dependencies"]
        maintainability = round(
            values["complexity"] * maintainability_weights["complexity"]["weight"]
            + capped(
                values["sloc"] * maintainability_weights["sloc"]["weight"],
                maintainability_weights["sloc"]["cap"],
            )
            + capped(
                values["public_api"] * maintainability_weights["public_api"]["weight"],
                maintainability_weights["public_api"]["cap"],
            )
            + capped(
                values["outbound"] * dependency_weights["outbound_weight"]
                + values["inbound"] * dependency_weights["inbound_weight"],
                dependency_weights["cap"],
            ),
            2,
        )

    change = None
    if "change" not in unknown_categories:
        change_weights = weights["change"]
        change = round(
            capped(values["churn"] / change_weights["churn"]["divisor"], change_weights["churn"]["cap"])
            + capped(
                values["commit_count"] * change_weights["commit_count"]["weight"],
                change_weights["commit_count"]["cap"],
            )
            + capped(
                values["contributors"] * change_weights["contributors"]["weight"],
                change_weights["contributors"]["cap"],
            )
            + capped(
                values["defect_commits"] * change_weights["defect_commits"]["weight"],
                change_weights["defect_commits"]["cap"],
            )
            + (change_weights["missing_test_evidence"] if not values["has_correctness_tests"] else 0.0),
            2,
        )

    correctness = None
    if "correctness" not in unknown_categories:
        correctness_weights = weights["correctness"]
        correctness = round(
            (correctness_weights["any_failed_tests"] if values["failed_tests"] else 0.0)
            + capped(
                values["failed_tests"] * correctness_weights["failed_tests"]["weight"],
                correctness_weights["failed_tests"]["cap"],
            )
            + capped(
                values["unknown_tests"] * correctness_weights["unknown_tests"]["weight"],
                correctness_weights["unknown_tests"]["cap"],
            )
            + capped(
                values["skipped_tests"] * correctness_weights["skipped_tests"]["weight"],
                correctness_weights["skipped_tests"]["cap"],
            )
            + (correctness_weights["missing_test_evidence"] if not values["has_correctness_tests"] else 0.0),
            2,
        )

    performance_weights = weights["performance"]
    performance = round(
        values["perf_score"] * performance_weights["perf_score"]["weight"]
        + capped(
            values["perf_mean_ms"] * performance_weights["mean_ms"]["weight"],
            performance_weights["mean_ms"]["cap"],
        )
        + capped(
            values["perf_variance"] * performance_weights["variance"]["weight"],
            performance_weights["variance"]["cap"],
        ),
        2,
    )

    architectural_weights = weights["architectural"]
    architectural = round(
        capped(
            values["outbound"] * architectural_weights["outbound_dependencies"]["weight"],
            architectural_weights["outbound_dependencies"]["cap"],
        )
        + capped(
            values["inbound"] * architectural_weights["inbound_dependencies"]["weight"],
            architectural_weights["inbound_dependencies"]["cap"],
        )
        + capped(
            values["layer_violations"] * architectural_weights["layer_violations"]["weight"],
            architectural_weights["layer_violations"]["cap"],
        )
        + (architectural_weights["cycle_member"] if values["cycle_member"] else 0.0)
        + (
            architectural_weights["large_module_penalty"]
            if values["sloc"] is not None
            and values["sloc"] >= architectural_weights["large_module_sloc_threshold"]
            else 0.0
        ),
        2,
    )

    category_scores = [maintainability, change, performance, architectural, correctness]
    total_score = None if any(score is None for score in category_scores) else round(
        sum(float(score) for score in category_scores),
        2,
    )
    return {
        "maintainability_risk": maintainability,
        "change_risk": change,
        "performance_risk": performance,
        "correctness_risk": correctness,
        "quality_risk": maintainability,
        "architectural_risk": architectural,
        "total_score": total_score,
    }
