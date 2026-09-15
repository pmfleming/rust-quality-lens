use super::*;
use serde_json::json;

#[test]
fn git_quoted_unicode_filename_is_included_in_scope() -> Result<()> {
    let root = tempfile::tempdir()?;
    fs::create_dir(root.path().join("src"))?;
    let source = root.path().join("src/café file.rs");
    fs::write(&source, "pub fn before() {}\n")?;
    for args in [
        vec!["init", "-q"],
        vec!["config", "core.quotePath", "true"],
        vec!["add", "."],
        vec![
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-qm",
            "initial",
        ],
    ] {
        crate::command_runner::checked_output(
            Command::new("git").args(args).current_dir(root.path()),
            std::time::Duration::from_secs(10),
        )?;
    }
    fs::write(source, "pub fn after_() {}\n")?;
    let text = git_diff(root.path(), "HEAD")?;
    let scope = diff::parse(&text)?;
    assert!(scope.paths.contains("src/café file.rs"));
    assert_eq!(scope.additions["src/café file.rs"], [[1, 1]]);
    Ok(())
}

#[test]
fn covered_context_does_not_inflate_changed_line_coverage() -> Result<()> {
    let root = tempfile::tempdir()?;
    fs::create_dir(root.path().join("src"))?;
    fs::write(root.path().join("src/lib.rs"), "pub fn value() {}\n")?;
    let config_path = root.path().join("rqlens.toml");
    fs::write(&config_path, "source_roots = [\"src\"]\n")?;
    let config = LensConfig::load(Some(config_path))?;
    let coverage = json!({
        "schema_version": 3,
        "risk_model_version": crate::measurement::MODEL_VERSION,
        "tool": "coverage",
        "input_fingerprint": config.input_fingerprint(),
        "measurement_confidence": {"complete": true, "partial": false},
        "data": {
            "summary": {"lines": {"count": 7, "covered": 6, "percent": 85.71}},
            "files": [{"path": "src/lib.rs", "line_hits": (1..=7).map(|line| json!({"line": line, "covered": line != 4})).collect::<Vec<_>>() }]
        }
    });
    write_json(&config.output_dir.join("coverage.json"), &coverage)?;
    let scope = diff::parse(
        "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,7 +1,7 @@\n one\n two\n three\n-old\n+new\n five\n six\n seven\n",
    )?;
    let evidence = changed_line_coverage(&config, &scope.additions, &config.input_fingerprint());
    assert_eq!(evidence["status"], "observed");
    assert_eq!(evidence["changed_executable_lines"], 1);
    assert_eq!(evidence["changed_covered_lines"], 0);
    assert_eq!(evidence["changed_line_coverage_percent"], 0.0);
    Ok(())
}

#[test]
#[cfg(not(windows))]
fn path_matching_preserves_case_on_case_sensitive_filesystems() -> Result<()> {
    let root = tempfile::tempdir()?;
    let upper = root.path().join("Upper.rs");
    let lower = root.path().join("upper.rs");
    fs::write(&upper, "upper")?;
    fs::write(&lower, "lower")?;
    // macOS may have a case-insensitive filesystem even though it is Unix.
    if fs::read_to_string(&upper)? != fs::read_to_string(&lower)? {
        assert_ne!(path_key(upper), path_key(lower));
    }
    Ok(())
}
