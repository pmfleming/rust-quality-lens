//! Shared public CLI measurement boundary for native integration fixtures.
use anyhow::Result;
use serde_json::Value;
use std::{fs, path::Path, process::Command};

pub fn measure(root: &Path, tool: &str, artifact: &str) -> Result<Value> {
    let output = Command::new(env!("CARGO_BIN_EXE_rqlens"))
        .args(["measure", tool, "--config"])
        .arg(root.join("rqlens.toml"))
        .output()?;
    anyhow::ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(serde_json::from_slice(&fs::read(
        root.join("target/analysis").join(artifact),
    )?)?)
}
