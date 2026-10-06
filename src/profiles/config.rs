use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct ProfileConfig {
    pub(crate) compatibility: Vec<CompatibilityProfile>,
    pub(crate) resources: Vec<ResourceProfile>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Operation {
    #[default]
    Check,
    Test,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CompatibilityProfile {
    pub(crate) id: String,
    #[serde(default = "active")]
    pub(crate) toolchain: String,
    pub(crate) host_os: Option<String>,
    pub(crate) target: Option<String>,
    #[serde(default)]
    pub(crate) operation: Operation,
    #[serde(default)]
    pub(crate) no_default_features: bool,
    #[serde(default)]
    pub(crate) all_features: bool,
    #[serde(default)]
    pub(crate) features: Vec<String>,
}
fn active() -> String {
    "active".into()
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ResourceKind {
    Build,
    Command,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ResourceProfile {
    pub(crate) id: String,
    pub(crate) kind: ResourceKind,
    #[serde(default)]
    pub(crate) command: Vec<String>,
    #[serde(default = "one")]
    pub(crate) runs: usize,
    #[serde(default)]
    pub(crate) release: bool,
    pub(crate) target: Option<String>,
    #[serde(default)]
    pub(crate) peak_rss: bool,
    #[serde(default)]
    pub(crate) instrumented_metrics: Vec<String>,
    #[serde(default)]
    pub(crate) budgets: BTreeMap<String, u64>,
}
fn one() -> usize {
    1
}

pub(super) const INSTRUMENTED: &[&str] = &[
    "allocations",
    "allocated_bytes",
    "instructions",
    "cache_misses",
];
pub(super) const METRICS: &[&str] = &[
    "elapsed_ms",
    "peak_rss_bytes",
    "artifact_bytes",
    "allocations",
    "allocated_bytes",
    "instructions",
    "cache_misses",
];

fn token(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}

impl ProfileConfig {
    pub(crate) fn validate(&self) -> Result<()> {
        let mut ids = BTreeSet::new();
        for id in self
            .compatibility
            .iter()
            .map(|profile| &profile.id)
            .chain(self.resources.iter().map(|profile| &profile.id))
        {
            ensure!(
                token(id) && ids.insert(id),
                "profile IDs must be unique non-empty ASCII tokens"
            );
        }
        for profile in &self.compatibility {
            ensure!(token(&profile.toolchain), "invalid profile toolchain");
            ensure!(
                profile.host_os.as_deref().is_none_or(token)
                    && profile.target.as_deref().is_none_or(token),
                "invalid profile host_os or target"
            );
            ensure!(
                !(profile.all_features
                    && (profile.no_default_features || !profile.features.is_empty())),
                "all_features cannot be combined with explicit feature selection in a compatibility profile"
            );
            ensure!(
                profile
                    .features
                    .iter()
                    .all(|name| !name.trim().is_empty() && !name.starts_with('-')),
                "invalid feature name"
            );
        }
        for profile in &self.resources {
            ensure!(
                (1..=100).contains(&profile.runs),
                "resource runs must be in 1..=100"
            );
            ensure!(
                profile.target.as_deref().is_none_or(token),
                "invalid resource target"
            );
            match profile.kind {
                ResourceKind::Command => {
                    ensure!(
                        profile.command.first().is_some_and(|arg| !arg.is_empty()),
                        "command resource profiles need a command argv"
                    );
                    ensure!(
                        !profile.release && profile.target.is_none(),
                        "release and target apply only to build profiles"
                    );
                }
                ResourceKind::Build => ensure!(
                    profile.command.is_empty()
                        && profile.instrumented_metrics.is_empty()
                        && profile.runs == 1,
                    "build profiles use one cold/warm pair, no custom command or instrumented metrics"
                ),
            }
            let unique: BTreeSet<_> = profile.instrumented_metrics.iter().collect();
            ensure!(
                unique.len() == profile.instrumented_metrics.len()
                    && profile
                        .instrumented_metrics
                        .iter()
                        .all(|metric| INSTRUMENTED.contains(&metric.as_str())),
                "unsupported or duplicate instrumented metric"
            );
            for metric in profile.budgets.keys() {
                ensure!(
                    METRICS.contains(&metric.as_str()),
                    "unsupported resource budget metric {metric}"
                );
                ensure!(
                    metric != "peak_rss_bytes" || profile.peak_rss,
                    "peak_rss_bytes budget requires peak_rss=true"
                );
                ensure!(
                    metric != "artifact_bytes" || profile.kind == ResourceKind::Build,
                    "artifact_bytes budget requires a build profile"
                );
                ensure!(
                    !INSTRUMENTED.contains(&metric.as_str())
                        || profile.instrumented_metrics.contains(metric),
                    "budget requires requested instrumentation for {metric}"
                );
            }
        }
        Ok(())
    }
}

pub(crate) fn schema() -> Value {
    json!({"type": "object", "additionalProperties": false, "properties": {
        "compatibility": {"type": "array", "default": [], "items": {"type": "object", "additionalProperties": false,
            "required": ["id"], "properties": {
                "id": {"type": "string", "minLength": 1}, "toolchain": {"type": "string", "default": "active", "description": "active, msrv, or an installed rustup toolchain"},
                "host_os": {"type": "string"}, "target": {"type": "string"}, "operation": {"enum": ["check", "test"], "default": "check"},
                "no_default_features": {"type": "boolean", "default": false}, "all_features": {"type": "boolean", "default": false},
                "features": {"type": "array", "items": {"type": "string"}, "default": []}}}},
        "resources": {"type": "array", "default": [], "items": {"type": "object", "additionalProperties": false,
            "required": ["id", "kind"], "properties": {
                "id": {"type": "string", "minLength": 1}, "kind": {"enum": ["build", "command"]},
                "command": {"type": "array", "items": {"type": "string"}, "default": []}, "runs": {"type": "integer", "minimum": 1, "maximum": 100, "default": 1},
                "release": {"type": "boolean", "default": false}, "target": {"type": "string"},
                "peak_rss": {"type": "boolean", "default": false},
                "instrumented_metrics": {"type": "array", "uniqueItems": true, "items": {"enum": INSTRUMENTED}, "default": []},
                "budgets": {"type": "object", "propertyNames": {"enum": METRICS}, "additionalProperties": {"type": "integer", "minimum": 0}, "default": {}}}}}
    }})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profiles_reject_ambiguous_scope_and_impossible_budgets() -> Result<()> {
        let valid = "[[compatibility]]\nid='minimal'\nno_default_features=true\n[[resources]]\nid='build'\nkind='build'\nrelease=true\n";
        let config: ProfileConfig = toml::from_str(valid)?;
        config.validate()?;
        for source in [
            valid.replace("id='build'", "id='minimal'"),
            valid.replace(
                "no_default_features=true",
                "no_default_features=true\nall_features=true",
            ),
            valid.replace("release=true", "budgets={peak_rss_bytes=10}"),
            valid.replace("release=true", "runs=2"),
            valid.replace("release=true", "instrumented_metrics=['invented']"),
        ] {
            assert!(
                toml::from_str::<ProfileConfig>(&source)?
                    .validate()
                    .is_err()
            );
        }
        assert!(toml::from_str::<ProfileConfig>("[[compatibility]]\nid='x'\ntyop=true").is_err());
        Ok(())
    }
}
