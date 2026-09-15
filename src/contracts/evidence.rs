//! Typed boundaries for evidence consumed by policy. JSON remains the public
//! wire format; missing fields and invalid states must not become empty evidence.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConfidenceState {
    Complete,
    Partial,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(try_from = "ConfidenceWire", into = "ConfidenceWire")]
pub(crate) struct MeasurementConfidence {
    state: ConfidenceState,
    details: Map<String, Value>,
}

#[derive(Deserialize, Serialize)]
struct ConfidenceWire {
    complete: bool,
    partial: bool,
    #[serde(flatten)]
    details: Map<String, Value>,
}

impl TryFrom<ConfidenceWire> for MeasurementConfidence {
    type Error = &'static str;

    fn try_from(wire: ConfidenceWire) -> std::result::Result<Self, Self::Error> {
        let state = match (wire.complete, wire.partial) {
            (true, false) => ConfidenceState::Complete,
            (false, true) => ConfidenceState::Partial,
            _ => return Err("inconsistent measurement confidence"),
        };
        Ok(Self {
            state,
            details: wire.details,
        })
    }
}

impl From<MeasurementConfidence> for ConfidenceWire {
    fn from(confidence: MeasurementConfidence) -> Self {
        Self {
            complete: confidence.is_complete(),
            partial: !confidence.is_complete(),
            details: confidence.details,
        }
    }
}

impl MeasurementConfidence {
    pub(crate) fn is_complete(&self) -> bool {
        self.state == ConfidenceState::Complete
    }

    pub(crate) fn mark_partial(&mut self, reason: &str) {
        self.state = ConfidenceState::Partial;
        let missing = self
            .details
            .entry("missing_input")
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Some(missing) = missing.as_array_mut() {
            missing.push(Value::String(reason.to_string()));
        }
    }

    pub(crate) fn insert(&mut self, key: &str, value: Value) {
        self.details.insert(key.to_string(), value);
    }
}

#[derive(Deserialize)]
pub(crate) struct ArtifactHeader {
    pub(crate) schema_version: u64,
    pub(crate) tool: String,
    pub(crate) risk_model_version: Option<u64>,
    pub(crate) measurement_confidence: MeasurementConfidence,
    pub(crate) input_fingerprint: FingerprintHeader,
}

#[derive(Deserialize)]
pub(crate) struct FingerprintHeader {
    pub(crate) digest: String,
    pub(crate) complete: bool,
    pub(crate) algorithm: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct BenchmarkRecord {
    pub(crate) id: String,
    pub(crate) estimate_nanoseconds: f64,
    #[serde(flatten)]
    pub(crate) details: Map<String, Value>,
}

pub(crate) fn benchmark_records(value: &Value) -> Result<Vec<BenchmarkRecord>> {
    let records: Vec<BenchmarkRecord> =
        serde_json::from_value(value.clone()).context("invalid benchmark records")?;
    benchmark_index(&records)?;
    Ok(records)
}

pub(crate) fn benchmark_index(records: &[BenchmarkRecord]) -> Result<BTreeMap<String, f64>> {
    let mut index = BTreeMap::new();
    for record in records {
        ensure!(
            !record.id.trim().is_empty(),
            "benchmark requires a nonempty ID"
        );
        ensure!(
            record.estimate_nanoseconds.is_finite() && record.estimate_nanoseconds > 0.0,
            "benchmark estimate must be a positive finite number"
        );
        ensure!(
            index
                .insert(record.id.clone(), record.estimate_nanoseconds)
                .is_none(),
            "duplicate benchmark ID {}",
            record.id
        );
    }
    Ok(index)
}

#[derive(Deserialize)]
pub(crate) struct CoveragePayload {
    summary: CoverageSummary,
}

#[derive(Deserialize)]
struct CoverageSummary {
    lines: CoverageMetric,
}

#[derive(Deserialize)]
struct CoverageMetric {
    percent: f64,
}

impl CoveragePayload {
    pub(crate) fn line_percent(&self) -> Result<f64> {
        let percent = self.summary.lines.percent;
        ensure!(
            percent.is_finite() && (0.0..=100.0).contains(&percent),
            "invalid line coverage percentage"
        );
        Ok(percent)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum InputStatus {
    Missing,
    Unreadable,
    Incompatible,
    Stale,
    Partial,
    Available,
}

#[derive(Serialize)]
pub(crate) struct InputState {
    pub(crate) status: InputStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reason: Option<String>,
}

impl InputState {
    pub(crate) fn new(status: InputStatus, reason: Option<String>) -> Self {
        Self { status, reason }
    }
}

#[cfg(test)]
mod tests {
    use super::MeasurementConfidence;
    use anyhow::Result;
    use serde_json::json;

    #[test]
    fn confidence_cannot_represent_contradictory_or_missing_states() -> Result<()> {
        for value in [
            json!({}),
            json!({"complete": true, "partial": true}),
            json!({"complete": false, "partial": false}),
        ] {
            assert!(serde_json::from_value::<MeasurementConfidence>(value).is_err());
        }
        let mut confidence: MeasurementConfidence = serde_json::from_value(
            json!({"complete": true, "partial": false, "confidence_scope": "test"}),
        )?;
        confidence.mark_partial("input changed");
        let wire = serde_json::to_value(confidence)?;
        assert_eq!(wire["complete"], false);
        assert_eq!(wire["partial"], true);
        assert_eq!(wire["confidence_scope"], "test");
        assert_eq!(wire["missing_input"], json!(["input changed"]));
        Ok(())
    }
}
