use crate::util::round2;

#[derive(Clone, Default)]
pub(crate) struct ChangeFacts {
    pub(crate) churn: u64,
    pub(crate) commit_count: usize,
    pub(crate) contributor_count: usize,
    pub(crate) defect_commit_count: usize,
    pub(crate) has_test_evidence: bool,
}

#[derive(Clone, Default)]
pub(crate) struct CorrectnessFacts {
    pub(crate) test_count: usize,
    pub(crate) failed_count: usize,
    pub(crate) unknown_count: usize,
    pub(crate) skipped_count: usize,
    pub(crate) run_failed: bool,
    pub(crate) compile_failed: bool,
    pub(crate) line_coverage_percent: Option<f64>,
}

impl CorrectnessFacts {
    pub(crate) fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "test_count": self.test_count,
            "failed_count": self.failed_count,
            "unknown_count": self.unknown_count,
            "skipped_count": self.skipped_count,
            "run_failed": self.run_failed,
            "compile_failed": self.compile_failed,
            "line_coverage_percent": self.line_coverage_percent,
        })
    }
}

pub(crate) struct ArchitectureRiskInputs {
    pub(crate) is_entrypoint: bool,
    pub(crate) sloc: usize,
    pub(crate) public_api_count: usize,
    pub(crate) outbound_dependencies: usize,
    pub(crate) inbound_dependencies: usize,
    pub(crate) complexity_score: Option<f64>,
    pub(crate) change: Option<ChangeFacts>,
    pub(crate) correctness: Option<CorrectnessFacts>,
    pub(crate) locality_risk: Option<f64>,
    pub(crate) leverage_pressure: Option<f64>,
    pub(crate) layer_violations: usize,
    pub(crate) in_cycle: bool,
}

pub(crate) struct ArchitectureRiskScores {
    pub(crate) maintainability_risk: Option<f64>,
    pub(crate) change_risk: Option<f64>,
    pub(crate) correctness_risk: Option<f64>,
    pub(crate) architectural_risk: f64,
    pub(crate) quality_risk: Option<f64>,
    pub(crate) total_score: Option<f64>,
    pub(crate) unknown_metrics: Vec<String>,
    pub(crate) score_components: serde_json::Value,
}

