//! Cross-lens capability contract v1; execution/completeness is not a quality verdict.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CapabilityStatus {
    Completed,
    Partial,
    Unavailable,
    Disabled,
    NotApplicable,
    FailedExecution,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct CapabilityEvidence {
    schema_version: u64,
    capability: String,
    scope: String,
    status: CapabilityStatus,
    evaluated: u64,
    skipped: u64,
    reasons: Vec<String>,
}

impl CapabilityEvidence {
    pub(crate) fn inputs(scope: &str, complete: bool) -> Self {
        Self {
            schema_version: 1,
            capability: "project-inputs".into(),
            scope: scope.into(),
            status: if complete {
                CapabilityStatus::Completed
            } else {
                CapabilityStatus::Partial
            },
            evaluated: u64::from(complete),
            skipped: u64::from(!complete),
            reasons: if complete {
                vec![]
            } else {
                vec!["See measurement_confidence for incomplete inputs.".into()]
            },
        }
    }

    pub(crate) fn validate(&self) -> bool {
        self.schema_version == 1
            && !self.capability.trim().is_empty()
            && !self.scope.trim().is_empty()
            && self.evaluated <= 9_007_199_254_740_991
            && self.skipped <= 9_007_199_254_740_991
            && self.reasons.iter().all(|reason| !reason.trim().is_empty())
            && (if self.status == CapabilityStatus::Completed {
                self.skipped == 0 && self.reasons.is_empty()
            } else {
                !self.reasons.is_empty()
            })
            && (matches!(
                self.status,
                CapabilityStatus::Completed | CapabilityStatus::Partial
            ) || self.evaluated == 0)
    }
}

#[cfg(test)]
mod tests {
    use super::CapabilityEvidence;
    use serde::Deserialize;
    #[derive(Deserialize)]
    struct Case {
        valid: bool,
        value: serde_json::Value,
    }
    #[derive(Deserialize)]
    struct Corpus {
        cases: Vec<Case>,
    }
    #[test]
    fn cross_lens_conformance() -> anyhow::Result<()> {
        let corpus: Corpus =
            serde_json::from_str(include_str!("../contracts/capability-cases.json"))?;
        for case in corpus.cases {
            let valid = serde_json::from_value::<CapabilityEvidence>(case.value.clone())
                .is_ok_and(|item| item.validate());
            assert_eq!(valid, case.valid, "{}", case.value);
        }
        Ok(())
    }
}
