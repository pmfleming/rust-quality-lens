//! Stage a measurement batch so an input change or producer failure cannot
//! replace previous evidence with a partially completed run.
use anyhow::{Context, Result, ensure};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use crate::config::LensConfig;
use crate::tool::MeasureTool;
use crate::util::write_json;

pub(crate) struct ArtifactBatch {
    directory: tempfile::TempDir,
    destination: PathBuf,
    pub(crate) config: LensConfig,
    written: BTreeSet<String>,
    _lock: PublicationLock,
}

// Cooperative writers fail closed. A crashed process leaves this marker for
// explicit operator recovery; silently breaking another writer's lock is unsafe.
struct PublicationLock(PathBuf);
impl PublicationLock {
    fn acquire(directory: &std::path::Path) -> Result<Self> {
        let path = directory.join(".rqlens-batch.lock");
        fs::OpenOptions::new().write(true).create_new(true).open(&path)
            .with_context(|| format!("acquiring publication lock {}; another writer or interrupted run requires attention", path.display()))?;
        Ok(Self(path))
    }
}
impl Drop for PublicationLock {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_file(&self.0) {
            eprintln!(
                "cannot release publication lock {}: {error}",
                self.0.display()
            );
        }
    }
}

impl ArtifactBatch {
    pub(crate) fn new(config: &LensConfig) -> Result<Self> {
        fs::create_dir_all(&config.output_dir)?;
        let lock = PublicationLock::acquire(&config.output_dir)?;
        let directory = tempfile::Builder::new()
            .prefix(".rqlens-batch-")
            .tempdir_in(&config.output_dir)?;
        // Focused producers may depend on artifacts from an earlier run. They
        // still undergo normal schema, confidence, and snapshot validation.
        for file in MeasureTool::schema_tools()
            .iter()
            .map(MeasureTool::output_file)
        {
            let source = config.output_dir.join(file);
            match fs::copy(&source, directory.path().join(file)) {
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(error).with_context(|| format!("staging {}", source.display()));
                }
            }
        }
        let mut staged = config.clone();
        staged.output_dir = directory.path().to_path_buf();
        Ok(Self {
            directory,
            destination: config.output_dir.clone(),
            config: staged,
            written: BTreeSet::new(),
            _lock: lock,
        })
    }

    pub(crate) fn write(&mut self, file: &str, document: &serde_json::Value) -> Result<()> {
        ensure!(
            file.ends_with(".json") && !file.contains(['/', '\\', ':']),
            "artifact must be a JSON basename"
        );
        write_json(&self.directory.path().join(file), document)?;
        self.written.insert(file.to_string());
        Ok(())
    }

    pub(crate) fn publish(
        self,
        snapshot: &super::InputSnapshot,
        original: &LensConfig,
    ) -> Result<()> {
        snapshot.ensure_unchanged(original)?;
        for file in &self.written {
            let document =
                serde_json::from_str(&fs::read_to_string(self.directory.path().join(file))?)?;
            write_json(&self.destination.join(file), &document)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::ArtifactBatch;
    use crate::{test_support::source_project, util::write_json};
    use anyhow::Result;
    use serde_json::json;
    use std::fs;

    #[test]
    fn competing_writers_and_escaping_names_are_rejected() -> Result<()> {
        let (_root, config) = source_project(None)?;
        let mut batch = ArtifactBatch::new(&config)?;
        assert!(ArtifactBatch::new(&config).is_err());
        for name in [
            "../outside.json",
            "sub/artifact.json",
            "C:artifact.json",
            "sub\\artifact.json",
            "not-json",
        ] {
            assert!(batch.write(name, &json!({})).is_err());
        }
        drop(batch);
        assert!(ArtifactBatch::new(&config).is_ok());
        Ok(())
    }

    #[test]
    fn changed_inputs_leave_previous_artifacts_untouched() -> Result<()> {
        let (root, config) = source_project(Some("pub fn before() {}"))?;
        let source = root.path().join("src/lib.rs");
        let output = config.output_dir.join("hotspots.json");
        write_json(&output, &json!({"previous": true}))?;
        let snapshot = super::super::InputSnapshot::capture(&config);
        let mut batch = ArtifactBatch::new(&config)?;
        assert_eq!(snapshot.fingerprint, batch.config.input_fingerprint());
        batch.write("hotspots.json", &json!({"new": true}))?;
        fs::write(source, "pub fn changed() {}")?;
        assert!(batch.publish(&snapshot, &config).is_err());
        let retained: serde_json::Value = serde_json::from_str(&fs::read_to_string(output)?)?;
        assert_eq!(retained, json!({"previous": true}));
        Ok(())
    }
}