pub(crate) fn architecture_risk_scores(inputs: ArchitectureRiskInputs) -> ArchitectureRiskScores {
    let scored_outbound_dependencies = if inputs.is_entrypoint {
        inputs.outbound_dependencies.saturating_sub(8)
    } else {
        inputs.outbound_dependencies
    };
    let scored_layer_violations = if inputs.is_entrypoint {
        inputs.layer_violations.saturating_sub(2)
    } else {
        inputs.layer_violations
    };
    let maintainability_risk = inputs.complexity_score.map(|complexity| {
        round2(
            (complexity
                + (inputs.sloc as f64 * 0.12).min(70.0)
                + (inputs.public_api_count as f64 * 2.5).min(30.0)
                + (scored_outbound_dependencies as f64 * 4.0 + inputs.inbound_dependencies as f64)
                    .min(35.0))
            .min(400.0),
        )
    });
    let change_risk = inputs.change.as_ref().map(|change| {
        round2(
            ((change.churn as f64 / 12.0).min(160.0)
                + (change.commit_count as f64 * 2.5).min(100.0)
                + (change.contributor_count as f64 * 14.0).min(80.0)
                + (change.defect_commit_count as f64 * 18.0).min(90.0)
                + if change.has_test_evidence { 0.0 } else { 90.0 })
            .min(520.0),
        )
    });
    let correctness_risk = inputs.correctness.as_ref().map(|correctness| {
        round2(
            (if correctness.failed_count > 0 {
                140.0
            } else {
                0.0
            } + (correctness.failed_count as f64 * 45.0).min(120.0)
                + (correctness.unknown_count as f64 * 4.0).min(80.0)
                + (correctness.skipped_count as f64 * 10.0).min(40.0)
                + if correctness.compile_failed {
                    180.0
                } else if correctness.run_failed && correctness.failed_count == 0 {
                    140.0
                } else {
                    0.0
                }
                + correctness
                    .line_coverage_percent
                    .map(|coverage| ((70.0 - coverage).max(0.0) * 1.5).min(105.0))
                    .unwrap_or(0.0)
                + if correctness.test_count > 0
                    || correctness
                        .line_coverage_percent
                        .is_some_and(|coverage| coverage > 0.0)
                {
                    0.0
                } else {
                    90.0
                })
            .min(470.0),
        )
    });
    let architectural_risk = round2(
        ((scored_outbound_dependencies as f64 * 10.0).min(120.0)
            + (inputs.inbound_dependencies as f64 * 8.0).min(120.0)
            + (scored_layer_violations as f64 * 32.0).min(120.0)
            + if inputs.in_cycle { 110.0 } else { 0.0 }
            + if inputs.sloc >= 250 { 60.0 } else { 0.0 })
        .min(530.0),
    );
    let quality_risk = match (
        maintainability_risk,
        inputs.locality_risk,
        inputs.leverage_pressure,
    ) {
        (Some(maintainability), Some(locality), Some(leverage)) => {
            Some(round2((maintainability + locality + leverage).min(600.0)))
        }
        _ => None,
    };
    let mut unknown_metrics = Vec::new();
    for (name, value) in [
        ("maintainability_risk", maintainability_risk),
        ("change_risk", change_risk),
        ("correctness_risk", correctness_risk),
        ("quality_risk", quality_risk),
    ] {
        if value.is_none() {
            unknown_metrics.push(name.to_string());
        }
    }
    let total_score = match (change_risk, correctness_risk, quality_risk) {
        (Some(change), Some(correctness), Some(quality)) => {
            Some(round2(change + correctness + quality + architectural_risk))
        }
        _ => {
            unknown_metrics.push("total_score".to_string());
            None
        }
    };
    let maintainability_components = inputs.complexity_score.map(|complexity| {
        serde_json::json!([
            component("complexity_score", complexity, complexity),
            component("sloc", inputs.sloc, (inputs.sloc as f64 * 0.12).min(70.0)),
            component(
                "public_api_count",
                inputs.public_api_count,
                (inputs.public_api_count as f64 * 2.5).min(30.0)
            ),
            component(
                "outbound_dependencies",
                scored_outbound_dependencies,
                (scored_outbound_dependencies as f64 * 4.0).min(35.0)
            ),
            component(
                "inbound_dependencies",
                inputs.inbound_dependencies,
                (inputs.inbound_dependencies as f64)
                    .min((35.0 - scored_outbound_dependencies as f64 * 4.0).max(0.0))
            ),
        ])
    });
    let change_components = inputs.change.as_ref().map(|change| {
        serde_json::json!([
            component(
                "churn",
                change.churn,
                (change.churn as f64 / 12.0).min(160.0)
            ),
            component(
                "commit_count",
                change.commit_count,
                (change.commit_count as f64 * 2.5).min(100.0)
            ),
            component(
                "contributor_count",
                change.contributor_count,
                (change.contributor_count as f64 * 14.0).min(80.0)
            ),
            component(
                "defect_commit_count",
                change.defect_commit_count,
                (change.defect_commit_count as f64 * 18.0).min(90.0)
            ),
            component(
                "missing_test_evidence",
                !change.has_test_evidence,
                if change.has_test_evidence { 0.0 } else { 90.0 }
            ),
        ])
    });
    let correctness_components = inputs.correctness.as_ref().map(|correctness| {
        let run_failure = if correctness.compile_failed {
            180.0
        } else if correctness.run_failed && correctness.failed_count == 0 {
            140.0
        } else {
            0.0
        };
        let coverage = correctness
            .line_coverage_percent
            .map(|coverage| ((70.0 - coverage).max(0.0) * 1.5).min(105.0))
            .unwrap_or(0.0);
        let missing_evidence = if correctness.test_count > 0
            || correctness
                .line_coverage_percent
                .is_some_and(|coverage| coverage > 0.0)
        {
            0.0
        } else {
            90.0
        };
        serde_json::json!([
            component(
                "any_failed_tests",
                correctness.failed_count > 0,
                if correctness.failed_count > 0 {
                    140.0
                } else {
                    0.0
                }
            ),
            component(
                "failed_test_count",
                correctness.failed_count,
                (correctness.failed_count as f64 * 45.0).min(120.0)
            ),
            component(
                "unknown_test_count",
                correctness.unknown_count,
                (correctness.unknown_count as f64 * 4.0).min(80.0)
            ),
            component(
                "skipped_test_count",
                correctness.skipped_count,
                (correctness.skipped_count as f64 * 10.0).min(40.0)
            ),
            component("test_run_failure", correctness.run_failed, run_failure),
            component(
                "line_coverage_below_70",
                correctness.line_coverage_percent,
                coverage
            ),
            component(
                "missing_test_evidence",
                missing_evidence > 0.0,
                missing_evidence
            ),
        ])
    });
    let architectural_components = serde_json::json!([
        component(
            "outbound_dependencies",
            scored_outbound_dependencies,
            (scored_outbound_dependencies as f64 * 10.0).min(120.0)
        ),
        component(
            "inbound_dependencies",
            inputs.inbound_dependencies,
            (inputs.inbound_dependencies as f64 * 8.0).min(120.0)
        ),
        component(
            "layer_violations",
            scored_layer_violations,
            (scored_layer_violations as f64 * 32.0).min(120.0)
        ),
        component(
            "cycle_membership",
            inputs.in_cycle,
            if inputs.in_cycle { 110.0 } else { 0.0 }
        ),
        component(
            "large_module",
            inputs.sloc,
            if inputs.sloc >= 250 { 60.0 } else { 0.0 }
        ),
    ]);
    let quality_components = serde_json::json!([
        component(
            "maintainability_risk",
            maintainability_risk,
            maintainability_risk.unwrap_or(0.0)
        ),
        component(
            "locality_risk",
            inputs.locality_risk,
            inputs.locality_risk.unwrap_or(0.0)
        ),
        component(
            "leverage_pressure",
            inputs.leverage_pressure,
            inputs.leverage_pressure.unwrap_or(0.0)
        ),
    ]);
    let total_components = serde_json::json!([
        component("quality_risk", quality_risk, quality_risk.unwrap_or(0.0)),
        component("change_risk", change_risk, change_risk.unwrap_or(0.0)),
        component(
            "correctness_risk",
            correctness_risk,
            correctness_risk.unwrap_or(0.0)
        ),
        component("architectural_risk", architectural_risk, architectural_risk),
    ]);
    let score_components = serde_json::json!({
        "maintainability_risk": maintainability_components,
        "change_risk": change_components,
        "correctness_risk": correctness_components,
        "architectural_risk": architectural_components,
        "quality_risk": quality_components,
        "total_score": total_components,
    });
    ArchitectureRiskScores {
        maintainability_risk,
        change_risk,
        correctness_risk,
        architectural_risk,
        quality_risk,
        total_score,
        unknown_metrics,
        score_components,
    }
}

