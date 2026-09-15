use anyhow::{Result, ensure};
use serde_json::Value;

use crate::config::LensConfig;
use crate::toolchain::ToolchainSnapshot;

pub(crate) struct InputSnapshot {
    pub(crate) fingerprint: Value,
    pub(crate) toolchain: ToolchainSnapshot,
    pub(crate) output_dir: std::path::PathBuf,
}

impl InputSnapshot {
    pub(crate) fn capture(config: &LensConfig) -> Self {
        let toolchain = crate::toolchain::snapshot(&config.project_root);
        let fingerprint = crate::fingerprint::project_with_toolchain(config, &toolchain);
        Self {
            fingerprint,
            toolchain,
            output_dir: config.output_dir.clone(),
        }
    }

    pub(crate) fn ensure_unchanged(&self, config: &LensConfig) -> Result<()> {
        let current = config.input_fingerprint();
        // Stable incomplete inputs are still useful partial evidence. A change
        // in either contents or observability must never be certified instead.
        ensure!(
            current == self.fingerprint,
            "quality inputs changed during measurement; results were not published, rerun against stable inputs"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn snapshot_detects_mid_run_edits_and_explicit_non_rust_inputs() -> Result<()> {
        let root = tempfile::tempdir()?;
        fs::create_dir(root.path().join("src"))?;
        fs::create_dir(root.path().join("assets"))?;
        let source = root.path().join("src/lib.rs");
        fs::write(&source, "pub fn before() {}")?;
        let asset = root.path().join("assets/schema.json");
        fs::write(&asset, "before")?;
        let path = root.path().join("rqlens.toml");
        fs::write(
            &path,
            "source_roots = [\"src\"]\nfingerprint_inputs = [\"assets\"]\n",
        )?;
        let config = LensConfig::load(Some(path))?;
        for path in [&source, &asset] {
            let snapshot = InputSnapshot::capture(&config);
            snapshot.ensure_unchanged(&config)?;
            let modified = fs::metadata(path)?.modified()?;
            fs::write(path, fs::read_to_string(path)?.replace("before", "after_"))?;
            fs::File::options()
                .write(true)
                .open(path)?
                .set_times(fs::FileTimes::new().set_modified(modified))?;
            assert!(snapshot.ensure_unchanged(&config).is_err());
        }
        let mut missing = config.clone();
        missing.fingerprint_inputs.push(
            root.path()
                .join("missing.json")
                .to_string_lossy()
                .into_owned(),
        );
        let snapshot = InputSnapshot::capture(&missing);
        assert_eq!(snapshot.fingerprint["complete"], false);
        snapshot.ensure_unchanged(&missing)?;
        Ok(())
    }
}
