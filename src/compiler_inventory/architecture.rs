//! Versioned compiler architecture observations, distinct from type inventory.
use super::{Origin, definition, validate_origin};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const MODEL_VERSION: u64 = 1;
pub(crate) const CATEGORIES: [&str; 4] = ["bodies", "imports", "traits", "types"];

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Module {
    pub(crate) id: String,
    pub(crate) path: String,
    pub(crate) parent_id: Option<String>,
    pub(crate) origin: Origin,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Declaration {
    pub(crate) id: String,
    pub(crate) module_id: String,
    pub(crate) kind: String,
    pub(crate) origin: Origin,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Target {
    Local {
        declaration_id: String,
    },
    External {
        crate_id: String,
        crate_name: String,
        definition_id: String,
        module_path: String,
        module_id: String,
    },
    Unknown {
        reason: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReferenceKind {
    Import,
    Type,
    Trait,
    Body,
    Method,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Reference {
    pub(crate) source_id: String,
    pub(crate) target: Target,
    pub(crate) kind: ReferenceKind,
    pub(crate) origin: Origin,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Capture {
    pub(crate) model_version: u64,
    pub(crate) crate_id: String,
    pub(crate) modules: Vec<Module>,
    pub(crate) declarations: Vec<Declaration>,
    pub(crate) references: Vec<Reference>,
    pub(crate) covered_categories: BTreeSet<String>,
    pub(crate) limitations: Vec<String>,
    #[serde(default)]
    pub(crate) excluded_compiler_infrastructure: BTreeSet<String>,
}

impl Capture {
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            self.model_version == MODEL_VERSION && !self.crate_id.is_empty(),
            "unsupported architecture capture identity"
        );
        ensure!(
            self.excluded_compiler_infrastructure
                .iter()
                .all(|kind| matches!(
                    kind.as_str(),
                    "standard_library_injection" | "test_harness" | "proc_macro_harness"
                )),
            "unknown compiler infrastructure exclusion"
        );
        let modules = self.validate_module_tree()?;
        let declarations = self.validate_declarations(&modules)?;
        for reference in &self.references {
            reference.validate(&declarations, &self.crate_id)?;
        }
        ensure!(
            self.covered_categories
                .iter()
                .all(|kind| CATEGORIES.contains(&kind.as_str())),
            "unknown architecture traversal category"
        );
        Ok(())
    }

    fn validate_module_tree(&self) -> Result<BTreeSet<&str>> {
        let mut ids = BTreeSet::new();
        for module in &self.modules {
            definition(&mut ids, &module.id, &module.origin)?;
            ensure!(
                module.path.is_empty() == module.parent_id.is_none(),
                "contradictory crate-root module path"
            );
        }
        ensure!(!ids.is_empty(), "architecture capture has no modules");
        let parents = self
            .modules
            .iter()
            .map(|m| (m.id.as_str(), m.parent_id.as_deref()))
            .collect::<BTreeMap<_, _>>();
        ensure!(
            parents.values().filter(|p| p.is_none()).count() == 1,
            "architecture needs exactly one crate root"
        );
        for module in &self.modules {
            let mut seen = BTreeSet::new();
            let mut cursor = Some(module.id.as_str());
            while let Some(id) = cursor {
                ensure!(
                    seen.insert(id) && parents.contains_key(id),
                    "cyclic or dangling module parent"
                );
                cursor = parents[id];
            }
        }
        Ok(ids)
    }

    fn validate_declarations(&self, modules: &BTreeSet<&str>) -> Result<BTreeSet<&str>> {
        let mut declarations = BTreeSet::new();
        for declaration in &self.declarations {
            definition(&mut declarations, &declaration.id, &declaration.origin)?;
            ensure!(
                modules.contains(declaration.module_id.as_str()) && !declaration.kind.is_empty(),
                "declaration has no module owner"
            );
        }
        Ok(declarations)
    }

    pub(crate) fn missing_evidence(&self) -> Vec<String> {
        let mut reasons = self.limitations.clone();
        for category in CATEGORIES {
            if !self.covered_categories.contains(category) {
                reasons.push(format!("architecture traversal missing {category}"));
            }
        }
        if self
            .references
            .iter()
            .any(|r| matches!(r.target, Target::Unknown { .. }))
        {
            reasons.push("architecture contains unknown reference targets".into());
        }
        if self.modules.iter().any(|m| m.origin.location.is_none())
            || self
                .declarations
                .iter()
                .any(|d| d.origin.location.is_none())
            || self.references.iter().any(|r| r.origin.location.is_none())
        {
            reasons.push("architecture has unattributable declarations or references".into());
        }
        reasons
    }
}

impl Reference {
    fn validate(&self, declarations: &BTreeSet<&str>, owner_crate: &str) -> Result<()> {
        ensure!(
            declarations.contains(self.source_id.as_str()),
            "reference has no source declaration"
        );
        validate_origin(&self.origin)?;
        match &self.target {
            Target::Local { declaration_id } => ensure!(
                declarations.contains(declaration_id.as_str()),
                "dangling local reference target"
            ),
            Target::External {
                crate_id,
                crate_name,
                definition_id,
                module_id,
                ..
            } => ensure!(
                !crate_id.is_empty()
                    && !crate_name.is_empty()
                    && !definition_id.is_empty()
                    && !module_id.is_empty()
                    && crate_id != owner_crate,
                "invalid external reference identity"
            ),
            Target::Unknown { reason } => {
                ensure!(!reason.is_empty(), "unknown reference needs a reason")
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Capture;
    use serde_json::{Value, json};

    pub(super) fn fixture() -> Value {
        let origin = json!({"generated": false, "location_kind": "authored", "location": {
            "path": "/fixture/src/lib.rs", "line": 1, "column": 1, "end_line": 1, "end_column": 2
        }});
        json!({"model_version": 1, "crate_id": "crate-A", "covered_categories": ["bodies", "imports", "traits", "types"], "limitations": [],
            "modules": [{"id": "root", "path": "", "parent_id": null, "origin": origin}],
            "declarations": [{"id": "item", "module_id": "root", "kind": "struct", "origin": origin}],
            "references": [{"source_id": "item", "target": {"status": "local", "declaration_id": "item"}, "kind": "type", "origin": origin}]})
    }

    #[test]
    fn ownership_contract_rejects_invalid_capture_relationships() -> anyhow::Result<()> {
        let capture: Capture = serde_json::from_value(fixture())?;
        capture.validate()?;
        assert!(capture.missing_evidence().is_empty());
        let mut tree = fixture();
        let origin = tree["modules"][0]["origin"].clone();
        tree["modules"].as_array_mut().ok_or_else(|| anyhow::anyhow!("modules"))?.extend([
            json!({"id": "branch", "path": "branch", "parent_id": "root", "origin": origin}),
            json!({"id": "leaf", "path": "branch::leaf", "parent_id": "branch", "origin": origin}),
        ]);
        serde_json::from_value::<Capture>(tree.clone())?.validate()?;
        let mut reversed: Capture = serde_json::from_value(tree.clone())?;
        reversed.modules.reverse();
        reversed.validate()?;
        let declaration = tree["declarations"][0].clone();
        for (pointer, value) in [
            ("/model_version", json!(2)),
            ("/crate_id", json!("")),
            ("/covered_categories", json!(["imaginary"])),
            ("/excluded_compiler_infrastructure", json!(["everything"])),
            ("/modules", json!([])),
            (
                "/modules",
                json!([
                    {"id": "root", "path": "", "parent_id": null, "origin": origin},
                    {"id": "other", "path": "", "parent_id": null, "origin": origin}
                ]),
            ),
            ("/modules/0/parent_id", json!("root")),
            ("/modules/1/id", json!("root")),
            ("/modules/1/path", json!("")),
            ("/modules/1/parent_id", json!("leaf")),
            ("/modules/2/parent_id", json!("missing")),
            ("/modules/0/origin/location/line", json!(0)),
            ("/declarations", json!([declaration, declaration])),
            ("/declarations/0/id", json!("")),
            ("/declarations/0/module_id", json!("missing")),
            ("/declarations/0/kind", json!("")),
            ("/declarations/0/origin/location/line", json!(0)),
            ("/references/0/source_id", json!("missing")),
            ("/references/0/origin/location/line", json!(0)),
            ("/references/0/target/declaration_id", json!("missing")),
            (
                "/references/0/target",
                json!({"status": "external", "crate_id": "crate-A", "crate_name": "self", "definition_id": "item", "module_id": "root", "module_path": ""}),
            ),
            (
                "/references/0/target",
                json!({"status": "unknown", "reason": ""}),
            ),
        ] {
            let mut wire = tree.clone();
            if pointer == "/excluded_compiler_infrastructure" {
                wire["excluded_compiler_infrastructure"] = json!([]);
            }
            *wire
                .pointer_mut(pointer)
                .ok_or_else(|| anyhow::anyhow!("fixture pointer"))? = value;
            let result = serde_json::from_value::<Capture>(wire)?.validate();
            eprintln!("capture-case {pointer}: {result:?}");
            assert!(result.is_err(), "{pointer}");
        }
        Ok(())
    }

    #[test]
    fn incomplete_categories_and_unknown_edges_cannot_certify_zero() -> anyhow::Result<()> {
        let mut wire = fixture();
        wire["references"][0]["target"] =
            json!({"status": "unknown", "reason": "unresolved associated type"});
        wire["covered_categories"] = json!(["imports"]);
        let capture: Capture = serde_json::from_value(wire)?;
        capture.validate()?;
        assert_eq!(capture.missing_evidence().len(), 4);
        Ok(())
    }
}
