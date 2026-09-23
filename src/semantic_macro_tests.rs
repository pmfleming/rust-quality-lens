//! Live compiler-assisted regression: the same two navigation origins returned
//! by wayland-scanner, without a registry/network dependency in the fixture.
use super::resolve;
use crate::{
    config::LensConfig,
    facts::{DependencyReferenceFact, FileFacts},
};
use std::fs;

fn macro_fixture() -> anyhow::Result<(tempfile::TempDir, Vec<FileFacts>)> {
    let root = tempfile::tempdir()?;
    fs::create_dir_all(root.path().join("src"))?;
    fs::create_dir_all(root.path().join("generator/src"))?;
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='semantic-macro-fixture'\nversion='0.1.0'\nedition='2024'\n[dependencies]\ngenerator={path='generator'}\n[workspace]\nmembers=['generator']\n",
    )?;
    fs::write(
        root.path().join("build.rs"),
        "fn main() { println!(\"cargo:rustc-check-cfg=cfg(build_ready)\"); println!(\"cargo:rustc-cfg=build_ready\"); }\n",
    )?;
    fs::write(
        root.path().join("generator/Cargo.toml"),
        "[package]\nname='generator'\nversion='0.1.0'\nedition='2024'\n[lib]\nproc-macro=true\n",
    )?;
    fs::write(
        root.path().join("generator/src/lib.rs"),
        r#"
extern crate proc_macro;
#[proc_macro]
pub fn protocol(_: proc_macro::TokenStream) -> proc_macro::TokenStream {
    "pub mod generated { pub struct Message; } pub mod __interfaces { pub const INTERFACE: u32 = 1; }".parse().unwrap()
}
"#,
    )?;
    let source = "#[cfg(build_ready)]\nmod protocol { generator::protocol!(); }\nuse crate::protocol::generated::{self, Message};\nuse crate::protocol::__interfaces::*;\n";
    let library = root.path().join("src/lib.rs");
    fs::write(&library, source)?;
    let config_path = root.path().join("rqlens.toml");
    fs::write(
        &config_path,
        "source_roots=['src']\n[rust]\nidentity_resolution='required'\nidentity_timeout_seconds=90\n",
    )?;
    let mut fact = FileFacts::test_fact(&library.to_string_lossy(), "lib");
    fact.package_name = "semantic-macro-fixture".into();
    fact.module_id = "semantic-macro-fixture::lib::lib".into();
    fact.target_kind = "lib".into();
    fact.graph.dependency_references = [
        ("crate::protocol::generated::Message", 3, "Message"),
        ("crate::protocol::generated::self", 3, "self"),
        ("crate::protocol::__interfaces::*", 4, "__interfaces"),
    ]
    .into_iter()
    .map(|(raw, line, token)| {
        Ok(DependencyReferenceFact {
            raw_path: raw.into(),
            line,
            column: source
                .lines()
                .nth(line - 1)
                .and_then(|line| line.find(token))
                .ok_or_else(|| anyhow::anyhow!("missing fixture token {token}"))?,
        })
    })
    .collect::<anyhow::Result<Vec<_>>>()?;
    Ok((root, vec![fact]))
}

#[test]
#[ignore = "requires rust-analyzer, a Rust sysroot and its proc-macro server"]
fn trusted_build_scripts_and_macros_resolve_generated_imports_and_cache() -> anyhow::Result<()> {
    let (root, mut facts) = macro_fixture()?;
    let config_path = root.path().join("rqlens.toml");
    let mut config = LensConfig::load(Some(config_path.clone()))?;
    assert!(!config.identity_build_macros);
    assert!(
        resolve(&config, &mut facts).is_err(),
        "macro execution must require opt-in"
    );
    fs::write(
        &config_path,
        "source_roots=['src']\n[rust]\nidentity_resolution='required'\nidentity_build_macros=true\nidentity_timeout_seconds=90\n",
    )?;
    config = LensConfig::load(Some(config_path))?;
    assert!(config.identity_build_macros);
    let summary = resolve(&config, &mut facts)?;
    assert!(summary.complete);
    assert!(!summary.cache_hit);
    assert_eq!(summary.local_definition_count, 3);
    assert_eq!(summary.external_definition_count, 0);
    for (resolved, symbol) in
        facts[0]
            .graph
            .resolved_dependencies
            .iter()
            .zip(["Message", "generated", "__interfaces"])
    {
        assert_eq!(
            resolved.target_module_id.as_deref(),
            Some("semantic-macro-fixture::lib::lib")
        );
        assert_eq!(
            resolved.symbol_identity.as_deref(),
            Some(format!("semantic-macro-fixture::lib::lib::{symbol}").as_str())
        );
    }
    assert!(resolve(&config, &mut facts)?.cache_hit);
    Ok(())
}
