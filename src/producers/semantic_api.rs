use anyhow::Result;
use serde_json::{Value, json};
use std::time::Duration;

use crate::command_runner::{CommandRequest, CommandStatus, run};
use crate::config::LensConfig;

pub(super) fn produce(config: &LensConfig) -> Result<Value> {
    if !config.verification.public_api {
        return Ok(json!({
            "summary": {"status": "disabled", "item_count": 0},
            "items": [],
            "measurement_confidence": {
                "complete": true,
                "partial": false,
                "confidence_scope": "reachable_public_api",
                "required_inputs": [],
                "observed_inputs": {"enabled": false},
                "missing_input": [],
                "stale_input": [],
                "unsupported_pattern": [],
            }
        }));
    }
    let arguments = vec![
        "public-api".to_string(),
        "--simplified".to_string(),
        "--color=never".to_string(),
    ];
    let mut request = CommandRequest::new("cargo", &arguments, &config.project_root);
    request.timeout = Duration::from_secs(config.verification.timeout_seconds);
    let outcome = run(request);
    let passed = outcome.status == CommandStatus::Passed;
    let items = if passed {
        parse_items(&outcome.stdout)
    } else {
        Vec::new()
    };
    Ok(json!({
        "summary": {
            "status": if passed { "completed" } else { "unavailable" },
            "item_count": items.len(),
        },
        "items": items,
        "command": outcome,
        "measurement_confidence": {
            "complete": passed,
            "partial": !passed,
            "confidence_scope": "reachable_public_api",
            "required_inputs": ["cargo-public-api"],
            "observed_inputs": {"enabled": true, "command_passed": passed},
            "missing_input": if passed { Vec::<String>::new() } else { vec!["cargo-public-api did not complete".to_string()] },
            "stale_input": [],
            "unsupported_pattern": [],
        }
    }))
}

fn parse_items(stdout: &str) -> Vec<Value> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with("Public API"))
        .map(|signature| json!({"signature": signature, "backend": "cargo-public-api"}))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::parse_items;

    #[test]
    fn public_api_text_is_normalized_into_signatures() {
        let items = parse_items("Public API of demo\npub struct Demo\npub fn run()\n");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["signature"], "pub struct Demo");
    }
}
