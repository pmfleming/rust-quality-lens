//! Strict Cargo JSON artifact decoding shared by execution-backed measurements.
use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;

#[derive(Deserialize)]
pub(crate) struct CargoArtifact {
    pub(crate) package_id: String,
    pub(crate) target: CargoTarget,
    pub(crate) manifest_path: Option<PathBuf>,
    pub(crate) executable: Option<PathBuf>,
    #[serde(default)]
    pub(crate) filenames: Vec<PathBuf>,
    pub(crate) profile: CargoProfile,
}

#[derive(Deserialize)]
pub(crate) struct CargoTarget {
    pub(crate) name: String,
    pub(crate) kind: Vec<String>,
}

#[derive(Deserialize)]
pub(crate) struct CargoProfile {
    #[serde(default)]
    pub(crate) test: bool,
}

pub(crate) fn cargo_artifacts(stdout: &str) -> Result<Vec<CargoArtifact>> {
    stdout
        .lines()
        .map(|line| {
            let message: Value = serde_json::from_str(line).context("invalid Cargo JSON")?;
            (message["reason"] == "compiler-artifact")
                .then(|| serde_json::from_value(message).context("invalid Cargo compiler artifact"))
                .transpose()
        })
        .filter_map(Result::transpose)
        .collect()
}
