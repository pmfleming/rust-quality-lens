use anyhow::{Result, bail, ensure};
use serde::Deserialize;
use serde_json::Value;

use crate::measurement::MODEL_VERSION;
use crate::tool::MeasureTool;

#[derive(Deserialize)]
struct Confidence {
    complete: bool,
    partial: bool,
}

// Consumers must validate before interpreting absent fields as empty findings.
// Legacy, unversioned payloads are not accepted as current measurement evidence.
pub(crate) fn validate_artifact(file: &str, value: &Value) -> Result<()> {
    let auxiliary = match file {
        "performance.json" => Some("performance"),
        "repository_outcomes.json" => Some("repository-outcomes"),
        "operational_evidence.json" => Some("operational-evidence"),
        _ => None,
    };
    let tool = MeasureTool::schema_tools()
        .into_iter()
        .find(|tool| tool.output_file() == file);
    let expected_tool = auxiliary.or_else(|| tool.as_ref().map(MeasureTool::name));
    ensure!(expected_tool.is_some(), "unknown artifact {file}");
    ensure!(
        value["schema_version"].as_u64() == Some(if auxiliary.is_some() { 1 } else { 3 }),
        "unsupported or missing artifact schema version"
    );
    ensure!(
        value["tool"].as_str() == expected_tool
            || (file == "correctness_review.json" && value["tool"] == "correctness"),
        "artifact tool does not match {file}"
    );
    if auxiliary.is_none() {
        ensure!(
            value["risk_model_version"].as_u64() == Some(MODEL_VERSION),
            "artifact uses a different or missing risk model version"
        );
    }
    let confidence: Confidence = serde_json::from_value(value["measurement_confidence"].clone())?;
    ensure!(
        confidence.complete != confidence.partial,
        "inconsistent measurement confidence"
    );
    ensure!(
        value["input_fingerprint"]["digest"]
            .as_str()
            .is_some_and(|digest| !digest.is_empty()),
        "missing input fingerprint digest"
    );
    ensure!(
        value["input_fingerprint"]["complete"].is_boolean(),
        "missing fingerprint completeness"
    );
    let array_payload = matches!(
        tool,
        Some(
            MeasureTool::Hotspots
                | MeasureTool::Clones
                | MeasureTool::EscapeHatches
                | MeasureTool::Reliability
                | MeasureTool::ApiHealth
                | MeasureTool::TypeHealth
                | MeasureTool::Locality
                | MeasureTool::Leverage
        )
    );
    let (key, other) = if array_payload {
        ("records", "data")
    } else {
        ("data", "records")
    };
    ensure!(value.get(other).is_none(), "ambiguous artifact payload");
    let payload = &value[key];
    ensure!(
        if array_payload {
            payload.is_array()
        } else {
            payload.is_object()
        },
        "missing or invalid {key} payload"
    );
    if file == "coverage.json" && confidence.complete {
        super::coverage_percent(value)?;
    }
    validate_policy_payload(file, payload)
}

fn validate_policy_payload(file: &str, payload: &Value) -> Result<()> {
    match file {
        "correctness_review.json" => {
            let summary = &payload["summary"];
            ensure!(
                summary["failed"].as_u64().is_some(),
                "missing or invalid failed test count"
            );
            ensure!(
                summary["run_failed"].is_boolean() && summary["compile_failed"].is_boolean(),
                "missing test execution status"
            );
            array(&payload["tests"])?;
        }
        "rust_practices.json" => {
            for check in array(&payload["checks"])? {
                finding(check)?;
                ensure!(
                    matches!(
                        check["status"].as_str(),
                        Some("passed" | "failed" | "unavailable" | "timed-out" | "skipped")
                    ),
                    "invalid practice check status"
                );
            }
        }
        "reliability_findings.json" => {
            for row in array(payload)? {
                finding(row)?;
                ensure!(
                    matches!(row["scope"].as_str(), Some("production" | "test")),
                    "invalid reliability scope"
                );
            }
        }
        "architecture_rules.json" | "test_quality.json" => {
            let key = if file == "architecture_rules.json" {
                "violations"
            } else {
                "findings"
            };
            for row in array(&payload[key])? {
                finding(row)?;
            }
        }
        "map.json" => {
            for node in array(&payload["graph"]["nodes"])? {
                ensure!(
                    node["data"]["id"].is_string(),
                    "map node is missing its identity"
                );
                ensure!(
                    node["data"]
                        .get("total_score")
                        .is_some_and(|score| score.is_null() || score.is_number()),
                    "map node is missing its score"
                );
            }
        }
        "operational_evidence.json" => {
            for row in array(&payload["records"])? {
                ensure!(
                    matches!(
                        row["status"].as_str(),
                        Some("healthy" | "breached" | "unknown")
                    ),
                    "invalid operational status"
                );
            }
        }
        "performance.json" => {
            super::benchmark_estimates(array(&payload["records"])?)?;
        }
        "repository_outcomes.json" => {
            array(&payload["records"])?;
        }
        _ => {}
    }
    Ok(())
}

fn array(value: &Value) -> Result<&Vec<Value>> {
    value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("missing or invalid records array"))
}

fn finding(value: &Value) -> Result<()> {
    if !value["rule_id"].is_string()
        || !matches!(
            value["severity"].as_str(),
            Some("error" | "warning" | "advisory")
        )
    {
        bail!("finding requires a rule ID and a valid severity");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn static_correctness_is_valid_evidence_but_not_a_test_execution() {
        let document = json!({
            "schema_version": 3, "risk_model_version": MODEL_VERSION, "tool": "correctness",
            "measurement_confidence": {"complete": true, "partial": false},
            "input_fingerprint": {"digest": "test", "complete": true},
            "data": {"summary": {"failed": 0, "run_failed": false, "compile_failed": false}, "tests": []}
        });
        assert!(validate_artifact("correctness_review.json", &document).is_ok());
    }

    #[test]
    fn rejects_unversioned_wrong_tool_and_inconsistent_envelopes() {
        let valid = json!({
            "schema_version": 3, "risk_model_version": MODEL_VERSION, "tool": "reliability",
            "measurement_confidence": {"complete": true, "partial": false},
            "input_fingerprint": {"digest": "test", "complete": true}, "records": []
        });
        assert!(validate_artifact("reliability_findings.json", &valid).is_ok());
        for (key, replacement) in [
            ("schema_version", Value::Null),
            ("tool", json!("hotspots")),
            ("records", json!({})),
            (
                "measurement_confidence",
                json!({"complete": true, "partial": true}),
            ),
            ("records", json!([{"rule_id": "test"}])),
        ] {
            let mut invalid = valid.clone();
            invalid[key] = replacement;
            assert!(
                validate_artifact("reliability_findings.json", &invalid).is_err(),
                "{key}"
            );
        }
    }
}
