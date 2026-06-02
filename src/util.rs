use anyhow::Result;
use serde_json::Value;
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub(crate) fn write_json(path: &Path, payload: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(payload)?),
    )?;
    Ok(())
}

pub(crate) fn repo_root() -> Result<PathBuf> {
    Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

pub(crate) fn resolve_config_path(path: PathBuf, config_dir: &Path) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        absolutize(&config_dir.join(path))
    }
}

pub(crate) fn resolve_project_path(path: PathBuf, project_root: &Path) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        absolutize(&project_root.join(path))
    }
}

pub(crate) fn absolutize(path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

pub(crate) fn normalize_slashes(path: impl AsRef<Path>) -> String {
    path.as_ref().to_string_lossy().replace('\\', "/")
}

pub(crate) fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

pub(crate) fn over_free(value: f64, free: f64, weight: f64, cap: f64) -> f64 {
    ((value - free).max(0.0) * weight).min(cap)
}

pub(crate) fn stable_hash(value: &str) -> String {
    let mut hash: u64 = 1469598103934665603;
    for byte in value.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    format!("{hash:016x}")
}

pub(crate) fn tail(text: &str, lines: usize) -> String {
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

pub(crate) fn dedupe(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

pub(crate) fn iter_rust_files(paths: &[String]) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut files = Vec::new();
    for raw_path in paths {
        let path = PathBuf::from(raw_path);
        if path.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
            if seen.insert(absolutize(&path)) {
                files.push(path);
            }
        } else if path.is_dir() {
            for entry in WalkDir::new(&path)
                .into_iter()
                .filter_map(|entry| entry.ok())
            {
                let entry_path = entry.path();
                if entry_path.is_file() && entry_path.extension().is_some_and(|ext| ext == "rs") {
                    if seen.insert(absolutize(entry_path)) {
                        files.push(entry_path.to_path_buf());
                    }
                }
            }
        }
    }
    files
}
