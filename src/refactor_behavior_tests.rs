use crate::command_runner::checked_output;
use std::{path::Path, process::Command, time::Duration};

#[test]
fn candidate_refactors_execute_native_behavior_instead_of_assuming_equivalence()
-> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/refactor_pairs.rs");
    let binary = root
        .path()
        .join(format!("behavior{}", std::env::consts::EXE_SUFFIX));
    checked_output(
        Command::new("rustc")
            .arg("--edition=2024")
            .arg(&source)
            .arg("-o")
            .arg(&binary),
        Duration::from_secs(30),
    )?;
    checked_output(&mut Command::new(&binary), Duration::from_secs(5))?;
    let error = match checked_output(
        Command::new("rustc")
            .arg("--edition=2024")
            .args(["--cfg", "broken_ownership"])
            .arg(source)
            .arg("-o")
            .arg(binary),
        Duration::from_secs(30),
    ) {
        Ok(_) => anyhow::bail!("removing required ownership must not compile"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("mismatched types"), "{error:#}");
    Ok(())
}
