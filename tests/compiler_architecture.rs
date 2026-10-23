use anyhow::{Context, Result};
use serde_json::Value;
use std::{collections::BTreeMap, fs, path::Path};
mod support;
use support::measure;

#[path = "compiler_architecture/cross_crate.rs"]
mod cross_crate;
#[path = "compiler_architecture/policy.rs"]
mod policy;

fn fixture() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir()?;
    fs::create_dir(root.path().join("src"))?;
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='architecture-fixture'\nversion='0.1.0'\nedition='2024'\n[features]\nextra=[]\n[dependencies]\nedge-macros={path='macros'}\n[workspace]\nexclude=['macros']\n",
    )?;
    fs::create_dir_all(root.path().join("macros/src"))?;
    fs::write(
        root.path().join("macros/Cargo.toml"),
        "[package]\nname='edge-macros'\nversion='0.1.0'\nedition='2024'\n[lib]\nproc-macro=true\n[workspace]\n",
    )?;
    fs::write(
        root.path().join("macros/src/lib.rs"),
        r#"
        extern crate proc_macro;
        #[proc_macro_attribute]
        pub fn edge(_: proc_macro::TokenStream, _: proc_macro::TokenStream) -> proc_macro::TokenStream {
            "pub fn replaced() { crate::denied::call(); }".parse().unwrap()
        }
    "#,
    )?;
    fs::write(
        root.path().join("build.rs"),
        r#"fn main() {
        std::fs::write(std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("edges.rs"),
            "pub fn included() { crate::denied::call(); } macro_rules! nested { () => { pub fn nested() { crate::denied::call(); } } } nested!();").unwrap();
    }"#,
    )?;
    fs::write(
        root.path().join("src/lib.rs"),
        r#"
pub mod denied {
    pub struct Secret { pub value: u32 }
    pub fn call() {}
    pub trait Action { fn act(&self); }
}
pub mod factory { pub fn secret() -> crate::denied::Secret { crate::denied::Secret { value: 1 } } }
pub mod generated {
    #[edge_macros::edge] pub fn replaced() {}
    include!(concat!(env!("OUT_DIR"), "/edges.rs"));
    macro_rules! emit { () => { pub struct Made(crate::denied::Secret); pub fn made() { crate::denied::call(); } } }
    emit!();
    macro_rules! methods { () => { pub fn method(&self) { crate::denied::call(); } } }
    impl Made { methods!(); }
    pub fn statement() { macro_rules! invoke { () => { crate::denied::call(); } } invoke!(); }
    pub fn generic<T: crate::denied::Action>(value: &T) { value.act(); }
}
pub mod reader { pub fn inferred_field() -> u32 { let value = crate::factory::secret(); value.value } }
pub mod remote_impl { impl crate::denied::Secret { pub fn outside(&self) { crate::generated::made(); } } }
#[cfg(test)] pub mod test_only { pub fn run() { crate::denied::call(); } }
#[cfg(feature="extra")] pub mod feature_only { pub fn run() { crate::denied::call(); } }
"#,
    )?;
    Ok(root)
}

fn configure(root: &Path, driver: &Path) -> Result<()> {
    fs::write(
        root.join("rqlens.toml"),
        format!(
            "source_roots=['src']\n[rust]\nidentity_resolution='disabled'\ncompiler_inventory_driver={:?}\n[verification]\nall_targets=true\n",
            driver.to_string_lossy()
        ),
    )?;
    Ok(())
}

fn observed_edges(capture: &Value) -> Result<Vec<(String, String, bool)>> {
    let modules = capture["modules"]
        .as_array()
        .context("modules")?
        .iter()
        .map(|m| {
            (
                m["id"].as_str().unwrap_or_default(),
                m["path"].as_str().unwrap_or_default(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let declarations = capture["declarations"]
        .as_array()
        .context("declarations")?
        .iter()
        .map(|d| {
            (
                d["id"].as_str().unwrap_or_default(),
                d["module_id"].as_str().unwrap_or_default(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    Ok(capture["references"]
        .as_array()
        .context("references")?
        .iter()
        .filter_map(|r| {
            let source = modules.get(declarations.get(r["source_id"].as_str()?)?)?;
            let target = modules.get(declarations.get(r["target"]["declaration_id"].as_str()?)?)?;
            Some((
                source.to_string(),
                target.to_string(),
                r["origin"]["generated"] == true,
            ))
        })
        .collect())
}

#[test]
#[ignore = "requires pinned nightly rustc-dev adapter; set RQLENS_TEST_COMPILER_DRIVER"]
fn live_capture_has_generated_body_type_trait_and_impl_module_edges() -> Result<()> {
    let driver = fs::canonicalize(
        std::env::var_os("RQLENS_TEST_COMPILER_DRIVER").context("compiler driver")?,
    )?;
    let root = fixture()?;
    configure(root.path(), &driver)?;
    let document = measure(root.path(), "compiler-inventory", "compiler_inventory.json")?;
    assert_eq!(
        document["measurement_confidence"]["complete"], true,
        "{document}"
    );
    assert_eq!(
        document["summary"]["architecture_contexts_with_complete_capture"], 2,
        "{document}"
    );
    for profile in document["data"]["profiles"]
        .as_array()
        .context("profiles")?
    {
        let capture = &profile["architecture"];
        assert_eq!(capture["limitations"], serde_json::json!([]));
        let edges = observed_edges(capture)?;
        for (source, target, generated) in [
            ("generated", "denied", true),
            ("reader", "denied", false),
            ("remote_impl", "generated", false),
        ] {
            assert!(
                edges.contains(&(source.into(), target.into(), generated)),
                "missing {source} -> {target}: {edges:?}"
            );
        }
        assert!(!edges.iter().any(|(source, _, _)| source == "feature_only"));
        assert_eq!(
            edges
                .iter()
                .any(|(source, target, _)| source == "test_only" && target == "denied"),
            profile["test"] == true
        );
        for name in [
            "included",
            "nested",
            "made",
            "replaced",
            "method",
            "statement",
        ] {
            let function = profile["functions"]
                .as_array()
                .context("functions")?
                .iter()
                .find(|f| {
                    f["qualified_name"]
                        .as_str()
                        .is_some_and(|s| s.ends_with(&format!("::{name}")))
                })
                .context("generated function")?;
            assert!(
                capture["references"]
                    .as_array()
                    .context("references")?
                    .iter()
                    .any(|r| r["source_id"] == function["id"] && r["origin"]["generated"] == true),
                "missing generated reference for {name}"
            );
        }
        let kinds = capture["references"]
            .as_array()
            .context("references")?
            .iter()
            .map(|r| r["kind"].as_str().unwrap_or_default())
            .collect::<std::collections::BTreeSet<_>>();
        for kind in ["type", "trait", "body", "method"] {
            assert!(kinds.contains(kind), "{kinds:?}");
        }
    }
    Ok(())
}
