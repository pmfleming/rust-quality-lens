use anyhow::{Context, Result};
use serde_json::Value;
use std::{fs, process::Command};
mod support;

fn measure(source: &str) -> Result<Value> {
    let root = tempfile::tempdir()?;
    fs::create_dir(root.path().join("src"))?;
    fs::write(root.path().join("src/lib.rs"), source)?;
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='lexical-fixture'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )?;
    let compiled = Command::new("cargo")
        .args(["check", "--offline"])
        .current_dir(root.path())
        .output()?;
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let config = root.path().join("rqlens.toml");
    fs::write(
        &config,
        "source_roots=['src']\n[rust]\nidentity_resolution='disabled'\n",
    )?;
    let document = support::measure(root.path(), "type-health", "type_health.json")?;
    let syntax = support::measure(root.path(), "hotspots", "hotspots.json")?;
    // Owner uncertainty belongs to type evidence, not successfully parsed bodies.
    assert_eq!(syntax["measurement_confidence"]["complete"], true);
    Ok(document)
}

#[test]
fn block_items_have_distinct_owners_and_forward_bindings() -> Result<()> {
    let document = measure(
        r#"
pub struct Same;
pub fn first() {
    impl Same { fn local(&self) {} }
    struct Same;
    { struct Same; impl Same { fn nested(&self) {} fn other(&self) {} } }
    impl crate::Same { fn global(&self) {} }
}
pub fn second() { enum Same { A } impl Same { fn enum_method(&self) {} } }
pub fn closure() { let _ = || { struct Same; impl Same { fn closure_method(&self) {} } }; }
pub mod child { pub struct Same; impl Same { fn module_method(&self) {} } }
"#,
    )?;
    assert_eq!(
        document["measurement_confidence"]["complete"], true,
        "{document}"
    );
    let rows = document["records"].as_array().context("type rows")?;
    assert_eq!(rows.len(), 6);
    let ids = rows
        .iter()
        .map(|r| r["qualified_name"].as_str().context("qualified type"))
        .collect::<Result<std::collections::BTreeSet<_>>>()?;
    assert_eq!(ids.len(), rows.len());
    let mut methods = rows
        .iter()
        .map(|r| r["method_count"].as_u64().context("method count"))
        .collect::<Result<Vec<_>>>()?;
    methods.sort();
    assert_eq!(methods, [1, 1, 1, 1, 1, 2]);
    Ok(())
}

#[test]
fn aliases_imports_and_generic_shadowing_do_not_join_same_named_types() -> Result<()> {
    let document = measure(
        r#"
pub struct Same;
pub struct Other;
trait Marker { fn mark(&self); }
impl<Same> Marker for Same { fn mark(&self) {} }
pub fn local_alias() { type Same = Other; impl Same { fn alias(&self) {} } }
pub fn imported() { use crate::Other as Same; impl Same { fn imported(&self) {} } }
"#,
    )?;
    assert_eq!(document["measurement_confidence"]["partial"], true);
    for row in document["records"].as_array().context("type rows")? {
        assert_eq!(row["observed_method_count"], 0);
        assert!(row["method_count"].is_null());
    }
    assert_eq!(
        document["measurement_confidence"]["type_identity"]["unresolved_impl_blocks"],
        3
    );
    Ok(())
}
