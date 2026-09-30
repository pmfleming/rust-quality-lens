use anyhow::{Result, ensure};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

#[derive(Serialize)]
#[serde(transparent)]
struct Text(String);
impl<'de> Deserialize<'de> for Text {
    fn deserialize<D: Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        if value.trim().is_empty() {
            return Err(serde::de::Error::custom("empty evidence text"));
        }
        Ok(Self(value))
    }
}
fn required_nullable<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    decoder: D,
) -> std::result::Result<Option<T>, D::Error> {
    Option::deserialize(decoder)
}
#[derive(Serialize, Deserialize)]
struct Producer {
    name: Text,
    version: Text,
}
#[derive(Serialize, Deserialize)]
struct Location {
    file: Text,
    line: f64,
}
#[derive(Serialize, Deserialize)]
struct Suppression {
    reason: Text,
}
#[derive(Serialize, Deserialize)]
struct Interval {
    lower: f64,
    upper: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Confidence {
    High,
    Medium,
    Low,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Severity {
    Error,
    Warning,
    Note,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Disposition {
    Block,
    Warn,
    Review,
    Info,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Diagnostic,
    Test,
    Metric,
    Heuristic,
    Semantic,
    #[serde(rename = "tool-rule")]
    ToolRule,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Compatibility {
    Compatible,
    Incompatible,
    Unknown,
}

#[derive(Serialize, Deserialize)]
struct Evidence {
    schema_version: f64,
    #[serde(flatten)]
    body: Body,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Body {
    Run {
        id: Text,
        analyzer: Producer,
        ruleset: Text,
        config_fingerprint: Text,
        input_fingerprint: Text,
        language_profile: Text,
        producers: Vec<Producer>,
    },
    Finding {
        id: Text,
        rule_id: Text,
        subject: Text,
        run_id: Text,
        locations: Vec<Location>,
        evidence_kind: Kind,
        confidence: Confidence,
        severity: Severity,
        disposition: Disposition,
        message: Text,
        #[serde(deserialize_with = "required_nullable")]
        suppression: Option<Suppression>,
    },
    Measurement {
        name: Text,
        #[serde(deserialize_with = "required_nullable")]
        value: Option<f64>,
        unit: Text,
        scope: Text,
        model: Text,
        run_id: Text,
        #[serde(deserialize_with = "required_nullable")]
        uncertainty: Option<Interval>,
        limitations: Vec<Text>,
    },
    Comparison {
        status: Compatibility,
        current_run: Text,
        #[serde(deserialize_with = "required_nullable")]
        baseline_run: Option<Text>,
        reasons: Vec<Text>,
    },
}
impl Evidence {
    fn valid(&self) -> bool {
        if self.schema_version != 1.0 {
            return false;
        }
        match &self.body {
            Body::Run { .. } => true,
            Body::Finding {
                rule_id, locations, ..
            } => {
                rule_id.0.contains(['.', '/', ':'])
                    && locations.iter().all(|item| {
                        item.line >= 1.0
                            && item.line <= 9_007_199_254_740_991.0
                            && item.line.fract() == 0.0
                    })
            }
            Body::Measurement {
                value,
                uncertainty,
                limitations,
                ..
            } => {
                let observed = value.is_some_and(f64::is_finite);
                (observed || (value.is_none() && !limitations.is_empty()))
                    && uncertainty.as_ref().is_none_or(|range| {
                        observed
                            && range.lower.is_finite()
                            && range.upper.is_finite()
                            && range.lower <= range.upper
                    })
            }
            Body::Comparison {
                status,
                baseline_run,
                reasons,
                ..
            } => match status {
                Compatibility::Compatible => baseline_run.is_some() && reasons.is_empty(),
                _ => !reasons.is_empty(),
            },
        }
    }
}

pub(crate) fn validate_extension(value: Option<&Value>) -> Result<()> {
    let Some(value) = value else {
        return Ok(());
    };
    let evidence: Vec<Evidence> = serde_json::from_value(value.clone())?;
    ensure!(
        evidence.iter().all(Evidence::valid),
        "invalid cross-lens evidence contract"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_wire_contract_cases() -> Result<()> {
        let cases: Vec<Value> = serde_json::from_str(include_str!("../contracts/wire-cases.json"))?;
        for case in cases {
            assert_eq!(
                validate_extension(Some(&serde_json::json!([case["value"]]))).is_ok(),
                case["valid"] == true,
                "{}",
                case["id"]
            );
        }
        assert!(validate_extension(None).is_ok());
        assert!(validate_extension(Some(&Value::Null)).is_err());
        Ok(())
    }
}
