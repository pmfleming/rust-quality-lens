use anyhow::{Result, bail, ensure};
use serde_json::Value;

use crate::measurement::MODEL_VERSION;
use crate::tool::MeasureTool;

// Consumers must validate before interpreting absent fields as empty findings.
// Legacy, unversioned payloads are not accepted as current measurement evidence.
pub(crate) fn validate_artifact(file: &str, value: &Value) -> Result<super::ArtifactHeader> {
    crate::wire_evidence::validate_extension(value.get("evidence_contract"))?;
    let header: super::ArtifactHeader = serde_json::from_value(value.clone())?;
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
        header.schema_version == if auxiliary.is_some() { 1 } else { 3 },
        "unsupported or missing artifact schema version"
    );
    ensure!(
        Some(header.tool.as_str()) == expected_tool
            || (file == "correctness_review.json" && header.tool == "correctness"),
        "artifact tool does not match {file}"
    );
    if auxiliary.is_none() {
        ensure!(
            header.risk_model_version == Some(MODEL_VERSION),
            "artifact uses a different or missing risk model version"
        );
    }
    ensure!(
        !header.input_fingerprint.digest.is_empty(),
        "missing input fingerprint digest"
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
    let payload = checked_payload(value, array_payload)?;
    if file == "coverage.json" && header.measurement_confidence.is_complete() {
        super::coverage_percent(value)?;
    }
    validate_policy_payload(file, payload, &header)?;
    Ok(header)
}

fn checked_payload(value: &Value, array_payload: bool) -> Result<&Value> {
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
    Ok(payload)
}

fn validate_policy_payload(
    file: &str,
    payload: &Value,
    header: &super::ArtifactHeader,
) -> Result<()> {
    match file {
        "correctness_review.json" => test_summary(payload),
        "rust_practices.json" => each(&payload["checks"], practice),
        "reliability_findings.json" => each(payload, reliability),
        "architecture_rules.json" => each(&payload["violations"], finding),
        "test_quality.json" => each(&payload["findings"], finding),
        "map.json" => each(&payload["graph"]["nodes"], map_node),
        "type_health.json" => {
            type_health_rows(payload, header.measurement_confidence.is_complete())
        }
        "operational_evidence.json" => each(&payload["records"], operational),
        "performance.json" => super::benchmark_records(&payload["records"]).map(|_| ()),
        "repository_outcomes.json" => array(&payload["records"]).map(|_| ()),
        _ => Ok(()),
    }
}

fn type_health_rows(payload: &Value, complete: bool) -> Result<()> {
    for row in array(payload)? {
        type_health(row)?;
        ensure!(
            !complete || row["measurement_confidence"]["complete"] == true,
            "complete type-health artifact contains a partial row"
        );
    }
    Ok(())
}

fn type_health(row: &Value) -> Result<()> {
    let confidence: super::MeasurementConfidence =
        serde_json::from_value(row["measurement_confidence"].clone())?;
    for (field, integer) in [
        ("method_count", true),
        ("impl_block_count", true),
        ("impl_file_count", true),
        ("structural_risk", false),
        ("structural_score", false),
    ] {
        let value = row
            .get(field)
            .ok_or_else(|| anyhow::anyhow!("missing {field}"))?;
        let numeric = if integer {
            value.as_u64().is_some()
        } else {
            value.as_f64().is_some()
        };
        ensure!(
            if confidence.is_complete() {
                numeric
            } else {
                value.is_null()
            },
            "type-health {field} must be numeric when complete and null when partial"
        );
    }
    for field in [
        "observed_method_count",
        "observed_impl_block_count",
        "observed_impl_file_count",
    ] {
        ensure!(row[field].as_u64().is_some(), "missing or invalid {field}");
    }
    Ok(())
}

fn each(value: &Value, validate: fn(&Value) -> Result<()>) -> Result<()> {
    array(value)?.iter().try_for_each(validate)
}

fn test_summary(payload: &Value) -> Result<()> {
    let summary = &payload["summary"];
    ensure!(
        summary["failed"].as_u64().is_some(),
        "missing or invalid failed test count"
    );
    ensure!(
        summary["run_failed"].is_boolean() && summary["compile_failed"].is_boolean(),
        "missing test execution status"
    );
    array(&payload["tests"]).map(|_| ())
}

fn practice(value: &Value) -> Result<()> {
    finding(value)?;
    one_of(
        &value["status"],
        &["passed", "failed", "unavailable", "timed-out", "skipped"],
        "invalid practice check status",
    )
}

fn reliability(value: &Value) -> Result<()> {
    finding(value)?;
    one_of(
        &value["scope"],
        &["production", "test"],
        "invalid reliability scope",
    )
}

fn operational(value: &Value) -> Result<()> {
    one_of(
        &value["status"],
        &["healthy", "breached", "unknown"],
        "invalid operational status",
    )
}

fn one_of(value: &Value, allowed: &[&str], message: &str) -> Result<()> {
    ensure!(
        value.as_str().is_some_and(|text| allowed.contains(&text)),
        "{message}"
    );
    Ok(())
}

fn map_node(node: &Value) -> Result<()> {
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
    use super::{
        map_node, operational, practice, reliability, type_health_rows, validate_artifact,
    };
    use crate::measurement::MODEL_VERSION;
    use anyhow::Result;
    use serde_json::{Value, json};

    #[test]
    fn finding_status_domains_remain_distinct() {
        type Validator = fn(&Value) -> Result<()>;
        for (validate, field, allowed) in [
            (
                practice as Validator,
                "status",
                &["passed", "failed", "unavailable", "timed-out", "skipped"][..],
            ),
            (
                reliability as Validator,
                "scope",
                &["production", "test"][..],
            ),
            (
                operational as Validator,
                "status",
                &["healthy", "breached", "unknown"][..],
            ),
        ] {
            let mut row = json!({"rule_id": "test", "severity": "warning"});
            for value in allowed {
                row[field] = json!(value);
                assert!(validate(&row).is_ok(), "{value}");
            }
            for value in [Value::Null, json!(1), json!("unrecognized")] {
                row[field] = value;
                assert!(validate(&row).is_err());
            }
        }
    }

    #[test]
    fn type_health_does_not_validate_unknown_counts_as_zero() -> Result<()> {
        let mut row = json!({
            "measurement_confidence": {"complete": false, "partial": true},
            "method_count": null, "impl_block_count": null, "impl_file_count": null,
            "structural_risk": null, "structural_score": null,
            "observed_method_count": 0, "observed_impl_block_count": 0, "observed_impl_file_count": 0,
        });
        type_health_rows(&json!([row]), false)?;
        assert!(type_health_rows(&json!([row]), true).is_err());
        row["method_count"] = json!(0);
        assert!(type_health_rows(&json!([row]), false).is_err());
        row["measurement_confidence"] = json!({"complete": true, "partial": false});
        for field in [
            "method_count",
            "impl_block_count",
            "impl_file_count",
            "structural_risk",
            "structural_score",
        ] {
            row[field] = json!(0);
        }
        type_health_rows(&json!([row]), true)?;
        Ok(())
    }

    #[test]
    fn map_validation_requires_identity_and_an_explicit_nullable_score() {
        for score in [Value::Null, json!(0), json!(1.5)] {
            assert!(map_node(&json!({"data": {"id": "module", "total_score": score}})).is_ok());
        }
        for node in [
            json!({"data": {"id": "module"}}),
            json!({"data": {"total_score": 0}}),
            json!({"data": {"id": "module", "total_score": "unknown"}}),
        ] {
            assert!(map_node(&node).is_err());
        }
    }

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
