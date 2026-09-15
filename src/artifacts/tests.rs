use super::read_artifact;
use crate::config::LensConfig;
use serde_json::json;
use std::fs;

#[test]
fn map_uses_content_fingerprints_instead_of_artifact_timestamps() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    fs::create_dir(root.path().join("src"))?;
    fs::write(root.path().join("src/lib.rs"), "pub fn value() {}")?;
    let config_path = root.path().join("rqlens.toml");
    fs::write(&config_path, "source_roots = [\"src\"]\n")?;
    let config = LensConfig::load(Some(config_path))?;
    let fingerprint = config.input_fingerprint();
    let mut document = json!({
        "schema_version": 3, "risk_model_version": crate::measurement::MODEL_VERSION,
        "tool": "hotspots", "records": [],
        "measurement_confidence": {"complete": true, "partial": false},
        "input_fingerprint": fingerprint,
    });
    let path = config.output_dir.join("hotspots.json");
    crate::util::write_json(&path, &document)?;
    assert_eq!(
        read_artifact(&config, "hotspots.json", true, &fingerprint).status,
        "available"
    );
    fs::remove_file(root.path().join("src/lib.rs"))?;
    let changed = config.input_fingerprint();
    // Rewriting the artifact gives it a newer mtime but does not make it fresh.
    crate::util::write_json(&path, &document)?;
    let read = read_artifact(&config, "hotspots.json", true, &changed);
    assert_eq!(read.status, "stale");
    assert!(read.value.is_none());
    document["input_fingerprint"] = changed.clone();
    document["input_fingerprint"]["complete"] = json!(false);
    crate::util::write_json(&path, &document)?;
    let read = read_artifact(&config, "hotspots.json", true, &changed);
    assert_eq!(read.status, "partial");
    assert!(read.value.is_none());
    Ok(())
}
