use super::measure;
use anyhow::{Context, Result};
use std::fs;

#[test]
#[ignore = "requires pinned nightly rustc-dev adapter; set RQLENS_TEST_COMPILER_DRIVER"]
fn compiler_policy_joins_renamed_workspace_dependencies_in_each_feature_context() -> Result<()> {
    let driver = fs::canonicalize(
        std::env::var_os("RQLENS_TEST_COMPILER_DRIVER").context("compiler driver")?,
    )?;
    let root = tempfile::tempdir()?;
    for package in ["app", "core"] {
        fs::create_dir_all(root.path().join(package).join("src"))?;
    }
    fs::write(
        root.path().join("Cargo.toml"),
        "[workspace]\nresolver='3'\nmembers=['app','core']\n",
    )?;
    fs::write(
        root.path().join("core/Cargo.toml"),
        "[package]\nname='edge-core'\nversion='0.1.0'\nedition='2024'\n",
    )?;
    fs::write(
        root.path().join("core/src/lib.rs"),
        "pub mod domain { pub fn run() {} }",
    )?;
    fs::write(
        root.path().join("app/Cargo.toml"),
        "[package]\nname='edge-app'\nversion='0.1.0'\nedition='2024'\n[features]\nextra=[]\n[dependencies]\nrenamed={package='edge-core',path='../core'}\n",
    )?;
    fs::write(
        root.path().join("app/src/lib.rs"),
        "pub mod client { pub fn run() { renamed::domain::run(); } }\n#[cfg(feature=\"extra\")] pub mod feature_boundary { pub fn run() { renamed::domain::run(); } }",
    )?;
    let config = format!(
        "source_roots=['app/src','core/src']\n[rust]\narchitecture_backend='compiler'\ncompiler_inventory_driver={:?}\nidentity_resolution='disabled'\n[verification]\nworkspace=true\nall_targets=true\n[[architecture.rules]]\nid='cross-crate'\nfrom=['edge-app::shared::*']\ndeny=['edge-core::shared::domain']\n",
        driver.to_string_lossy()
    );
    fs::write(root.path().join("rqlens.toml"), &config)?;
    let base = measure(root.path(), "architecture-rules", "architecture_rules.json")?;
    assert_eq!(base["measurement_confidence"]["complete"], true, "{base}");
    assert_eq!(base["summary"]["compilation_context_count"], 4);
    assert_eq!(base["summary"]["violation_count"], 2);
    assert!(
        base["data"]["violations"]
            .as_array()
            .context("violations")?
            .iter()
            .all(|v| v["target_selector"] == "edge-core::shared::domain")
    );
    fs::write(
        root.path().join("rqlens.toml"),
        config.replace(
            "all_targets=true",
            "all_targets=true\nfeatures=['edge-app/extra']",
        ),
    )?;
    let featured = measure(root.path(), "architecture-rules", "architecture_rules.json")?;
    assert_eq!(
        featured["measurement_confidence"]["complete"], true,
        "{featured}"
    );
    assert_eq!(featured["summary"]["violation_count"], 4);
    let before = &base["data"]["violations"][0]["compiler_context_id"];
    assert!(
        featured["data"]["violations"]
            .as_array()
            .context("violations")?
            .iter()
            .all(|v| &v["compiler_context_id"] != before)
    );
    Ok(())
}