fn component(signal: &str, raw: impl serde::Serialize, contribution: f64) -> serde_json::Value {
    serde_json::json!({
        "signal": signal,
        "raw": raw,
        "contribution": round2(contribution),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        ArchitectureRiskInputs, ChangeFacts, CorrectnessFacts, architecture_risk_scores, round2,
    };

    #[test]
    fn total_is_unknown_until_required_scores_are_known() {
        let missing = architecture_risk_scores(ArchitectureRiskInputs {
            is_entrypoint: false,
            sloc: 10,
            public_api_count: 1,
            outbound_dependencies: 0,
            inbound_dependencies: 0,
            complexity_score: None,
            change: None,
            correctness: None,
            locality_risk: None,
            leverage_pressure: None,
            layer_violations: 0,
            in_cycle: false,
        });
        assert!(missing.total_score.is_none());
        assert!(
            missing
                .unknown_metrics
                .contains(&"maintainability_risk".to_string())
        );
    }

    #[test]
    fn computes_full_score_when_inputs_are_known() {
        let scored = architecture_risk_scores(ArchitectureRiskInputs {
            is_entrypoint: false,
            sloc: 10,
            public_api_count: 1,
            outbound_dependencies: 1,
            inbound_dependencies: 1,
            complexity_score: Some(10.0),
            change: Some(ChangeFacts {
                churn: 24,
                commit_count: 2,
                contributor_count: 1,
                defect_commit_count: 1,
                has_test_evidence: true,
            }),
            correctness: Some(CorrectnessFacts {
                test_count: 1,
                failed_count: 0,
                unknown_count: 0,
                skipped_count: 0,
                run_failed: false,
                compile_failed: false,
                line_coverage_percent: None,
            }),
            locality_risk: Some(2.0),
            leverage_pressure: Some(3.0),
            layer_violations: 1,
            in_cycle: true,
        });
        assert!(scored.total_score.is_some());
        assert!(scored.unknown_metrics.is_empty());
        assert_eq!(scored.architectural_risk, 160.0);
        let (Some(total), Some(change), Some(correctness), Some(quality)) = (
            scored.total_score,
            scored.change_risk,
            scored.correctness_risk,
            scored.quality_risk,
        ) else {
            panic!("complete inputs should produce complete scores");
        };
        assert_eq!(
            total,
            round2(change + correctness + quality + scored.architectural_risk)
        );
        let explained_total = scored.score_components["total_score"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|component| component["contribution"].as_f64())
            .sum::<f64>();
        assert_eq!(total, round2(explained_total));
    }

    #[test]
    fn computes_total_from_owned_quality_evidence() {
        let scored = architecture_risk_scores(ArchitectureRiskInputs {
            is_entrypoint: false,
            sloc: 10,
            public_api_count: 1,
            outbound_dependencies: 1,
            inbound_dependencies: 1,
            complexity_score: Some(10.0),
            change: Some(ChangeFacts {
                churn: 24,
                commit_count: 2,
                contributor_count: 1,
                defect_commit_count: 1,
                has_test_evidence: true,
            }),
            correctness: Some(CorrectnessFacts {
                test_count: 1,
                failed_count: 0,
                unknown_count: 0,
                skipped_count: 0,
                run_failed: false,
                compile_failed: false,
                line_coverage_percent: None,
            }),
            locality_risk: Some(2.0),
            leverage_pressure: Some(3.0),
            layer_violations: 1,
            in_cycle: true,
        });

        assert!(scored.total_score.is_some());
        assert!(scored.unknown_metrics.is_empty());
    }

    #[test]
    fn entrypoints_get_orchestration_allowance() {
        let normal = architecture_risk_scores(ArchitectureRiskInputs {
            is_entrypoint: false,
            sloc: 20,
            public_api_count: 0,
            outbound_dependencies: 10,
            inbound_dependencies: 0,
            complexity_score: Some(4.0),
            change: None,
            correctness: None,
            locality_risk: None,
            leverage_pressure: None,
            layer_violations: 3,
            in_cycle: false,
        });
        let entrypoint = architecture_risk_scores(ArchitectureRiskInputs {
            is_entrypoint: true,
            sloc: 20,
            public_api_count: 0,
            outbound_dependencies: 10,
            inbound_dependencies: 0,
            complexity_score: Some(4.0),
            change: None,
            correctness: None,
            locality_risk: None,
            leverage_pressure: None,
            layer_violations: 3,
            in_cycle: false,
        });

        assert!(entrypoint.architectural_risk < normal.architectural_risk);
        assert!(entrypoint.maintainability_risk < normal.maintainability_risk);
    }
}
