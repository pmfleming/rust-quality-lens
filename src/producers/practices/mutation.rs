//! cargo-mutants outcomes are observations, not inferred from its exit status.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs};

use crate::{config::LensConfig, util::round2};

pub(super) fn check(config: &LensConfig) -> Value {
    let rule = "rust.tests.mutation";
    let source = "https://mutants.rs/";
    if !config.verification.mutation {
        return super::skipped_check(
            rule,
            "Tests reject generated code mutations",
            "enable verification.mutation to require cargo-mutants",
            source,
        );
    }
    let directory = match tempfile::tempdir() {
        Ok(directory) => directory,
        Err(error) => {
            return json!({"rule_id": rule, "status": "unavailable", "severity": "error",
            "category": "verified-gate", "source": source, "evidence": {"reason": error.to_string()}});
        }
    };
    let mut arguments = vec![
        "mutants".into(),
        "--no-times".into(),
        "--output".into(),
        directory.path().to_string_lossy().into_owned(),
    ];
    if config.verification.workspace {
        arguments.push("--workspace".into());
        for package in &config.verification.exclude {
            arguments.extend(["--exclude".into(), package.clone()]);
        }
    }
    arguments.extend(config.verification.cargo_feature_arguments());
    if config.verification.locked {
        arguments.extend(["--".into(), "--locked".into()]);
    }
    let mut check = super::cargo_check(
        config,
        rule,
        "Tests reject generated code mutations",
        arguments,
        source,
        BTreeMap::new(),
    );
    // A private, fresh output directory prevents old outcomes from certifying a new run.
    let report = fs::read(directory.path().join("mutants.out/outcomes.json"))
        .context("missing fresh cargo-mutants outcomes.json")
        .and_then(|bytes| Ok(serde_json::from_slice::<Value>(&bytes)?))
        .and_then(|document| normalize(&document));
    let execution_complete = matches!(check["evidence"]["exit_code"].as_i64(), Some(0 | 2 | 3))
        && check["evidence"]["reason"].is_null();
    let (mut metrics, mut reasons) = match report {
        Ok(metrics) => (metrics, Vec::new()),
        Err(error) => (
            json!({"records": [], "summary": null}),
            vec![error.to_string()],
        ),
    };
    if !execution_complete {
        reasons
            .push("mutation execution did not finish normally with a successful baseline".into());
    }
    metrics["measurement_confidence"] = super::behavior_confidence("mutation_outcomes", &reasons);
    check["metrics"] = metrics;
    check
}

fn normalize(document: &Value) -> Result<Value> {
    let outcomes = document["outcomes"]
        .as_array()
        .context("outcomes must be an array")?;
    ensure!(
        outcomes
            .iter()
            .any(|row| row["scenario"] == "Baseline" && row["summary"] == "Success"),
        "mutation baseline is missing or unsuccessful"
    );
    let mut records = Vec::new();
    let mut counts = BTreeMap::from([
        ("killed", 0usize),
        ("survived", 0),
        ("timeout", 0),
        ("unviable", 0),
    ]);
    for outcome in outcomes {
        if outcome["scenario"] == "Baseline" {
            continue;
        }
        let mutant = outcome["scenario"]["Mutant"]
            .as_object()
            .context("unsupported mutation scenario")?;
        let mutant = Value::Object(mutant.clone());
        let status = match outcome["summary"].as_str() {
            Some("CaughtMutant") => "killed",
            Some("MissedMutant") => "survived",
            Some("Timeout") => "timeout",
            Some("Unviable") => "unviable",
            _ => anyhow::bail!("unsupported mutation outcome: {}", outcome["summary"]),
        };
        let path = mutant["file"]
            .as_str()
            .filter(|s| !s.is_empty())
            .context("mutant file missing")?;
        let line = mutant["span"]["start"]["line"]
            .as_u64()
            .filter(|line| *line > 0)
            .context("mutant source line missing")?;
        *counts.entry(status).or_default() += 1;
        records.push(json!({"status": status, "path": path, "line": line,
            "package": mutant["package"], "function": mutant["function"]["function_name"],
            "mutation": mutant, "phase_results": outcome["phase_results"]}));
    }
    ensure!(
        document["total_mutants"].as_u64() == Some(records.len() as u64),
        "mutation total does not match outcomes"
    );
    let denominator = counts["killed"] + counts["survived"];
    Ok(
        json!({"model_version": 1, "scope": "mutants selected by cargo-mutants configuration; not all possible faults",
        "score_formula": "100 * killed / (killed + survived); timeout and unviable excluded",
        "summary": {"counts": counts, "tested_mutants": records.len(),
            "score_denominator": denominator,
            "mutation_score_percent": (denominator > 0).then(|| round2(100.0 * counts["killed"] as f64 / denominator as f64)),
            "excluded_mutants": null, "equivalent_mutants": null},
        "records": records, "tool_version": document["cargo_mutants_version"]}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mutation_denominator_and_locations_are_explicit() -> Result<()> {
        let mut rows = vec![json!({"scenario": "Baseline", "summary": "Success"})];
        for summary in ["CaughtMutant", "MissedMutant", "Timeout", "Unviable"] {
            rows.push(
                json!({"scenario": {"Mutant": {"file": "src/lib.rs", "span": {"start": {"line": 3}},
                "function": {"function_name": "foo"}}}, "summary": summary}),
            );
        }
        let mut input = json!({"outcomes": rows, "total_mutants": 4});
        let metrics = normalize(&input)?;
        assert_eq!(metrics["summary"]["mutation_score_percent"], 50.0);
        assert_eq!(metrics["summary"]["score_denominator"], 2);
        assert_eq!(metrics["records"][1]["function"], "foo");
        input["outcomes"][1]["summary"] = json!("FutureOutcome");
        assert!(normalize(&input).is_err());
        input["outcomes"] = json!([{"scenario": "Baseline", "summary": "Success"}]);
        assert!(normalize(&input).is_err());
        input["total_mutants"] = json!(0);
        assert!(normalize(&input)?["summary"]["mutation_score_percent"].is_null());
        input["outcomes"][0]["summary"] = json!("Failure");
        assert!(normalize(&input).is_err());
        Ok(())
    }
}
