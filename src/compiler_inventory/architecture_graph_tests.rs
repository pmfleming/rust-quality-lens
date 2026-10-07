use super::{Profile, architecture_graph::project};
use crate::{architecture::dependency_violation, config::ArchitectureRule};
use anyhow::{Context, Result};
use serde_json::json;

fn profile(package: &str, test: bool) -> Result<Profile> {
    let origin = json!({"generated": false, "location_kind": "authored", "location": {
        "path": format!("/{package}/src/lib.rs"), "line": 1, "column": 1, "end_line": 1, "end_column": 2
    }});
    Ok(serde_json::from_value(json!({
        "schema_version": 1, "backend": "rustc-hir", "compiler_version": super::COMPILER_VERSION,
        "package_name": package, "manifest_dir": format!("/{package}"), "source_root": format!("/{package}/src/lib.rs"),
        "crate_name": package, "target_triple": "x86_64-unknown-linux-gnu", "test": test,
        "cfg": if test { vec!["test", "unix"] } else { vec!["unix"] }, "types": [], "impls": [], "functions": [],
        "architecture": {"model_version": 1, "crate_id": format!("{package}-{test}"),
            "covered_categories": ["bodies", "imports", "traits", "types"], "limitations": [],
            "modules": [
                {"id":"root", "path":"", "parent_id":null, "origin":origin},
                {"id":"a", "path":"a", "parent_id":"root", "origin":origin},
                {"id":"b", "path":"b", "parent_id":"root", "origin":origin}],
            "declarations": [{"id":"decl-a", "module_id":"a", "kind":"fn", "origin":origin}, {"id":"decl-b", "module_id":"b", "kind":"fn", "origin":origin}],
            "references": [
                {"source_id":"decl-a", "target":{"status":"local", "declaration_id":"decl-b"}, "kind":"body", "origin":origin},
                {"source_id":"decl-a", "target":{"status":"local", "declaration_id":"decl-b"}, "kind":"type", "origin":origin}]
        }
    }))?)
}

#[test]
fn inline_modules_contexts_and_observations_remain_distinct() -> Result<()> {
    let profiles = [profile("fixture", false)?, profile("fixture", true)?];
    for profile in &profiles {
        profile.validate()?;
    }
    let graphs = project(&profiles);
    assert_ne!(graphs[0].compiler_context_id, graphs[1].compiler_context_id);
    assert_ne!(graphs[0].edges[0].source_id, graphs[1].edges[0].source_id);
    let rule: ArchitectureRule = serde_json::from_value(
        json!({"id":"boundary", "from":["fixture::shared::a"], "deny":["fixture::shared::b"]}),
    )?;
    for graph in graphs {
        assert!(graph.missing_evidence.is_empty());
        assert_eq!(graph.modules.len(), 3);
        assert_eq!(graph.edges.len(), 1);
        let edge = &graph.edges[0];
        assert_eq!(edge.observations.len(), 2);
        assert!(
            dependency_violation(
                &rule,
                &edge.source_id,
                &edge.target_id,
                &graph.modules[&edge.source_id].selector,
                &graph.modules[&edge.target_id].selector
            )
            .is_some()
        );
    }
    Ok(())
}

#[test]
fn compiler_crate_ids_join_cross_crate_edges_not_dependency_aliases() -> Result<()> {
    let mut source = profile("application", false)?;
    let target = profile("library", false)?;
    let capture = source.architecture.as_mut().context("source capture")?;
    capture.references[0].target = super::architecture::Target::External {
        crate_id: "library-false".into(),
        crate_name: "renamed_dependency".into(),
        definition_id: "decl-b".into(),
        module_path: "b".into(),
        module_id: "b".into(),
    };
    let graphs = project(&[source.clone(), target.clone()]);
    let graph = &graphs[0];
    assert!(graph.missing_evidence.is_empty());
    assert!(
        graph
            .edges
            .iter()
            .any(|edge| graph.modules[&edge.target_id].selector == "library::shared::b")
    );
    let mut ambiguous = target.clone();
    ambiguous.test = true;
    ambiguous.cfg.insert(0, "test".into());
    let graphs = project(&[source.clone(), target, ambiguous]);
    assert_eq!(graphs[0].unresolved_references.len(), 1);
    assert!(!graphs[0].missing_evidence.is_empty());
    let graphs = project(&[source]);
    assert!(
        graphs[0]
            .modules
            .values()
            .any(|m| m.selector == "external::renamed_dependency::b")
    );
    Ok(())
}

#[test]
fn absent_architecture_and_unknown_targets_stay_partial() -> Result<()> {
    let mut missing = profile("missing", false)?;
    missing.architecture = None;
    let mut unknown = profile("unknown", false)?;
    unknown.architecture.as_mut().context("capture")?.references[0].target =
        super::architecture::Target::Unknown {
            reason: "unsupported associated target".into(),
        };
    let profiles = vec![missing, unknown];
    let graphs = project(&profiles);
    assert!(graphs.iter().all(|g| !g.missing_evidence.is_empty()));
    assert_eq!(graphs[1].unresolved_references.len(), 1);
    let inventory = super::Inventory {
        expected_contexts: profiles
            .iter()
            .map(|p| (p.manifest_dir.clone(), p.source_root.clone(), p.test))
            .collect(),
        profiles,
        ..Default::default()
    };
    assert_eq!(
        inventory.report(true)["measurement_confidence"]["complete"],
        true,
        "type inventory and architecture coverage are independent"
    );
    let report = inventory.architecture_policy(&Default::default(), true);
    assert_eq!(report["summary"]["violation_count"], 0);
    assert_eq!(report["measurement_confidence"]["partial"], true);
    super::validate_architecture_policy(&report, true)?;
    let mut forged = report;
    forged["measurement_confidence"]["complete"] = json!(true);
    forged["measurement_confidence"]["partial"] = json!(false);
    assert!(super::validate_architecture_policy(&forged, true).is_err());
    Ok(())
}
