use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashSet};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;
use walkdir::WalkDir;

pub(crate) struct FileLock {
    file: File,
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

pub(crate) fn lock_file(path: &Path) -> Result<FileLock> {
    let mut lock_path = path.to_path_buf();
    lock_path.add_extension("lock");
    if let Some(parent) = lock_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .with_context(|| format!("opening lock file {}", lock_path.display()))?;
    file.lock()
        .with_context(|| format!("locking {}", lock_path.display()))?;
    Ok(FileLock { file })
}

pub(crate) fn write_json(path: &Path, payload: &Value) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    writeln!(temporary, "{}", serde_json::to_string_pretty(payload)?)?;
    temporary.as_file_mut().sync_all()?;
    temporary.persist(path)?;
    Ok(())
}

pub(crate) fn repo_root() -> Result<PathBuf> {
    Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

pub(crate) fn bundled_helper_manifest() -> Result<PathBuf> {
    let checkout_manifest = repo_root()?.join("rust_helpers").join("Cargo.toml");
    if checkout_manifest.is_file() {
        return Ok(checkout_manifest);
    }
    let sources = [
        ("src/lib.rs", include_str!("bundled_helpers/lib.rs.txt")),
        (
            "src/bin/rust_facts.rs",
            include_str!("bundled_helpers/rust_facts.rs.txt"),
        ),
        (
            "src/bin/ast_hasher.rs",
            include_str!("bundled_helpers/ast_hasher.rs.txt"),
        ),
    ];
    let source_identity = sources
        .iter()
        .map(|(path, source)| format!("{path}\n{source}"))
        .collect::<String>();
    let root = helper_cache_root().join(format!(
        "{}-{}",
        env!("CARGO_PKG_VERSION"),
        stable_hash(&source_identity)
    ));
    let _cache_lock = lock_file(&root)?;
    fs::create_dir_all(root.join("src/bin"))?;
    write_if_changed(
        &root.join("Cargo.toml"),
        r#"[package]
name = "rust-quality-lens-helpers"
version = "0.0.0"
edition = "2024"
rust-version = "1.95"
publish = false

[dependencies]
proc-macro2 = { version = "1.0", features = ["span-locations"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
syn = { version = "2.0", features = ["full", "visit"] }
quote = "1.0"
tree-sitter = "=0.25.3"
tree-sitter-rust = "=0.23.2"
"#,
    )?;
    for (path, source) in sources {
        write_if_changed(&root.join(path), source)?;
    }
    Ok(root.join("Cargo.toml"))
}

fn helper_cache_root() -> PathBuf {
    env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .unwrap_or_else(env::temp_dir)
        .join("rqlens")
        .join("helpers")
}

fn write_if_changed(path: &Path, contents: &str) -> Result<()> {
    if fs::read_to_string(path).ok().as_deref() == Some(contents) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents).with_context(|| format!("writing bundled helper {}", path.display()))
}

pub(crate) fn resolve_config_path(path: PathBuf, config_dir: &Path) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        absolutize(config_dir.join(path))
    }
}

pub(crate) fn resolve_project_path(path: PathBuf, project_root: &Path) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        absolutize(project_root.join(path))
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
    format!("{:016x}", fnv1a(1469598103934665603, value.as_bytes()))
}

pub(crate) fn project_input_fingerprint(project_root: &Path, source_roots: &[String]) -> Value {
    let mut files = BTreeSet::new();
    for entry in WalkDir::new(project_root)
        .into_iter()
        .filter_entry(|entry| !ignored_input_directory(entry.path(), project_root))
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if path.is_file() && is_quality_input(path) {
            files.insert(absolutize(path));
        }
    }
    for path in iter_rust_files(source_roots) {
        files.insert(absolutize(path));
    }

    let mut hash = 1469598103934665603;
    let mut observed = 0usize;
    let mut read_errors = Vec::new();
    for path in files {
        let relative = path.strip_prefix(project_root).unwrap_or(&path);
        hash = fnv1a(hash, normalize_slashes(relative).as_bytes());
        hash = fnv1a(hash, &[0]);
        match fs::read(&path) {
            Ok(bytes) => {
                hash = fnv1a(hash, &bytes);
                observed += 1;
            }
            Err(error) => read_errors.push(format!("{}: {error}", path.display())),
        }
        hash = fnv1a(hash, &[0xff]);
    }
    json!({
        "algorithm": "fnv1a64-path-and-content-v1",
        "digest": format!("{hash:016x}"),
        "file_count": observed,
        "read_errors": read_errors,
        "complete": read_errors.is_empty(),
    })
}

fn fnv1a(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    hash
}

fn ignored_input_directory(path: &Path, project_root: &Path) -> bool {
    path != project_root
        && path
            .file_name()
            .is_some_and(|name| matches!(name.to_str(), Some(".git" | ".direnv" | "target")))
}

fn is_quality_input(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "rs")
        || matches!(
            path.file_name().and_then(|name| name.to_str()),
            Some(
                "Cargo.toml"
                    | "Cargo.lock"
                    | "rust-toolchain"
                    | "rust-toolchain.toml"
                    | "rqlens.toml"
                    | "deny.toml"
            )
        )
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
    paths
        .iter()
        .flat_map(rust_files_at)
        .filter(|path| seen.insert(absolutize(path)))
        .collect()
}

fn rust_files_at(raw_path: &String) -> Vec<PathBuf> {
    let path = PathBuf::from(raw_path);
    if is_rust_file(&path) {
        return vec![path];
    }
    if !path.is_dir() {
        return Vec::new();
    }
    WalkDir::new(path)
        .into_iter()
        .filter_entry(|entry| {
            entry.depth() == 0
                || !entry.file_type().is_dir()
                || !matches!(
                    entry.file_name().to_str(),
                    Some(".git" | ".direnv" | "target")
                )
        })
        .filter_map(Result::ok)
        .map(|entry| entry.into_path())
        .filter(|path| is_rust_file(path))
        .collect()
}

fn is_rust_file(path: &Path) -> bool {
    path.is_file()
        && path.extension().is_some_and(|extension| extension == "rs")
        && !normalize_slashes(path).contains("/bundled_helpers/")
}

#[cfg(test)]
mod tests {
    use super::{lock_file, project_input_fingerprint};

    #[test]
    fn file_locks_exclude_concurrent_writers() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let base = root.path().join("artifact-cache");
        let _guard = lock_file(&base)?;
        let mut lock_path = base;
        lock_path.add_extension("lock");
        let contender = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(lock_path)?;
        assert!(contender.try_lock().is_err());
        Ok(())
    }

    #[test]
    fn project_fingerprint_changes_with_quality_inputs_only() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        std::fs::create_dir_all(root.path().join("src"))?;
        std::fs::create_dir_all(root.path().join("target"))?;
        std::fs::write(root.path().join("Cargo.toml"), "[workspace]\n")?;
        std::fs::write(root.path().join("src/lib.rs"), "pub fn value() {}\n")?;
        let roots = vec![root.path().join("src").to_string_lossy().to_string()];
        let initial = project_input_fingerprint(root.path(), &roots);

        std::fs::write(root.path().join("target/ignored.rs"), "changed\n")?;
        assert_eq!(
            initial["digest"],
            project_input_fingerprint(root.path(), &roots)["digest"]
        );

        std::fs::write(root.path().join("src/lib.rs"), "pub fn changed() {}\n")?;
        assert_ne!(
            initial["digest"],
            project_input_fingerprint(root.path(), &roots)["digest"]
        );
        Ok(())
    }
}
