//! Opt-in executed compatibility and resource profiles; separate from static risk.
use crate::{
    config::LensConfig,
    measurement::InputSnapshot,
    util::{lock_file, round2, write_json},
};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::PathBuf};

mod compatibility;
pub(crate) mod config;
mod resources;
pub(crate) use config::ProfileConfig;

pub(crate) fn run(config: &LensConfig, require_complete: bool) -> Result<PathBuf> {
    config.profiles.validate()?;
    ensure!(
        !config.profiles.compatibility.is_empty() || !config.profiles.resources.is_empty(),
        "configure at least one profiles.compatibility or profiles.resources entry"
    );
    fs::create_dir_all(&config.output_dir)?;
    let _lock = lock_file(&config.output_dir.join("rqlens"))?;
    let snapshot = InputSnapshot::capture(config);
    let compatibility: Vec<_> = config
        .profiles
        .compatibility
        .iter()
        .map(|profile| compatibility::measure(config, profile, &snapshot.toolchain))
        .collect();
    let resources: Vec<_> = config
        .profiles
        .resources
        .iter()
        .map(|profile| resources::measure(config, profile))
        .collect();
    let mut reasons = Vec::new();
    let rows: Vec<_> = compatibility.iter().chain(&resources).collect();
    for row in &rows {
        if row["measurement_confidence"]["complete"] != true {
            reasons.push(format!(
                "profile {} is incomplete; see its measurement_confidence",
                row["id"]
            ));
        }
    }
    if snapshot.fingerprint["complete"] != true {
        reasons.push("project input fingerprint is incomplete".into());
    }
    let failed = rows
        .iter()
        .filter(|row| {
            matches!(
                row["status"].as_str(),
                Some("failed" | "unavailable" | "timed-out")
            )
        })
        .count();
    let document = json!({"schema_version": 1, "tool": "quality-profiles", "generated_from": "rqlens",
        "generator_version": env!("CARGO_PKG_VERSION"), "generated_at": chrono::Utc::now().to_rfc3339(),
        "input_fingerprint": snapshot.fingerprint,
        "environment": {"os": std::env::consts::OS, "arch": std::env::consts::ARCH,
            "host": hostname::get().ok().map(|host| host.to_string_lossy().into_owned()),
            "toolchain": snapshot.toolchain, "compiler_environment": compiler_environment()},
        "measurement_confidence": confidence("declared_quality_profiles", &reasons),
        "summary": {"profile_count": rows.len(), "failed_or_unavailable": failed,
            "compatibility": compatibility_summary(&compatibility)},
        "units": {"elapsed_ms": "milliseconds", "peak_rss_bytes": "bytes", "artifact_bytes": "bytes",
            "allocations": "count", "allocated_bytes": "bytes", "instructions": "count", "cache_misses": "count"},
        "data": {"compatibility": compatibility, "resources": resources}});
    snapshot.ensure_unchanged(config)?;
    let path = config.output_dir.join("quality_profiles.json");
    write_json(&path, &document)?;
    ensure!(
        failed == 0,
        "{failed} quality profiles failed or were unavailable; evidence written to {}",
        path.display()
    );
    ensure!(
        !require_complete || reasons.is_empty(),
        "quality profile evidence is incomplete; written to {}",
        path.display()
    );
    Ok(path)
}

fn compiler_environment() -> BTreeMap<String, String> {
    [
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "RUSTC",
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
        "RUSTUP_TOOLCHAIN",
        "CARGO_BUILD_TARGET",
    ]
    .into_iter()
    .filter_map(|key| {
        std::env::var(key)
            .ok()
            .map(|value| (key.to_string(), value))
    })
    .collect()
}

fn compatibility_summary(rows: &[Value]) -> Value {
    let declared = rows.len();
    let passed = rows.iter().filter(|row| row["status"] == "passed").count();
    let executed = rows.iter().filter(|row| row["executed"] == true).count();
    let tests_declared = rows
        .iter()
        .filter(|row| row["profile"]["operation"] == "test")
        .count();
    let tests_passed = rows
        .iter()
        .filter(|row| row["profile"]["operation"] == "test" && row["status"] == "passed")
        .count();
    json!({"declared_profiles": declared, "executed_profiles": executed, "passed_profiles": passed,
        "compilation_confirmed_profiles": rows.iter().filter(|row| row["compilation_confirmed"] == true).count(),
        "test_profiles_declared": tests_declared, "test_profiles_passed": tests_passed,
        "declared_profiles_pass_percent": (declared > 0).then(|| round2(100.0 * passed as f64 / declared as f64)),
        "declared_test_profiles_pass_percent": (tests_declared > 0).then(|| round2(100.0 * tests_passed as f64 / tests_declared as f64))})
}

fn confidence(scope: &str, reasons: &[String]) -> Value {
    json!({"complete": reasons.is_empty(), "partial": !reasons.is_empty(), "confidence_scope": scope,
        "required_inputs": ["declared_profile", "fresh_execution"], "observed_inputs": {},
        "missing_input": reasons, "stale_input": [], "unsupported_pattern": []})
}

pub(crate) fn schema() -> Value {
    let confidence = json!({"type": "object", "required": ["complete", "partial", "confidence_scope"],
        "properties": {"complete": {"type": "boolean"}, "partial": {"type": "boolean"}, "confidence_scope": {"type": "string"}}});
    let row = json!({"type": "object", "required": ["id", "profile", "status", "measurement_confidence"],
        "properties": {"id": {"type": "string"}, "profile": {"type": "object"},
            "status": {"enum": ["passed", "failed", "unavailable", "timed-out", "skipped"]},
            "measurement_confidence": confidence}});
    json!({"$schema": "https://json-schema.org/draft/2020-12/schema", "title": "quality_profiles.json",
        "type": "object", "required": ["schema_version", "tool", "input_fingerprint", "environment", "measurement_confidence", "summary", "data"],
        "properties": {"schema_version": {"const": 1}, "tool": {"const": "quality-profiles"},
            "measurement_confidence": confidence, "summary": {"type": "object"}, "environment": {"type": "object"},
            "data": {"type": "object", "required": ["compatibility", "resources"], "properties": {
                "compatibility": {"type": "array", "items": row}, "resources": {"type": "array", "items": row}}}}})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn skipped_profiles_remain_in_the_declared_denominator() {
        let rows = vec![
            json!({"status": "passed", "executed": true, "compilation_confirmed": true, "profile": {"operation": "check"}}),
            json!({"status": "skipped", "executed": false, "profile": {"operation": "test"}}),
        ];
        let summary = compatibility_summary(&rows);
        assert_eq!(summary["declared_profiles_pass_percent"], 50.0);
        assert_eq!(summary["declared_test_profiles_pass_percent"], 0.0);
        assert_eq!(summary["test_profiles_passed"], 0);
        assert!(compatibility_summary(&[])["declared_profiles_pass_percent"].is_null());
    }
}
