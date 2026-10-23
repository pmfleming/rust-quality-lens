use anyhow::{Context, Result};
use serde_json::Value;
use std::{fs, path::Path};
mod support;
use support::measure;

fn fixture(driver: Option<&Path>) -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir()?;
    fs::create_dir(root.path().join("src"))?;
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='compiled-fixture'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )?;
    fs::write(root.path().join("src/lib.rs"), "pub struct Plain;")?;
    configure(root.path(), driver, false)?;
    Ok(root)
}

fn configure(root: &Path, driver: Option<&Path>, feature: bool) -> Result<()> {
    let driver = driver.map_or_else(String::new, |driver| {
        format!(
            "compiler_inventory_driver = {:?}\n",
            driver.to_string_lossy()
        )
    });
    fs::write(
        root.join("rqlens.toml"),
        format!(
            "source_roots=['src']\noutput_dir='target/analysis'\n[rust]\nidentity_resolution='disabled'\n{driver}[verification]\nall_targets=true\nfeatures={}\n",
            if feature { "['extra']" } else { "[]" }
        ),
    )?;
    Ok(())
}

#[test]
fn unavailable_compiler_inventory_is_not_a_complete_empty_inventory() -> Result<()> {
    let root = fixture(None)?;
    let absent = measure(root.path(), "compiler-inventory", "compiler_inventory.json")?;
    assert_eq!(absent["measurement_confidence"]["partial"], true);
    assert_eq!(absent["summary"]["context_count"], 0);
    assert_eq!(absent["capabilities"][0]["status"], "partial");
    let missing = root.path().join("missing-compiler-adapter");
    configure(root.path(), Some(&missing), false)?;
    let fallback = measure(root.path(), "type-health", "type_health.json")?;
    assert_eq!(fallback["measurement_confidence"]["complete"], false);
    assert_eq!(fallback["records"][0]["type_name"], "Plain");
    assert!(fallback["records"][0]["method_count"].is_null());
    assert!(
        fallback["measurement_confidence"]["missing_input"]
            .as_array()
            .is_some_and(|reasons| !reasons.is_empty())
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn unrequested_compiler_inventory_never_executes_the_adapter() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let root = fixture(None)?;
    let marker = root.path().join("executed");
    let adapter = root.path().join("adapter");
    fs::write(
        &adapter,
        format!("#!/bin/sh\ntouch '{}'\nexit 1\n", marker.display()),
    )?;
    fs::set_permissions(&adapter, fs::Permissions::from_mode(0o755))?;
    configure(root.path(), Some(&adapter), false)?;
    let document = measure(root.path(), "hotspots", "hotspots.json")?;
    assert_eq!(document["measurement_confidence"]["complete"], true);
    assert!(!marker.exists());
    Ok(())
}

fn normalize(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.remove("generated_at");
            object.remove("measured_at");
            object.values_mut().for_each(normalize);
        }
        Value::Array(array) => array.iter_mut().for_each(normalize),
        _ => {}
    }
}

