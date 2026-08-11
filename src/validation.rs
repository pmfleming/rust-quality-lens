use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::util::{round2, write_json};

pub(crate) fn run(
    projects: &[String],
    output_dir: PathBuf,
    include_inferred: bool,
) -> Result<PathBuf> {
    let mut reports = Vec::new();
    for specification in projects {
        let (name, path) = specification.split_once('=').ok_or_else(|| {
            anyhow::anyhow!("project must use name=/path syntax: {specification}")
        })?;
        reports.push(validate_project(name, Path::new(path), include_inferred)?);
    }
    let pooled = pooled_metrics(&reports);
    let complete =
        !include_inferred && reports.iter().all(|report| report["status"] == "validated");
    let report = json!({
        "version": 1,
        "generated_from": "rqlens",
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "validation_design": "retrospective-ranking-association",
        "include_inferred_outcomes": include_inferred,
        "summary": pooled,
        "projects": reports,
        "measurement_confidence": {
            "complete": complete,
            "partial": !complete,
            "confidence_scope": "outcome_rank_validation",
            "required_inputs": ["map.json", "repository_outcomes.json", "module_outcome_labels"],
            "observed_inputs": {"project_count": projects.len()},
            "missing_input": [],
            "stale_input": [],
            "unsupported_pattern": if include_inferred { vec!["commit-message outcomes are unreviewed labels"] } else { Vec::<&str>::new() },
        },
        "interpretation": {
            "precision_at_10": "share of the ten highest-risk modules with a labeled outcome",
            "recall_at_10": "share of scorable labeled-outcome modules appearing in the ten highest-risk modules",
            "top_quartile_lift": "top-quartile outcome rate divided by the project-wide outcome rate",
            "warning": "association is not causation or a defect probability; prospective validation requires scores captured before the labeled outcome window"
        }
    });
    fs::create_dir_all(&output_dir)?;
    let path = output_dir.join("validation_report.json");
    write_json(&path, &report)?;
    Ok(path)
}

fn validate_project(name: &str, path: &Path, include_inferred: bool) -> Result<Value> {
    let analysis = if path.join("map.json").is_file() {
        path.to_path_buf()
    } else {
        path.join("target/analysis")
    };
    let map_path = analysis.join("map.json");
    let outcomes_path = analysis.join("repository_outcomes.json");
    if !map_path.is_file() || !outcomes_path.is_file() {
        let missing = [
            (!map_path.is_file()).then_some("map.json"),
            (!outcomes_path.is_file()).then_some("repository_outcomes.json"),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        return Ok(json!({
            "name": name,
            "path": analysis,
            "status": "missing-input",
            "missing": missing,
        }));
    }
    let map: Value = serde_json::from_str(
        &fs::read_to_string(&map_path)
            .with_context(|| format!("reading {}", map_path.display()))?,
    )?;
    let outcomes: Value = serde_json::from_str(
        &fs::read_to_string(&outcomes_path)
            .with_context(|| format!("reading {}", outcomes_path.display()))?,
    )?;
    let ranked = ranked_modules(&map);
    let labels = outcome_modules(&outcomes, include_inferred);
    let metrics = ranking_metrics(&ranked, &labels);
    Ok(json!({
        "name": name,
        "path": analysis,
        "status": if labels.is_empty() { "no-labels" } else if ranked.is_empty() { "no-scores" } else { "validated" },
        "risk_model_version": map["risk_model_version"],
        "artifact_generated_at": map["generated_at"],
        "ranked_module_count": ranked.len(),
        "labeled_outcome_module_count": labels.len(),
        "metrics": metrics,
    }))
}

fn ranked_modules(map: &Value) -> Vec<(String, String, f64)> {
    let data = map.get("data").unwrap_or(map);
    let mut modules = data["graph"]["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|node| {
            let data = &node["data"];
            Some((
                data["id"].as_str()?.to_string(),
                data["module_key"].as_str()?.to_string(),
                data["total_score"].as_f64()?,
            ))
        })
        .collect::<Vec<_>>();
    modules.sort_by(|left, right| right.2.total_cmp(&left.2));
    modules
}

