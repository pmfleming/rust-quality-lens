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
}

impl CorrectnessFacts {
    pub(crate) fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "test_count": self.test_count,
            "failed_count": self.failed_count,
            "unknown_count": self.unknown_count,
            "skipped_count": self.skipped_count,
        })
    }
}

#[derive(Clone, Default)]
pub(crate) struct PerformanceFacts {
    pub(crate) benchmark_score: f64,
    pub(crate) mean_ms: f64,
    pub(crate) variance: f64,
}

pub(crate) struct ArchitectureRiskInputs {
    pub(crate) sloc: usize,
    pub(crate) public_api_count: usize,
    pub(crate) outbound_dependencies: usize,
    pub(crate) inbound_dependencies: usize,
    pub(crate) complexity_score: Option<f64>,
    pub(crate) change: Option<ChangeFacts>,
    pub(crate) correctness: Option<CorrectnessFacts>,
    pub(crate) locality_risk: Option<f64>,
    pub(crate) leverage_pressure: Option<f64>,
    pub(crate) performance: Option<PerformanceFacts>,
    pub(crate) layer_violations: usize,
    pub(crate) in_cycle: bool,
}

pub(crate) struct ArchitectureRiskScores {
    pub(crate) maintainability_risk: Option<f64>,
    pub(crate) change_risk: Option<f64>,
    pub(crate) performance_risk: Option<f64>,
    pub(crate) correctness_risk: Option<f64>,
    pub(crate) architectural_risk: f64,
    pub(crate) quality_risk: Option<f64>,
    pub(crate) total_score: Option<f64>,
    pub(crate) unknown_metrics: Vec<String>,
}

pub(crate) fn architecture_risk_scores(inputs: ArchitectureRiskInputs) -> ArchitectureRiskScores {
    let maintainability_risk = inputs.complexity_score.map(|complexity| {
        round2(
            (complexity
                + (inputs.sloc as f64 * 0.12).min(70.0)
                + (inputs.public_api_count as f64 * 2.5).min(30.0)
                + (inputs.outbound_dependencies as f64 * 4.0 + inputs.inbound_dependencies as f64)
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
                + if correctness.test_count > 0 {
                    0.0
                } else {
                    90.0
                })
            .min(470.0),
        )
    });
    let performance_risk = inputs.performance.as_ref().map(|performance| {
        round2(
            (performance.benchmark_score
                + (performance.mean_ms * 2.5).min(120.0)
                + (performance.variance * 180.0).min(90.0))
            .min(300.0),
        )
    });
    let architectural_risk = round2(
        ((inputs.outbound_dependencies as f64 * 10.0).min(120.0)
            + (inputs.inbound_dependencies as f64 * 8.0).min(120.0)
            + (inputs.layer_violations as f64 * 32.0).min(120.0)
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
        ("performance_risk", performance_risk),
        ("correctness_risk", correctness_risk),
        ("quality_risk", quality_risk),
    ] {
        if value.is_none() {
            unknown_metrics.push(name.to_string());
        }
    }
    let total_score = match (
        maintainability_risk,
        change_risk,
        performance_risk,
        correctness_risk,
        quality_risk,
    ) {
        (
            Some(maintainability),
            Some(change),
            Some(performance),
            Some(correctness),
            Some(quality),
        ) => Some(round2(
            maintainability + change + performance + correctness + quality + architectural_risk,
        )),
        _ => {
            unknown_metrics.push("total_score".to_string());
            None
        }
    };
    ArchitectureRiskScores {
        maintainability_risk,
        change_risk,
        performance_risk,
        correctness_risk,
        architectural_risk,
        quality_risk,
        total_score,
        unknown_metrics,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ArchitectureRiskInputs, ChangeFacts, CorrectnessFacts, PerformanceFacts,
        architecture_risk_scores,
    };

    #[test]
    fn total_is_unknown_until_required_scores_are_known() {
        let missing = architecture_risk_scores(ArchitectureRiskInputs {
            sloc: 10,
            public_api_count: 1,
            outbound_dependencies: 0,
            inbound_dependencies: 0,
            complexity_score: None,
            change: None,
            correctness: None,
            locality_risk: None,
            leverage_pressure: None,
            performance: None,
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
            }),
            locality_risk: Some(2.0),
            leverage_pressure: Some(3.0),
            performance: Some(PerformanceFacts {
                benchmark_score: 1.0,
                mean_ms: 2.0,
                variance: 0.1,
            }),
            layer_violations: 1,
            in_cycle: true,
        });
        assert!(scored.total_score.is_some());
        assert!(scored.unknown_metrics.is_empty());
        assert_eq!(scored.architectural_risk, 160.0);
    }
}
