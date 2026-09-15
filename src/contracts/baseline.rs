//! Historical evidence must be valid and complete, but need not match today's
//! source fingerprint. Freshness and comparability are separate checks.
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::path::Path;

pub(crate) fn read_baseline(path: &Path, file: &str) -> Result<Value> {
    let path = if path.is_dir() {
        path.join(file)
    } else {
        path.to_path_buf()
    };
    let value: Value = serde_json::from_str(
        &std::fs::read_to_string(&path)
            .with_context(|| format!("reading baseline {}", path.display()))?,
    )
    .with_context(|| format!("parsing baseline {}", path.display()))?;
    validate_comparison_evidence(file, &value)
        .with_context(|| format!("invalid baseline {}", path.display()))?;
    Ok(value)
}

pub(crate) fn validate_comparison_evidence(file: &str, value: &Value) -> Result<()> {
    let header = super::validate_artifact(file, value)?;
    ensure!(
        header.measurement_confidence.is_complete() && header.input_fingerprint.complete,
        "{file} contains incomplete evidence"
    );
    ensure!(
        header
            .input_fingerprint
            .algorithm
            .as_deref()
            .is_some_and(|s| !s.is_empty()),
        "{file} is missing its fingerprint algorithm"
    );
    match file {
        "map.json" => ensure!(
            value["data"]["graph"]["nodes"]
                .as_array()
                .is_some_and(|nodes| nodes
                    .iter()
                    .all(|node| node["data"]["total_score"].is_number())),
            "map contains unknown scores"
        ),
        "coverage.json" => {
            coverage_percent(value)?;
        }
        "performance.json" => {
            let records = value["data"]["records"]
                .as_array()
                .context("missing benchmark records")?;
            ensure!(
                !records.is_empty(),
                "no benchmarks are available for comparison"
            );
            super::benchmark_records(&value["data"]["records"])?;
        }
        _ => anyhow::bail!("unsupported comparison evidence {file}"),
    }
    Ok(())
}

pub(crate) fn coverage_percent(value: &Value) -> Result<f64> {
    let coverage: super::CoveragePayload = serde_json::from_value(value["data"].clone())
        .context("missing or invalid line coverage percentage")?;
    coverage.line_percent()
}

#[cfg(test)]
mod tests {
    use super::{coverage_percent, validate_comparison_evidence};
    use serde_json::{Value, json};

    fn document(file: &str, payload: Value) -> Value {
        let performance = file == "performance.json";
        json!({
            "schema_version": if performance {1} else {3},
            "risk_model_version": crate::measurement::MODEL_VERSION,
            "tool": if performance {"performance"} else {"coverage"},
            "measurement_confidence": {"complete": true, "partial": false},
            "input_fingerprint": {"digest": "historical", "algorithm": "v1", "complete": true},
            "data": payload
        })
    }

    #[test]
    fn malformed_incomplete_and_wrong_version_baselines_are_rejected() {
        for (file, payload) in [
            (
                "coverage.json",
                json!({"summary": {"lines": {"percent": 90.0}}}),
            ),
            (
                "performance.json",
                json!({"records": [{"id": "parse", "estimate_nanoseconds": 100.0}]}),
            ),
        ] {
            let valid = document(file, payload);
            assert!(validate_comparison_evidence(file, &valid).is_ok());
            for (key, replacement) in [
                ("schema_version", json!(0)),
                (
                    "measurement_confidence",
                    json!({"complete": false, "partial": true}),
                ),
                (
                    "input_fingerprint",
                    json!({"digest": "old", "complete": false}),
                ),
                ("data", json!({})),
            ] {
                let mut invalid = valid.clone();
                invalid[key] = replacement;
                assert!(
                    validate_comparison_evidence(file, &invalid).is_err(),
                    "{file}: {key}"
                );
            }
        }
    }

    #[test]
    fn missing_invalid_and_duplicate_metrics_are_not_empty_comparisons() {
        for percent in [Value::Null, json!("90"), json!(-1), json!(101)] {
            assert!(
                coverage_percent(&json!({"data": {"summary": {"lines": {"percent": percent}}}}))
                    .is_err()
            );
        }
        for records in [
            vec![json!({})],
            vec![json!({"id": "parse", "estimate_nanoseconds": 0})],
            vec![json!({"id": "parse", "estimate_nanoseconds": -1})],
            vec![json!({"id": "parse", "estimate_nanoseconds": "100"})],
            vec![json!({"id": "parse", "estimate_nanoseconds": 100}); 2],
        ] {
            assert!(super::super::benchmark_records(&json!(records)).is_err());
        }
    }
}
