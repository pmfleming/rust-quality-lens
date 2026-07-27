use crate::util::{absolutize, bundled_helper_manifest, resolve_config_path, resolve_project_path};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) const CONFIG_FILE_NAME: &str = "rqlens.toml";

#[derive(Debug, Deserialize)]
struct RawConfig {
    project_name: Option<String>,
    project_root: Option<PathBuf>,
    source_roots: Option<Vec<String>>,
    output_dir: Option<PathBuf>,
    rust: Option<RawRustConfig>,
    verification: Option<RawVerificationConfig>,
    policy: Option<PolicyConfig>,
}

#[derive(Debug, Deserialize)]
struct RawRustConfig {
    helper_manifest: Option<PathBuf>,
    identity_resolution: Option<SemanticIdentityMode>,
    rust_analyzer: Option<PathBuf>,
    identity_timeout_seconds: Option<u64>,
    identity_offline: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
struct RawVerificationConfig {
    timeout_seconds: Option<u64>,
    workspace: Option<bool>,
    all_targets: Option<bool>,
    all_features: Option<bool>,
    no_default_features: Option<bool>,
    features: Option<Vec<String>>,
    exclude: Option<Vec<String>>,
    audit: Option<bool>,
    deny: Option<bool>,
    semver: Option<bool>,
    semver_baseline_rev: Option<String>,
    feature_matrix: Option<bool>,
    miri: Option<bool>,
}

#[derive(Clone, Debug)]
pub(crate) struct VerificationConfig {
    pub(crate) timeout_seconds: u64,
    pub(crate) workspace: bool,
    pub(crate) all_targets: bool,
    pub(crate) all_features: bool,
    pub(crate) no_default_features: bool,
    pub(crate) features: Vec<String>,
    pub(crate) exclude: Vec<String>,
    pub(crate) audit: bool,
    pub(crate) deny: bool,
    pub(crate) semver: bool,
    pub(crate) semver_baseline_rev: Option<String>,
    pub(crate) feature_matrix: bool,
    pub(crate) miri: bool,
}

impl Default for VerificationConfig {
    fn default() -> Self {
        Self {
            timeout_seconds: 600,
            workspace: true,
            all_targets: true,
            all_features: false,
            no_default_features: false,
            features: Vec::new(),
            exclude: Vec::new(),
            audit: false,
            deny: false,
            semver: false,
            semver_baseline_rev: None,
            feature_matrix: false,
            miri: false,
        }
    }
}

impl VerificationConfig {
    pub(crate) fn cargo_arguments(
        &self,
        command: &str,
        include_targets: bool,
        doc_tests: bool,
    ) -> Vec<String> {
        let mut arguments = vec![command.to_string()];
        if self.workspace {
            arguments.push("--workspace".to_string());
        }
        if include_targets && self.all_targets {
            arguments.push("--all-targets".to_string());
        }
        if self.all_features {
            arguments.push("--all-features".to_string());
        } else {
            if self.no_default_features {
                arguments.push("--no-default-features".to_string());
            }
            if !self.features.is_empty() {
                arguments.extend(["--features".to_string(), self.features.join(",")]);
            }
        }
        if self.workspace {
            for package in &self.exclude {
                arguments.extend(["--exclude".to_string(), package.clone()]);
            }
        }
        if doc_tests {
            arguments.push("--doc".to_string());
        }
        if command == "doc" {
            arguments.push("--no-deps".to_string());
        }
        arguments
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct PolicyConfig {
    #[serde(default)]
    pub(crate) rules: BTreeMap<String, PolicyRule>,
    #[serde(default)]
    pub(crate) waivers: Vec<PolicyWaiver>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum PolicyRuleLevel {
    Off,
    Advisory,
    Warning,
    #[default]
    Error,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct PolicyRule {
    #[serde(default)]
    pub(crate) level: PolicyRuleLevel,
    #[serde(default)]
    pub(crate) max: usize,
    #[serde(default)]
    pub(crate) exclude_paths: Vec<String>,
    #[serde(default)]
    pub(crate) exclude_packages: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct PolicyWaiver {
    pub(crate) rule_id: String,
    #[serde(default)]
    pub(crate) path: Option<String>,
    pub(crate) reason: String,
    pub(crate) owner: String,
    pub(crate) expires: String,
}

impl PolicyConfig {
    pub(crate) fn active_waiver(&self, rule_id: &str, path: Option<&str>) -> Option<&PolicyWaiver> {
        self.waivers
            .iter()
            .find(|waiver| waiver.matches(rule_id, path) && !waiver.expired())
    }

    pub(crate) fn expired_waivers(&self) -> Vec<&PolicyWaiver> {
        self.waivers
            .iter()
            .filter(|waiver| waiver.expired())
            .collect()
    }

    fn validate(&self) -> Result<()> {
        for (rule_id, rule) in &self.rules {
            if rule_id.trim().is_empty() {
                bail!("policy rule IDs must not be empty");
            }
            if rule
                .exclude_paths
                .iter()
                .chain(&rule.exclude_packages)
                .any(|value| value.trim().is_empty())
            {
                bail!("policy rule {rule_id} has an empty exclusion");
            }
        }
        for waiver in &self.waivers {
            if waiver.rule_id.trim().is_empty()
                || waiver.reason.trim().is_empty()
                || waiver.owner.trim().is_empty()
            {
                bail!("policy waivers require non-empty rule_id, reason, and owner");
            }
            chrono::NaiveDate::parse_from_str(&waiver.expires, "%Y-%m-%d").with_context(|| {
                format!(
                    "policy waiver {} has invalid expiry {}; expected YYYY-MM-DD",
                    waiver.rule_id, waiver.expires
                )
            })?;
        }
        Ok(())
    }
}

impl PolicyRule {
    pub(crate) fn includes(&self, path: Option<&str>, package: Option<&str>) -> bool {
        !self
            .exclude_paths
            .iter()
            .any(|pattern| path_pattern_matches(pattern, path))
            && !self
                .exclude_packages
                .iter()
                .any(|excluded| Some(excluded.as_str()) == package)
    }
}

impl PolicyRuleLevel {
    pub(crate) fn as_str(self) -> &'static str {
        const LEVEL_NAMES: [&str; 4] = ["off", "advisory", "warning", "error"];
        LEVEL_NAMES[self as usize]
    }
}

impl PolicyWaiver {
    fn matches(&self, rule_id: &str, path: Option<&str>) -> bool {
        self.rule_id == rule_id
            && self
                .path
                .as_deref()
                .is_none_or(|pattern| path_pattern_matches(pattern, path))
    }

    fn expired(&self) -> bool {
        chrono::NaiveDate::parse_from_str(&self.expires, "%Y-%m-%d")
            .map_or(true, |expiry| expiry < chrono::Utc::now().date_naive())
    }
}

fn path_pattern_matches(pattern: &str, path: Option<&str>) -> bool {
    let pattern = pattern.replace('\\', "/");
    let path = path.unwrap_or_default().replace('\\', "/");
    pattern
        .strip_suffix("/**")
        .map_or(path == pattern, |prefix| {
            let prefix = prefix.trim_end_matches('/');
            path == prefix
                || path
                    .strip_prefix(prefix)
                    .is_some_and(|remainder| remainder.starts_with('/'))
        })
}

impl From<Option<RawVerificationConfig>> for VerificationConfig {
    fn from(raw: Option<RawVerificationConfig>) -> Self {
        let raw = raw.unwrap_or_default();
        let defaults = Self::default();
        Self {
            timeout_seconds: raw
                .timeout_seconds
                .unwrap_or(defaults.timeout_seconds)
                .max(1),
            workspace: raw.workspace.unwrap_or(defaults.workspace),
            all_targets: raw.all_targets.unwrap_or(defaults.all_targets),
            all_features: raw.all_features.unwrap_or(defaults.all_features),
            no_default_features: raw
                .no_default_features
                .unwrap_or(defaults.no_default_features),
            features: raw.features.unwrap_or_default(),
            exclude: raw.exclude.unwrap_or_default(),
            audit: raw.audit.unwrap_or_default(),
            deny: raw.deny.unwrap_or_default(),
            semver: raw.semver.unwrap_or_default(),
            semver_baseline_rev: raw.semver_baseline_rev,
            feature_matrix: raw.feature_matrix.unwrap_or_default(),
            miri: raw.miri.unwrap_or_default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SemanticIdentityMode {
    #[default]
    Auto,
    Required,
    Disabled,
}

#[derive(Clone, Debug)]
pub(crate) struct LensConfig {
    pub(crate) project_name: String,
    pub(crate) project_root: PathBuf,
    pub(crate) source_roots: Vec<String>,
    pub(crate) output_dir: PathBuf,
    pub(crate) helper_manifest: PathBuf,
    pub(crate) identity_resolution: SemanticIdentityMode,
    pub(crate) rust_analyzer: PathBuf,
    pub(crate) identity_timeout_seconds: u64,
    pub(crate) identity_offline: bool,
    pub(crate) verification: VerificationConfig,
    pub(crate) policy: PolicyConfig,
}

impl LensConfig {
    pub(crate) fn load(path: Option<PathBuf>) -> Result<Self> {
        let config_path = resolve_config_input(path)?;
        let config_dir = config_path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .unwrap_or(std::env::current_dir()?);
        let raw = match &config_path {
            Some(path) => toml::from_str::<RawConfig>(
                &fs::read_to_string(path)
                    .with_context(|| format!("reading config {}", path.display()))?,
            )?,
            _ => RawConfig {
                project_name: None,
                project_root: None,
                source_roots: None,
                output_dir: None,
                rust: None,
                verification: None,
                policy: None,
            },
        };
        let project_root =
            resolve_config_path(raw.project_root.unwrap_or_else(|| ".".into()), &config_dir);
        let rust = raw.rust;
        let identity_offline = rust
            .as_ref()
            .and_then(|rust| rust.identity_offline)
            .unwrap_or(true);
        let source_roots = raw.source_roots.map_or_else(
            || discover_rust_source_roots(&project_root, identity_offline),
            |roots| {
                roots
                    .into_iter()
                    .map(|path| {
                        resolve_project_path(PathBuf::from(path), &project_root)
                            .to_string_lossy()
                            .to_string()
                    })
                    .collect()
            },
        );
        let output_dir = resolve_project_path(
            raw.output_dir.unwrap_or_else(|| "target/analysis".into()),
            &project_root,
        );
        let default_helper_manifest = bundled_helper_manifest()?;
        let helper_manifest = resolve_project_path(
            rust.as_ref()
                .and_then(|rust| rust.helper_manifest.clone())
                .unwrap_or(default_helper_manifest),
            &project_root,
        );
        let identity_resolution = rust
            .as_ref()
            .and_then(|rust| rust.identity_resolution)
            .unwrap_or_default();
        let rust_analyzer = rust
            .as_ref()
            .and_then(|rust| rust.rust_analyzer.clone())
            .unwrap_or_else(|| PathBuf::from("rust-analyzer"));
        let identity_timeout_seconds = rust
            .as_ref()
            .and_then(|rust| rust.identity_timeout_seconds)
            .unwrap_or(60)
            .max(1);
        let verification = VerificationConfig::from(raw.verification);
        let policy = raw.policy.unwrap_or_default();
        policy.validate()?;
        let project_name = raw.project_name.unwrap_or_else(|| {
            project_root
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });
        Ok(Self {
            project_name,
            project_root,
            source_roots,
            output_dir,
            helper_manifest,
            identity_resolution,
            rust_analyzer,
            identity_timeout_seconds,
            identity_offline,
            verification,
            policy,
        })
    }
}

fn discover_rust_source_roots(project_root: &Path, offline: bool) -> Vec<String> {
    let roots = cargo_metadata(project_root, offline, false)
        .or_else(|| cargo_metadata(project_root, offline, true))
        .map(|metadata| metadata_source_roots(project_root, &metadata))
        .unwrap_or_default();
    if roots.is_empty() {
        vec![project_root.join("src").to_string_lossy().to_string()]
    } else {
        roots
            .into_iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect()
    }
}

fn cargo_metadata(project_root: &Path, offline: bool, no_dependencies: bool) -> Option<Value> {
    let mut command = Command::new("cargo");
    command
        .args(["metadata", "--format-version", "1"])
        .current_dir(project_root);
    if offline {
        command.arg("--offline");
    }
    if no_dependencies {
        command.arg("--no-deps");
    }
    command
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| serde_json::from_slice(&output.stdout).ok())
}

fn metadata_source_roots(project_root: &Path, metadata: &Value) -> BTreeSet<PathBuf> {
    let project_root = absolutize(project_root);
    metadata["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|package| {
            let manifest = PathBuf::from(package["manifest_path"].as_str()?);
            let package_root = manifest.parent()?.to_path_buf();
            absolutize(&package_root)
                .starts_with(&project_root)
                .then_some((package, package_root))
        })
        .flat_map(|(package, package_root)| {
            let conventional = package_root.join("src");
            let mut roots = BTreeSet::new();
            if conventional.is_dir() {
                roots.insert(conventional);
            }
            for target in package["targets"].as_array().into_iter().flatten() {
                let Some(source) = target["src_path"].as_str().map(PathBuf::from) else {
                    continue;
                };
                if !source.starts_with(package_root.join("src")) {
                    roots.insert(source);
                }
            }
            roots
        })
        .collect()
}

pub(crate) fn discover_config(start: impl AsRef<Path>) -> Option<PathBuf> {
    let mut dir = absolutize(start);
    if dir.is_file() {
        dir.pop();
    }
    loop {
        let candidate = dir.join(CONFIG_FILE_NAME);
        if candidate.exists() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn resolve_config_input(path: Option<PathBuf>) -> Result<Option<PathBuf>> {
    if let Some(path) = path {
        let path = absolutize(path);
        if !path.exists() {
            bail!("config file does not exist: {}", path.display());
        }
        return Ok(Some(path));
    }
    Ok(discover_config(std::env::current_dir()?).map(absolutize))
}

pub(crate) fn write_default_config(path: Option<PathBuf>, force: bool) -> Result<PathBuf> {
    let target = absolutize(path.unwrap_or_else(|| CONFIG_FILE_NAME.into()));
    if target.exists() && !force {
        bail!(
            "config already exists at {}; pass --force to overwrite it",
            target.display()
        );
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let project_root = target
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or(std::env::current_dir()?);
    let project_name = project_root
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    fs::write(
        &target,
        format!(
            r#"project_name = "{project_name}"
project_root = "."
# source_roots is optional; Cargo workspace and local path packages are discovered automatically.
output_dir = "target/analysis"

[rust]
identity_resolution = "auto"
rust_analyzer = "rust-analyzer"
identity_timeout_seconds = 60
identity_offline = true

[verification]
timeout_seconds = 600
workspace = true
all_targets = true
all_features = false
audit = false
deny = false
semver = false
feature_matrix = false
miri = false

# Stable rule limits prevent new findings while allowing an explicit baseline:
# [policy.rules."rust.reliability.expect"]
# level = "error"
# max = 0
# exclude_paths = ["src/generated/**"]
# exclude_packages = ["generated-bindings"]

# Temporary exceptions must be owned, justified, and time-bounded:
# [[policy.waivers]]
# rule_id = "rust.project.license"
# reason = "Owner license decision is pending"
# owner = "project-maintainers"
# expires = "2026-12-31"
"#
        ),
    )?;
    Ok(target)
}

pub(crate) fn config_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://example.local/rqlens.schema.json",
        "title": "rust-quality-lens config",
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "project_name": {"type": "string"},
            "project_root": {"type": "string", "default": "."},
            "source_roots": {
                "type": "array",
                "items": {"type": "string"},
                "description": "Optional explicit roots. When omitted, local Cargo package roots are discovered automatically."
            },
            "output_dir": {"type": "string", "default": "target/analysis"},
            "rust": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "helper_manifest": {"type": "string"},
                    "identity_resolution": {"type": "string", "enum": ["auto", "required", "disabled"], "default": "auto"},
                    "rust_analyzer": {"type": "string", "default": "rust-analyzer"},
                    "identity_timeout_seconds": {"type": "integer", "minimum": 1, "default": 60},
                    "identity_offline": {"type": "boolean", "default": true}
                }
            },
            "verification": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "timeout_seconds": {"type": "integer", "minimum": 1, "default": 600},
                    "workspace": {"type": "boolean", "default": true},
                    "all_targets": {"type": "boolean", "default": true},
                    "all_features": {"type": "boolean", "default": false},
                    "no_default_features": {"type": "boolean", "default": false},
                    "features": {"type": "array", "items": {"type": "string"}, "default": []},
                    "exclude": {"type": "array", "items": {"type": "string"}, "default": []},
                    "audit": {"type": "boolean", "default": false},
                    "deny": {"type": "boolean", "default": false},
                    "semver": {"type": "boolean", "default": false},
                    "semver_baseline_rev": {"type": "string"},
                    "feature_matrix": {"type": "boolean", "default": false},
                    "miri": {"type": "boolean", "default": false}
                }
            },
            "policy": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "rules": {
                        "type": "object",
                        "additionalProperties": {
                            "type": "object",
                            "additionalProperties": false,
                            "properties": {
                                "level": {"type": "string", "enum": ["off", "advisory", "warning", "error"], "default": "error"},
                                "max": {"type": "integer", "minimum": 0, "default": 0},
                                "exclude_paths": {"type": "array", "items": {"type": "string"}, "default": []},
                                "exclude_packages": {"type": "array", "items": {"type": "string"}, "default": []}
                            }
                        },
                        "default": {}
                    },
                    "waivers": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "additionalProperties": false,
                            "required": ["rule_id", "reason", "owner", "expires"],
                            "properties": {
                                "rule_id": {"type": "string"},
                                "path": {"type": "string"},
                                "reason": {"type": "string"},
                                "owner": {"type": "string"},
                                "expires": {"type": "string", "format": "date"}
                            }
                        },
                        "default": []
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{PolicyConfig, PolicyRule, PolicyRuleLevel, PolicyWaiver, metadata_source_roots};
    use serde_json::json;
    use std::collections::BTreeMap;

    #[test]
    fn policy_waivers_are_scoped_and_time_bounded() {
        let policy = PolicyConfig {
            rules: BTreeMap::new(),
            waivers: vec![
                PolicyWaiver {
                    rule_id: "rust.project.license".to_string(),
                    path: None,
                    reason: "decision pending".to_string(),
                    owner: "maintainers".to_string(),
                    expires: "2099-12-31".to_string(),
                },
                PolicyWaiver {
                    rule_id: "rust.reliability.unwrap".to_string(),
                    path: Some("src/generated/**".to_string()),
                    reason: "generated code".to_string(),
                    owner: "codegen-team".to_string(),
                    expires: "2000-01-01".to_string(),
                },
            ],
        };
        assert!(policy.active_waiver("rust.project.license", None).is_some());
        assert!(
            policy
                .active_waiver("rust.reliability.unwrap", Some("src/generated/model.rs"))
                .is_none()
        );
        assert_eq!(policy.expired_waivers().len(), 1);
    }

    #[test]
    fn policy_rules_support_path_and_package_exclusions() {
        let rule = PolicyRule {
            level: PolicyRuleLevel::Error,
            max: 0,
            exclude_paths: vec!["src/generated/**".to_string()],
            exclude_packages: vec!["generated-bindings".to_string()],
        };
        assert!(!rule.includes(Some("src/generated/model.rs"), Some("application")));
        assert!(rule.includes(Some("src/generated_code.rs"), Some("application")));
        assert!(!rule.includes(Some("src/lib.rs"), Some("generated-bindings")));
        assert!(rule.includes(Some("src/lib.rs"), Some("application")));
    }

    #[test]
    fn cargo_metadata_discovers_local_package_source_roots() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let application = root.path();
        let helper = root.path().join("crates/helper");
        std::fs::create_dir_all(application.join("src"))?;
        std::fs::create_dir_all(helper.join("src"))?;
        let metadata = json!({
            "packages": [
                {
                    "manifest_path": application.join("Cargo.toml"),
                    "targets": [{"src_path": application.join("src/lib.rs")}]
                },
                {
                    "manifest_path": helper.join("Cargo.toml"),
                    "targets": [{"src_path": helper.join("src/lib.rs")}]
                },
                {
                    "manifest_path": "/outside/registry/dependency/Cargo.toml",
                    "targets": [{"src_path": "/outside/registry/dependency/src/lib.rs"}]
                }
            ]
        });
        let roots = metadata_source_roots(application, &metadata);
        assert!(roots.contains(&application.join("src")));
        assert!(roots.contains(&helper.join("src")));
        assert_eq!(roots.len(), 2);
        Ok(())
    }
}
