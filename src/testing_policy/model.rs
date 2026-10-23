use super::disposition::Disposition;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Config {
    pub(crate) manifest: String,
    #[serde(default)]
    pub(crate) evidence_inputs: Vec<String>,
}

impl Config {
    pub(crate) fn validate(&self) -> Result<()> {
        for path in std::iter::once(&self.manifest).chain(&self.evidence_inputs) {
            relative_path(path)?;
        }
        Ok(())
    }
}

pub(super) fn relative_path(path: &str) -> Result<()> {
    ensure!(
        !path.trim().is_empty()
            && !path.contains(['\\', ':'])
            && std::path::Path::new(path)
                .components()
                .all(|part| matches!(part, std::path::Component::Normal(_))),
        "expected a nonempty project-relative path: {path}"
    );
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(deny_unknown_fields)]
pub(crate) struct TestSelector {
    pub(crate) package: String,
    pub(crate) target: String,
    pub(crate) path: String,
    pub(crate) name: String,
}

impl TestSelector {
    pub(super) fn validate(&self) -> Result<()> {
        relative_path(&self.path)?;
        for value in [&self.package, &self.target, &self.name] {
            text(value)?;
            ensure!(!value.contains('*'), "test selectors must be exact");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Role {
    EndToEnd,
    Integration,
    Contract,
    Local,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Owner {
    pub(crate) role: Role,
    pub(crate) rationale: String,
    pub(crate) tests: Vec<TestSelector>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Support {
    pub(crate) test: TestSelector,
    // Empty is allowed: it requests a distinct-risk review, not deletion.
    pub(crate) distinct_risk: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Behavior {
    pub(crate) id: String,
    pub(crate) description: String,
    pub(crate) risks: Vec<String>,
    pub(crate) requirement: String,
    pub(crate) scope: String,
    pub(crate) expectations: Vec<String>,
    pub(crate) real_boundaries: Vec<String>,
    #[serde(default)]
    pub(crate) substituted_boundaries: Vec<String>,
    pub(crate) reviewer: String,
    pub(crate) review_revision: String,
    #[serde(default)]
    pub(crate) required: bool,
    pub(crate) owner: Owner,
    #[serde(default)]
    pub(crate) supporting: Vec<Support>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Manifest {
    pub(crate) version: u32,
    pub(crate) behaviors: Vec<Behavior>,
    #[serde(default)]
    pub(crate) dispositions: Vec<Disposition>,
}

impl Manifest {
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(self.version == 1, "unsupported testing manifest version");
        let mut ids = BTreeSet::new();
        for behavior in &self.behaviors {
            ensure!(
                ids.insert(&behavior.id),
                "duplicate behavior {}",
                behavior.id
            );
            behavior.validate()?;
        }
        let mut dispositions = BTreeSet::new();
        for disposition in &self.dispositions {
            ensure!(
                dispositions.insert(&disposition.id),
                "duplicate disposition {}",
                disposition.id
            );
            ensure!(
                ids.contains(&disposition.behavior),
                "unknown disposition behavior {}",
                disposition.behavior
            );
            disposition.validate()?;
        }
        Ok(())
    }
}

impl Behavior {
    fn validate(&self) -> Result<()> {
        for value in [
            &self.id,
            &self.description,
            &self.requirement,
            &self.scope,
            &self.reviewer,
            &self.review_revision,
            &self.owner.rationale,
        ] {
            text(value)?;
        }
        strings(&self.risks)?;
        strings(&self.expectations)?;
        for value in self
            .real_boundaries
            .iter()
            .chain(&self.substituted_boundaries)
        {
            text(value)?;
        }
        let mut tests = BTreeSet::new();
        for test in &self.owner.tests {
            test.validate()?;
            ensure!(
                tests.insert(test),
                "duplicate owner selector in {}",
                self.id
            );
        }
        for support in &self.supporting {
            support.test.validate()?;
            ensure!(
                tests.insert(&support.test),
                "duplicate supporting selector in {}",
                self.id
            );
        }
        Ok(())
    }
}

pub(super) fn text(value: &str) -> Result<()> {
    ensure!(
        !value.trim().is_empty(),
        "required testing policy text is empty"
    );
    Ok(())
}

pub(super) fn strings(values: &[String]) -> Result<()> {
    ensure!(!values.is_empty(), "required testing policy list is empty");
    values.iter().try_for_each(|value| text(value))
}
