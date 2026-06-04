use std::path::Path;

use crate::util::normalize_slashes;

const LAYER_NEEDLES: &[(&str, &[&str])] = &[
    (
        "Interface",
        &[
            "api",
            "cli",
            "controller",
            "controllers",
            "handler",
            "handlers",
            "http",
            "route",
            "routes",
            "ui",
            "web",
        ],
    ),
    (
        "Application",
        &[
            "app",
            "application",
            "command",
            "commands",
            "service",
            "services",
            "use_case",
            "use_cases",
            "workflow",
            "workflows",
        ],
    ),
    (
        "Domain",
        &["core", "domain", "entity", "entities", "model", "models"],
    ),
    (
        "Infrastructure",
        &[
            "adapter",
            "adapters",
            "database",
            "db",
            "file",
            "fs",
            "infra",
            "infrastructure",
            "io",
            "persistence",
            "repository",
            "storage",
        ],
    ),
    ("Tests", &["spec", "specs", "test", "tests"]),
];

pub(crate) fn classify_path(path: impl AsRef<str>) -> &'static str {
    let path = path.as_ref().replace('\\', "/").to_lowercase();
    let segments = path
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    LAYER_NEEDLES
        .iter()
        .find(|(_, needles)| {
            needles
                .iter()
                .any(|needle| segments.iter().any(|segment| segment == needle))
        })
        .map(|(layer, _)| *layer)
        .unwrap_or("Unclassified")
}

pub(crate) fn classify_module(module: &str) -> &'static str {
    classify_path(module.replace("::", "/"))
}

pub(crate) fn layer_color(layer: &str) -> &'static str {
    match layer {
        "Entrypoint" => "#dcdcaa",
        "Interface" => "#569cd6",
        "Application" => "#d7ba7d",
        "Domain" => "#4ec9b0",
        "Infrastructure" => "#c586c0",
        "Tests" => "#9cdcfe",
        _ => "#808080",
    }
}

pub(crate) fn module_for_path(path: &str) -> String {
    let path = normalize_slashes(path);
    if let Some(src_path) = path.strip_prefix("src/") {
        src_path
            .trim_end_matches(".rs")
            .trim_end_matches("/mod")
            .replace('/', "::")
    } else {
        Path::new(&path)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
    }
}

pub(crate) fn test_kind_for_path(path: &str) -> &'static str {
    let normalized = normalize_slashes(path);
    if normalized.starts_with("tests/") {
        "integration"
    } else if normalized.starts_with("benches/") {
        "benchmark"
    } else if normalized.starts_with("examples/") {
        "example"
    } else if normalized.starts_with("src/bin/") {
        "binary"
    } else {
        "inline"
    }
}

pub(crate) fn title_from_name(name: &str) -> String {
    let stop = ["test", "should", "when"];
    let tokens = name
        .split(|ch: char| ch == '_' || !ch.is_alphanumeric())
        .filter(|token| !token.is_empty() && !stop.contains(token))
        .collect::<Vec<_>>();
    let useful = if tokens.is_empty() {
        vec!["behavior"]
    } else {
        tokens
    };
    format!(
        "Checks {}.",
        useful.into_iter().take(7).collect::<Vec<_>>().join(" ")
    )
}

pub(crate) fn project_relative_path(config_root: &Path, path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        config_root.join(path)
    };
    absolute
        .strip_prefix(config_root)
        .map(normalize_slashes)
        .unwrap_or_else(|_| normalize_slashes(absolute))
}
