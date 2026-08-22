use serde_json::{Value, json};
use std::collections::BTreeMap;

use crate::facts::TypeFact;
use crate::util::{over_free, round2};

pub(crate) fn type_risk(
    ty: &TypeFact,
    method_count: usize,
    impl_block_count: usize,
    impl_file_count: usize,
) -> (f64, Vec<String>, Value) {
    let risk = round2(type_pressure(
        ty,
        method_count,
        impl_block_count,
        impl_file_count,
    ));
    let field_pressure = over_free(ty.field_count as f64, 8.0, 2.5, 35.0);
    let variant_pressure = over_free(ty.variant_count as f64, 8.0, 1.8, 28.0);
    let payload_pressure = over_free(ty.variant_field_count as f64, 10.0, 1.4, 18.0);
    let method_pressure = over_free(method_count as f64, 12.0, 0.85, 32.0);
    let impl_pressure = ((impl_file_count.saturating_sub(2) as f64 * 4.5)
        + (impl_block_count.saturating_sub(4) as f64 * 1.2))
        .min(24.0);
    let declaration_pressure = over_free(ty.declaration_span as f64, 30.0, 0.35, 12.0);
    (
        risk,
        type_signals(ty, method_count, impl_block_count, impl_file_count),
        json!([
            {"signal": "fields_over_8", "raw": ty.field_count, "contribution": round2(field_pressure)},
            {"signal": "variants_over_8", "raw": ty.variant_count, "contribution": round2(variant_pressure)},
            {"signal": "variant_fields_over_10", "raw": ty.variant_field_count, "contribution": round2(payload_pressure)},
            {"signal": "methods_over_12", "raw": method_count, "contribution": round2(method_pressure)},
            {"signal": "impl_spread", "raw": {"files": impl_file_count, "blocks": impl_block_count}, "contribution": round2(impl_pressure)},
            {"signal": "declaration_lines_over_30", "raw": ty.declaration_span, "contribution": round2(declaration_pressure)},
        ]),
    )
}

fn type_pressure(
    ty: &TypeFact,
    method_count: usize,
    impl_block_count: usize,
    impl_file_count: usize,
) -> f64 {
    let impl_spread_pressure = ((impl_file_count.saturating_sub(2) as f64 * 4.5)
        + (impl_block_count.saturating_sub(4) as f64 * 1.2))
        .min(24.0);
    (over_free(ty.field_count as f64, 8.0, 2.5, 35.0)
        + over_free(ty.variant_count as f64, 8.0, 1.8, 28.0)
        + over_free(ty.variant_field_count as f64, 10.0, 1.4, 18.0)
        + over_free(method_count as f64, 12.0, 0.85, 32.0)
        + impl_spread_pressure
        + over_free(ty.declaration_span as f64, 30.0, 0.35, 12.0))
    .clamp(0.0, 100.0)
}

fn type_signals(
    ty: &TypeFact,
    method_count: usize,
    impl_block_count: usize,
    impl_file_count: usize,
) -> Vec<String> {
    let mut signals = Vec::new();
    push_struct_signal(&mut signals, ty);
    if ty.variant_count >= 12 {
        signals.push(format!("large enum {} variants", ty.variant_count));
    }
    if ty.variant_field_count >= 12 {
        signals.push(format!(
            "enum payload surface {} fields",
            ty.variant_field_count
        ));
    }
    if method_count >= 20 {
        signals.push(format!("broad method surface {method_count}"));
    }
    if impl_file_count >= 4 {
        signals.push(format!("impl spread {impl_file_count} files"));
    }
    if impl_block_count >= 6 {
        signals.push(format!("many impl blocks {impl_block_count}"));
    }
    if ty.declaration_span >= 45 {
        signals.push(format!("large declaration {} lines", ty.declaration_span));
    }
    if signals.is_empty() {
        signals.push("stable".to_string());
    }
    signals
}

fn push_struct_signal(signals: &mut Vec<String>, ty: &TypeFact) {
    if ty.kind == "struct" && ty.shape == "tuple" && ty.field_count >= 8 {
        signals.push(format!("wide tuple struct {} fields", ty.field_count));
    } else if ty.kind == "struct" && ty.field_count >= 16 {
        signals.push(format!("wide struct {} fields", ty.field_count));
    }
    if ty.kind == "struct" && ty.shape == "unit" {
        signals.push("unit struct".to_string());
    }
}

