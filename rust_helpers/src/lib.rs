use std::env;
use std::fs;
use std::path::Path;

pub fn read_paths_file(path: &str) -> Result<Vec<String>, String> {
    let content = fs::read_to_string(path)
        .map_err(|error| format!("error reading paths file '{path}': {error}"))?;
    Ok(content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}

pub fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}

pub fn module_key_for_path(path: &str) -> String {
    let normalized = normalize_path(path);
    let mut rel = normalized.as_str();
    if let Some(roots) = env::var_os("RQLENS_SOURCE_ROOTS") {
        for root in env::split_paths(&roots) {
            let root = normalize_path(&root.to_string_lossy());
            let root = root.trim_end_matches('/');
            if let Some(rest) = normalized.strip_prefix(&format!("{root}/")) {
                rel = rest;
                break;
            }
        }
    }
    let without_src = rel.strip_prefix("src/").unwrap_or(rel);
    let without_extension = without_src.strip_suffix(".rs").unwrap_or(without_src);
    Path::new(without_extension)
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .filter(|part| *part != "mod")
        .collect::<Vec<_>>()
        .join("::")
}

pub fn stable_hash(text: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in text.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}
