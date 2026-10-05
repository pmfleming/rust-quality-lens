use super::{Inventory, Profile, capture::validate_coverage, validate_report};
use anyhow::Result;
use serde_json::json;
use std::collections::BTreeSet;

fn profile() -> Result<Profile> {
    let root = std::env::current_dir()?.join("compiler-inventory-fixture");
    let source = root.join("src/lib.rs");
    let location = json!({"path": source, "line": 2, "column": 1, "end_line": 2, "end_column": 8});
    let origin =
        json!({"generated": true, "location_kind": "invocation_anchor", "location": location});
    Ok(serde_json::from_value(json!({
        "schema_version": 1, "backend": "rustc-hir", "compiler_version": super::COMPILER_VERSION,
        "package_name": "fixture", "manifest_dir": root, "source_root": source,
        "crate_name": "fixture", "target_triple": "x86_64-unknown-linux-gnu", "test": false, "cfg": ["unix"],
        "types": [{"id":"type", "qualified_name":"fixture::Generated", "name":"Generated", "kind":"struct",
            "field_count": 1, "variant_count":0, "variant_field_count":0, "origin": origin}],
        "impls": [{"id":"impl", "owner_id":"type", "owner_display":"Generated", "owner_kind":"local_adt",
            "method_ids":["method"], "origin": origin}],
        "functions": [{"id":"method", "qualified_name":"fixture::<impl>::method", "origin": origin}],
    }))?)
}

#[test]
fn compiler_ids_join_generated_impls_without_name_or_span_guessing() -> Result<()> {
    let profile = profile()?;
    profile.validate()?;
    let expected_contexts = BTreeSet::from([(
        profile.manifest_dir.clone(),
        profile.source_root.clone(),
        false,
    )]);
    let inventory = Inventory {
        profiles: vec![profile],
        expected_contexts,
        ..Inventory::default()
    };
    let rows = inventory.type_rows(true);
    assert_eq!(rows[0]["method_count"], 1);
    assert_eq!(rows[0]["impl_block_count"], 1);
    assert_eq!(rows[0]["origin"]["location_kind"], "invocation_anchor");
    assert_eq!(
        rows[0]["structural_score_status"],
        "not_calibrated_for_compiler_inventory"
    );
    assert!(rows[0]["structural_risk"].is_null());
    let partial = inventory.type_rows(false);
    assert!(partial[0]["method_count"].is_null());
    assert_eq!(partial[0]["observed_method_count"], 1);
    validate_report(&inventory.report(true))?;
    super::validate_type_projection(&rows[0])?;
    let mut forged = rows[0].clone();
    forged["observed_method_count"] = json!(2);
    assert!(super::validate_type_projection(&forged).is_err());
    let mut forged = rows[0].clone();
    forged["compiler_context"]["test"] = json!(true);
    assert!(super::validate_type_projection(&forged).is_err());
    let mut forged = inventory.report(true);
    forged["profiles"] = json!([]);
    forged["summary"]["context_count"] = json!(0);
    assert!(validate_report(&forged).is_err());
    Ok(())
}

#[test]
fn duplicate_unknown_and_contradictory_compiler_evidence_is_rejected() -> Result<()> {
    let original = profile()?;
    let mut duplicate = original.clone();
    duplicate.types.push(duplicate.types[0].clone());
    assert!(duplicate.validate().is_err());
    let mut owner = original.clone();
    owner.impls[0].owner_id = Some("not-a-local-type".into());
    assert!(owner.validate().is_err());
    let mut method = original.clone();
    method.impls[0].method_ids.push("method".into());
    assert!(method.validate().is_err());
    let mut location = original.clone();
    location.types[0].origin.location_kind = "authored".into();
    assert!(location.validate().is_err());
    let mut cfg = original.clone();
    cfg.test = true;
    assert!(cfg.validate().is_err());
    let mut version = original.clone();
    version.compiler_version = "other".into();
    assert!(version.validate().is_err());
    let expected = BTreeSet::from([(
        original.manifest_dir.clone(),
        original.source_root.clone(),
        false,
    )]);
    validate_coverage(std::slice::from_ref(&original), &expected)?;
    assert!(validate_coverage(&[], &expected).is_err());
    assert!(validate_coverage(&[original.clone(), original], &expected).is_err());
    Ok(())
}

#[test]
fn test_and_feature_profiles_are_not_collapsed_into_one_type_total() -> Result<()> {
    let normal = profile()?;
    let mut tests = normal.clone();
    tests.test = true;
    tests.cfg = vec!["test".into(), "unix".into()];
    tests.validate()?;
    let mut feature = normal.clone();
    feature.cfg.insert(0, "feature=\"extra\"".into());
    assert_ne!(normal.context_id(), feature.context_id());
    let inventory = Inventory {
        profiles: vec![normal, tests],
        ..Inventory::default()
    };
    let rows = inventory.type_rows(true);
    assert_eq!(rows.as_array().map(Vec::len), Some(2));
    assert_ne!(
        rows[0]["compiler_context_id"],
        rows[1]["compiler_context_id"]
    );
    assert_eq!(rows[0]["method_count"], 1);
    assert_eq!(rows[1]["method_count"], 1);
    Ok(())
}
