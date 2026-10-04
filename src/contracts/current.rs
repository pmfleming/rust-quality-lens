//! Shared validation/freshness boundary for current (not historical) artifacts.
use serde_json::Value;
use std::path::Path;

pub(crate) enum ArtifactReadError {
    Missing,
    Malformed,
    Incompatible(String),
}

pub(crate) struct CurrentArtifact {
    pub(crate) document: Value,
    pub(crate) current: bool,
    pub(crate) complete: bool,
}

pub(crate) fn read_current(
    path: &Path,
    file: &str,
    fingerprint: &Value,
) -> Result<CurrentArtifact, ArtifactReadError> {
    let text = std::fs::read_to_string(path).map_err(|_| ArtifactReadError::Missing)?;
    let document = serde_json::from_str(&text).map_err(|_| ArtifactReadError::Malformed)?;
    validated_current(document, file, fingerprint)
        .map_err(|error| ArtifactReadError::Incompatible(error.to_string()))
}

pub(crate) fn validated_current(
    document: Value,
    file: &str,
    fingerprint: &Value,
) -> anyhow::Result<CurrentArtifact> {
    let header = super::validate_artifact(file, &document)?;
    Ok(CurrentArtifact {
        current: crate::fingerprint::freshness(&document, fingerprint)
            == crate::fingerprint::Freshness::Current,
        complete: header.measurement_confidence.is_complete(),
        document,
    })
}