fn write_expanded_fixture(root: &Path) -> Result<()> {
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='compiled-fixture'\nversion='0.1.0'\nedition='2024'\n[workspace]\nexclude=['macros']\n[features]\nextra=[]\n[dependencies]\nfixture-macros={path='macros'}\n[[test]]\nname='smoke'\npath='tests/smoke.rs'\nrequired-features=['extra']\n",
    )?;
    fs::write(
        root.join("build.rs"),
        r#"fn main() {
            println!("cargo::rustc-check-cfg=cfg(from_build_script)");
            println!("cargo::rustc-cfg=from_build_script");
            std::fs::write(std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("generated.rs"),
                "pub struct Included; impl Included { pub fn included(&self) {} } macro_rules! nested { () => { pub struct IncludedNested; } } nested!();").unwrap();
        }"#,
    )?;
    fs::create_dir_all(root.join("tests"))?;
    fs::write(
        root.join("tests/smoke.rs"),
        "pub struct IntegrationType; #[test] fn smoke() {}",
    )?;
    fs::create_dir_all(root.join("macros/src"))?;
    fs::write(
        root.join("macros/Cargo.toml"),
        "[package]\nname='fixture-macros'\nversion='0.1.0'\nedition='2024'\n[lib]\nproc-macro=true\n[workspace]\n",
    )?;
    fs::write(
        root.join("macros/src/lib.rs"),
        r#"
extern crate proc_macro;
#[proc_macro_attribute]
pub fn identity(_: proc_macro::TokenStream, input: proc_macro::TokenStream) -> proc_macro::TokenStream { input }
#[proc_macro_attribute]
pub fn replace(_: proc_macro::TokenStream, _: proc_macro::TokenStream) -> proc_macro::TokenStream {
    "pub struct AttributeGenerated; impl AttributeGenerated { pub fn generated(&self) {} }".parse().unwrap()
}
"#,
    )?;
    fs::write(
        root.join("src/lib.rs"),
        r#"
macro_rules! make { () => { #[derive(Clone, Debug)] pub struct Generated { pub value: u8 } } }
make!();
include!(concat!(env!("OUT_DIR"), "/generated.rs"));
use Generated as Alias;
impl Alias { pub fn authored(&self) {} }
macro_rules! methods { () => { pub fn from_macro(&self) {} } }
impl Generated { methods!(); }
macro_rules! nonlocal { () => { impl Generated { pub fn from_statement_macro(&self) {} } } }
fn nested_scope() { nonlocal!(); }
#[cfg(test)] impl Generated { pub fn test_only_method(&self) {} }
#[fixture_macros::replace] pub struct RemovedByAttribute;
#[fixture_macros::identity] pub struct AttributePass;
#[cfg(from_build_script)] pub struct BuildSelected;
#[cfg(not(from_build_script))] compile_error!("build cfg missing");
#[cfg(feature="extra")] pub struct Extra;
#[cfg(not(feature="extra"))] pub struct DefaultOnly;
#[cfg(test)] pub struct TestOnly;
pub mod a { pub struct Same; }
pub mod b { pub struct Same; }
type SameAlias = a::Same;
impl SameAlias { pub fn only_a(&self) {} }
pub trait Local { fn local(&self); }
impl Local for u8 { fn local(&self) {} }
"#,
    )?;
    Ok(())
}

#[test]
#[ignore = "requires pinned nightly rustc-dev adapter; set RQLENS_TEST_COMPILER_DRIVER"]
fn live_compiler_integrates_cfg_macros_derives_attributes_and_owners() -> Result<()> {
    let driver = std::env::var_os("RQLENS_TEST_COMPILER_DRIVER")
        .context("set RQLENS_TEST_COMPILER_DRIVER")?;
    let driver = fs::canonicalize(driver)?;
    let root = fixture(Some(&driver))?;
    write_expanded_fixture(root.path())?;
    let mut first = measure(root.path(), "compiler-inventory", "compiler_inventory.json")?;
    assert_eq!(first["measurement_confidence"]["complete"], true, "{first}");
    assert_eq!(first["summary"]["context_count"], 2);
    validate_profile_origins(root.path(), &first)?;
    let mut second = measure(root.path(), "compiler-inventory", "compiler_inventory.json")?;
    normalize(&mut first);
    normalize(&mut second);
    assert_eq!(
        first, second,
        "fresh builds must not leak temporary paths, PID order or duplicate captures"
    );

    validate_type_health(root.path())?;
    let syntax = measure(root.path(), "hotspots", "hotspots.json")?;
    assert_eq!(
        syntax["measurement_confidence"]["partial"], true,
        "declarations must not certify source/body rule coverage"
    );
    validate_feature_contexts(root.path(), &driver)
}

fn validate_profile_origins(root: &Path, inventory: &Value) -> Result<()> {
    for profile in inventory["data"]["profiles"]
        .as_array()
        .context("profiles")?
    {
        assert!(
            profile["cfg"]
                .as_array()
                .context("cfg")?
                .iter()
                .any(|cfg| cfg == "from_build_script")
        );
        let types = profile["types"].as_array().context("types")?;
        for name in ["Included", "IncludedNested", "AttributePass"] {
            let included = types
                .iter()
                .find(|ty| ty["name"] == name)
                .context("generated type")?;
            assert_eq!(
                included["origin"]["location_kind"], "invocation_anchor",
                "{included}"
            );
            assert_eq!(
                included["origin"]["location"]["path"],
                root.join("src/lib.rs").to_string_lossy().as_ref()
            );
        }
        assert_eq!(
            types.iter().any(|ty| ty["name"] == "TestOnly"),
            profile["test"] == true
        );
    }
    Ok(())
}

fn validate_type_health(root: &Path) -> Result<()> {
    let types = measure(root, "type-health", "type_health.json")?;
    assert_eq!(types["measurement_confidence"]["complete"], true, "{types}");
    let rows = types["records"].as_array().context("type rows")?;
    for row in rows {
        match row["type_name"].as_str() {
            Some("Generated") => {
                // Clone, Debug, imported alias, impl macro, statement macro;
                // the test context has one additional cfg-selected impl.
                let expected = if row["compiler_context"]["test"] == true {
                    6
                } else {
                    5
                };
                assert_eq!(row["method_count"], expected);
                assert_eq!(row["impl_block_count"], expected);
                assert_eq!(row["origin"]["location_kind"], "invocation_anchor");
            }
            Some("AttributeGenerated") => assert_eq!(row["method_count"], 1),
            Some("RemovedByAttribute" | "Extra") => {
                panic!("inactive/replaced declaration leaked into compiler inventory")
            }
            Some("Same") => assert_eq!(
                row["method_count"],
                if row["qualified_name"]
                    .as_str()
                    .is_some_and(|name| name == "a::Same" || name.ends_with("::a::Same"))
                {
                    1
                } else {
                    0
                }
            ),
            _ => {}
        }
        assert!(row["structural_risk"].is_null());
    }
    for name in ["Generated", "AttributeGenerated"] {
        assert_eq!(
            rows.iter().filter(|row| row["type_name"] == name).count(),
            2,
            "one definition per normal/test context, not authored+generated duplicates"
        );
    }
    Ok(())
}

fn validate_feature_contexts(root: &Path, driver: &Path) -> Result<()> {
    configure(root, Some(driver), true)?;
    let featured = measure(root, "type-health", "type_health.json")?;
    let featured = featured["records"].as_array().context("featured rows")?;
    assert!(featured.iter().any(|row| row["type_name"] == "Extra"));
    assert!(!featured.iter().any(|row| row["type_name"] == "DefaultOnly"));
    assert_eq!(
        featured
            .iter()
            .filter(|row| row["type_name"] == "IntegrationType"
                && row["compiler_context"]["test"] == true)
            .count(),
        1
    );

    fs::write(
        root.join("src/lib.rs"),
        "pub struct StillObserved; compile_error!(\"failed compilation\");",
    )?;
    let failed = measure(root, "type-health", "type_health.json")?;
    assert_eq!(failed["measurement_confidence"]["partial"], true);
    assert!(
        failed["records"]
            .as_array()
            .context("fallback rows")?
            .iter()
            .any(|row| row["type_name"] == "StillObserved")
    );
    assert!(
        failed["records"]
            .as_array()
            .context("fallback rows")?
            .iter()
            .all(|row| row["method_count"].is_null())
    );
    Ok(())
}
