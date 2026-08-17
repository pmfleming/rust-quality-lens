use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::LensConfig;
use crate::util::{project_input_fingerprint, stable_hash, write_json};

const MAX_FUTURE_SKEW_SECONDS: i64 = 300;

pub(crate) fn ingest(config: &LensConfig, input: &Path, max_age_hours: u64) -> Result<PathBuf> {
    let text = fs::read_to_string(input)
        .with_context(|| format!("reading telemetry evidence {}", input.display()))?;
    let value: Value = serde_json::from_str(&text)?;
    let signals = value["signals"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("telemetry input requires a signals array"))?;
    let window_end = value["window"]["end"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("telemetry input requires window.end as RFC 3339"))?;
    let end = DateTime::parse_from_rfc3339(window_end)
        .with_context(|| format!("invalid telemetry window.end {window_end}"))?
        .with_timezone(&Utc);
    let now = Utc::now();
    let age_hours = telemetry_age_hours(end, now)?;
    let stale = age_hours > max_age_hours;
    let records = signals
        .iter()
        .map(normalize_signal)
        .collect::<Result<Vec<_>>>()?;
    let breached = records
        .iter()
        .filter(|record| record["status"] == "breached")
        .count();
    let unknown = records
        .iter()
        .filter(|record| record["status"] == "unknown")
        .count();
    let document = json!({
        "schema_version": 1,
        "generated_from": "rqlens",
        "generated_at": Utc::now().to_rfc3339(),
        "generator_version": env!("CARGO_PKG_VERSION"),
        "input_fingerprint": project_input_fingerprint(&config.project_root, &config.source_roots),
        "telemetry_input": {
            "algorithm": "fnv1a64-content-v1",
            "digest": stable_hash(&text),
            "path": input,
            "window": value["window"],
            "age_hours": age_hours,
            "max_age_hours": max_age_hours,
        },
        "tool": "operational-evidence",
        "measurement_confidence": {
            "complete": !stale && unknown == 0,
            "partial": stale || unknown > 0,
            "confidence_scope": "observed_production_telemetry",
            "required_inputs": ["normalized_telemetry_signals", "observation_window"],
            "observed_inputs": {"signal_count": records.len(), "window_end": window_end},
            "missing_input": [],
            "stale_input": if stale { vec![format!("telemetry window ended {age_hours} hours ago")] } else { Vec::<String>::new() },
            "unsupported_pattern": if unknown > 0 { vec![format!("{unknown} signals have unknown status")] } else { Vec::<String>::new() },
        },
        "summary": {
            "signal_count": records.len(),
            "healthy": records.len().saturating_sub(breached + unknown),
            "breached": breached,
            "unknown": unknown,
            "window_end": window_end,
        },
        "data": {"records": records},
    });
    fs::create_dir_all(&config.output_dir)?;
    let output = config.output_dir.join("operational_evidence.json");
    write_json(&output, &document)?;
    Ok(output)
}

fn telemetry_age_hours(end: DateTime<Utc>, now: DateTime<Utc>) -> Result<u64> {
    if end > now + chrono::Duration::seconds(MAX_FUTURE_SKEW_SECONDS) {
        bail!(
            "telemetry window.end {end} is in the future beyond the allowed {} second clock skew",
            MAX_FUTURE_SKEW_SECONDS
        );
    }
    Ok(now.signed_duration_since(end).num_hours().max(0) as u64)
}

fn normalize_signal(signal: &Value) -> Result<Value> {
    let id = required_string(signal, "id")?;
    let kind = required_string(signal, "kind")?;
    let status = required_string(signal, "status")?;
    if !matches!(status, "healthy" | "breached" | "unknown") {
        bail!("telemetry signal {id} has invalid status {status}");
    }
    let source = required_string(signal, "source")?;
    let value = signal
        .get("value")
        .filter(|value| value.is_number())
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("telemetry signal {id} requires a numeric value"))?;
    Ok(json!({
        "id": id,
        "kind": kind,
        "status": status,
        "value": value,
        "unit": signal.get("unit").cloned().unwrap_or(Value::Null),
        "threshold": signal.get("threshold").cloned().unwrap_or(Value::Null),
        "modules": signal.get("modules").cloned().unwrap_or_else(|| json!([])),
        "service": signal.get("service").cloned().unwrap_or(Value::Null),
        "source": source,
        "evidence_class": "operational-observed",
    }))
}

fn required_string<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value[field]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("telemetry signal requires non-empty {field}"))
}

#[cfg(test)]
mod tests {
    use super::{normalize_signal, telemetry_age_hours};
    use chrono::{TimeZone, Utc};
    use serde_json::json;

    #[test]
    fn future_telemetry_windows_are_rejected_beyond_clock_skew() {
        let Some(now) = Utc.with_ymd_and_hms(2026, 8, 20, 12, 0, 0).single() else {
            panic!("fixed UTC timestamp should be valid");
        };
        assert_eq!(
            telemetry_age_hours(now + chrono::Duration::minutes(4), now).ok(),
            Some(0)
        );
        assert!(telemetry_age_hours(now + chrono::Duration::minutes(6), now).is_err());
        assert_eq!(
            telemetry_age_hours(now - chrono::Duration::hours(25), now).ok(),
            Some(25)
        );
    }

    #[test]
    fn telemetry_status_must_be_explicit() {
        let signal = normalize_signal(&json!({
            "id": "api-errors",
            "kind": "error-rate",
            "status": "breached",
            "value": 2.1,
            "unit": "percent",
            "source": "prometheus"
        }));
        assert_eq!(
            signal.ok().map(|value| value["evidence_class"].clone()),
            Some(json!("operational-observed"))
        );
        assert!(
            normalize_signal(&json!({
                "id": "bad", "kind": "error-rate", "status": "maybe", "value": 1, "source": "test"
            }))
            .is_err()
        );
    }
}