fn outcome_modules(outcomes: &Value, include_inferred: bool) -> BTreeSet<String> {
    outcomes["data"]["records"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|record| include_inferred || record["evidence_class"] == "reviewed-label")
        .flat_map(|record| record["modules"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

fn ranking_metrics(ranked: &[(String, String, f64)], labels: &BTreeSet<String>) -> Value {
    if ranked.is_empty() || labels.is_empty() {
        return json!({
            "precision_at_10": null,
            "recall_at_10": null,
            "mean_percentile_rank": null,
            "top_quartile_lift": null,
            "scorable_outcome_modules": 0,
        });
    }
    let matches = ranked
        .iter()
        .enumerate()
        .filter(|(_, (id, module, _))| label_matches(labels, id, module))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let cutoff = ranked.len().min(10);
    let top_hits = matches.iter().filter(|index| **index < cutoff).count();
    let quartile = ranked.len().div_ceil(4).max(1);
    let quartile_hits = matches.iter().filter(|index| **index < quartile).count();
    let overall_rate = matches.len() as f64 / ranked.len() as f64;
    let top_rate = quartile_hits as f64 / quartile as f64;
    let percentiles = matches
        .iter()
        .map(|index| {
            if ranked.len() == 1 {
                100.0
            } else {
                (1.0 - *index as f64 / (ranked.len() - 1) as f64) * 100.0
            }
        })
        .collect::<Vec<_>>();
    json!({
        "precision_at_10": round2(top_hits as f64 / cutoff as f64),
        "recall_at_10": if matches.is_empty() { 0.0 } else { round2(top_hits as f64 / matches.len() as f64) },
        "mean_percentile_rank": if percentiles.is_empty() { Value::Null } else { Value::from(round2(percentiles.iter().sum::<f64>() / percentiles.len() as f64)) },
        "top_quartile_lift": if overall_rate == 0.0 { Value::Null } else { Value::from(round2(top_rate / overall_rate)) },
        "scorable_outcome_modules": matches.len(),
        "top_10_hits": top_hits,
    })
}

fn label_matches(labels: &BTreeSet<String>, id: &str, module: &str) -> bool {
    labels.contains(id)
        || labels.contains(module)
        || labels
            .iter()
            .any(|label| id.ends_with(&format!("::{label}")))
}

fn pooled_metrics(reports: &[Value]) -> Value {
    let validated = reports
        .iter()
        .filter(|report| report["status"] == "validated")
        .collect::<Vec<_>>();
    let average = |field: &str| {
        let values = validated
            .iter()
            .filter_map(|report| report["metrics"][field].as_f64())
            .collect::<Vec<_>>();
        if values.is_empty() {
            Value::Null
        } else {
            Value::from(round2(values.iter().sum::<f64>() / values.len() as f64))
        }
    };
    json!({
        "project_count": reports.len(),
        "validated_project_count": validated.len(),
        "average_precision_at_10": average("precision_at_10"),
        "average_recall_at_10": average("recall_at_10"),
        "average_mean_percentile_rank": average("mean_percentile_rank"),
        "average_top_quartile_lift": average("top_quartile_lift"),
    })
}

#[cfg(test)]
mod tests {
    use super::ranking_metrics;
    use std::collections::BTreeSet;

    #[test]
    fn ranking_validation_rewards_outcomes_near_the_top() {
        let ranked = (0..20)
            .map(|index| {
                (
                    format!("pkg::target::m{index}"),
                    format!("m{index}"),
                    (20 - index) as f64,
                )
            })
            .collect::<Vec<_>>();
        let labels = BTreeSet::from(["m0".to_string(), "m1".to_string()]);
        let metrics = ranking_metrics(&ranked, &labels);
        assert_eq!(metrics["recall_at_10"], 1.0);
        assert!(
            metrics["top_quartile_lift"]
                .as_f64()
                .is_some_and(|lift| lift > 1.0)
        );
    }
}
