//! Shared temporary source projects for evidence/freshness tests.
use crate::config::LensConfig;
use anyhow::Result;
use std::fs;
use tempfile::TempDir;

/// `None` deliberately leaves the configured source directory absent.
/// Keep the returned directory alive for the lifetime of the configuration.
pub(crate) fn source_project(source: Option<&str>) -> Result<(TempDir, LensConfig)> {
    let root = tempfile::tempdir()?;
    if let Some(source) = source {
        fs::create_dir(root.path().join("src"))?;
        fs::write(root.path().join("src/lib.rs"), source)?;
    }
    let path = root.path().join("rqlens.toml");
    fs::write(&path, "source_roots = [\"src\"]\n")?;
    let config = LensConfig::load(Some(path))?;
    Ok((root, config))
}