const ESCAPE_CALIBRATIONS: &[(&str, &str, f64)] = &[
    ("unsafe_block", "unsafe block", 10.0),
    ("unsafe_fn", "unsafe fn", 10.0),
    ("unsafe_impl", "unsafe impl", 10.0),
    ("unsafe_trait", "unsafe trait", 10.0),
    ("extern_block", "extern block", 8.0),
    ("extern_fn", "extern fn", 7.0),
    ("static_mut", "mutable static", 14.0),
    ("union", "union", 12.0),
    ("raw_borrow", "raw borrow", 6.0),
    ("asm_macro", "inline assembly", 14.0),
    ("transmute", "transmute", 12.0),
    ("maybe_uninit", "MaybeUninit", 5.0),
    ("deref_impl", "Deref impl", 4.0),
    ("deref_mut_impl", "DerefMut impl", 5.0),
    ("glob_import", "glob import", 2.0),
    ("container_ref_return", "container ref return", 3.0),
    ("repr_escape", "layout repr", 5.0),
    ("linkage_escape", "linkage attribute", 8.0),
    ("clippy_suppression", "Clippy suppression", 3.0),
    ("lint_suppression", "lint suppression", 2.0),
];

pub(crate) fn escape_weights() -> BTreeMap<&'static str, f64> {
    ESCAPE_CALIBRATIONS
        .iter()
        .map(|(key, _, weight)| (*key, *weight))
        .collect()
}

pub(crate) fn escape_labels() -> BTreeMap<&'static str, &'static str> {
    let mut labels = ESCAPE_CALIBRATIONS
        .iter()
        .map(|(key, label, _)| (*key, *label))
        .collect::<BTreeMap<_, _>>();
    labels.insert("unsafe_api", "raw-memory API (unscored)");
    labels
}

pub(crate) fn risk_model_weights() -> Value {
    json!({
        "maintainability": {
            "complexity": {"weight": 1.0},
            "sloc": {"weight": 0.12, "cap": 70.0},
            "public_api": {"weight": 2.5, "cap": 30.0},
            "dependencies": {"outbound_weight": 4.0, "inbound_weight": 1.0, "cap": 35.0},
            "entrypoint_outbound_allowance": 8
        },
        "change": {
            "churn": {"divisor": 12.0, "cap": 160.0},
            "commit_count": {"weight": 2.5, "cap": 100.0},
            "contributor_count": {"weight": 14.0, "cap": 80.0},
            "defect_keyword_commits": {"weight": 18.0, "cap": 90.0},
            "missing_test_evidence": 90.0
        },
        "correctness": {
            "any_failed_tests": 140.0,
            "failed_test_count": {"weight": 45.0, "cap": 120.0},
            "unknown_test_count": {"weight": 4.0, "cap": 80.0},
            "skipped_test_count": {"weight": 10.0, "cap": 40.0},
            "test_command_failure_without_failed_test": 140.0,
            "test_compile_failure": 180.0,
            "line_coverage_below_70_percent": {"weight": 1.5, "cap": 105.0},
            "missing_test_evidence": 90.0
        },
        "quality": {
            "maintainability_risk": {"weight": 1.0},
            "locality_risk": {"weight": 1.0},
            "leverage_pressure": {"weight": 1.0},
            "cap": 600.0
        },
        "architectural": {
            "outbound_dependencies": {"weight": 10.0, "cap": 120.0},
            "inbound_dependencies": {"weight": 8.0, "cap": 120.0},
            "layer_violations": {"weight": 32.0, "cap": 120.0},
            "entrypoint_outbound_allowance": 8,
            "entrypoint_layer_violation_allowance": 2,
            "cycle_membership_penalty": 110.0,
            "large_module": {"threshold_sloc": 250, "penalty": 60.0}
        }
    })
}

pub(crate) fn risk_model_tool_scores() -> Value {
    json!({
        "escape_hatches": {"weights": escape_weights()},
        "type_health": {"score_cap": 100.0},
        "locality": {"score_cap": 100.0},
        "leverage": {"score_cap": 100.0},
        "hotspots": {
            "function": {
                "branch_pressure_weight": 3.7,
                "path_pressure_over_1_weight": 2.0,
                "nesting_over_2_weight": 3.0,
                "lines_over_20_weight": 0.25,
                "quality_multiplier": 1.12,
                "empirical_bands": {"watch": 19.15, "high": 32.98, "extreme": 80.08}
            },
            "module": {
                "max_function_weight": 0.55,
                "p95_function_weight": 0.25,
                "mean_function_weight": 0.10,
                "size_pressure_weight": 0.50,
                "empirical_bands": {"watch": 67.30, "high": 95.51, "extreme": 139.94}
            }
        },
        "clones_token": {"window_tokens": 50, "minimum_line_span": 5, "instance_weight": 5.0},
        "clones_ast": {"minimum_ast_nodes": 6, "cross_file_factor": 1.5},
        "clones_module_responsibility": {"includes_target_kind": true, "minimum_responsibility_lines": 8},
        "clones_test_ast": {"minimum_line_span": 5}
    })
}

pub(crate) fn risk_model_classification() -> Value {
    json!({
        "basis": "four_project_semantic_identity_percentiles_2026-07-13",
        "watch_total_score_p90": 712.99,
        "high_total_score_p95": 838.64,
        "extreme_total_score_p99": 1028.20,
        "warn_total_score": 712.99,
        "bad_total_score": 838.64,
        "warn_color_score": 712.99,
        "bad_color_score": 838.64
    })
}
