use super::{
    Expansion, Report, SourceSite, Status, Summary, inventory_complete, validate,
    validate_expansion,
};
use crate::facts::{FileFacts, MacroContext, MacroInvocation};
use anyhow::Result;
use serde_json::{Value, json};

fn observation() -> Expansion {
    Expansion::unavailable(
        SourceSite {
            path: "src/lib.rs".into(),
            package_name: "example".into(),
            syntax_target: "example".into(),
            invocation: MacroInvocation {
                kind: MacroContext::Item,
                macro_path: "make".into(),
                module_key: "lib".into(),
                line: 1,
                column_utf16: 0,
            },
        },
        Status::Unavailable,
        "not evaluated",
    )
}

#[test]
fn empty_expansion_is_not_missing_expansion() -> Result<()> {
    let mut site = observation();
    assert!(site.apply(Value::Null).is_err());
    assert!(
        site.apply(json!({"name": "make!", "expansion": false}))
            .is_err()
    );
    assert!(
        site.apply(json!({"name": "other!", "expansion": ""}))
            .is_err()
    );
    assert!(site.status == Status::Unavailable);
    site.apply(json!({"name": "make!", "expansion": ""}))?;
    assert!(site.status == Status::Expanded);
    assert_eq!(site.expanded_source.as_deref(), Some(""));
    validate_expansion(site)
}

#[test]
fn response_success_cannot_certify_semantic_inventory() -> Result<()> {
    let mut site = observation();
    site.apply(json!({"name": "make", "expansion": "struct Generated;"}))?;
    let report = Report {
        summary: Summary {
            inventory_complete: true,
            observed_sites: 1,
            expanded_sites: 1,
            response_complete: true,
            semantic_inventory_complete: false,
        },
        backend: "rust-analyzer/expandMacro".into(),
        analyzer_version: Some("fixture".into()),
        backend_configuration: json!({}),
        scope: "item/impl".into(),
        expansions: vec![site],
        measurement_confidence: serde_json::from_value(
            json!({"complete": false, "partial": true}),
        )?,
    };
    let mut wire = serde_json::to_value(report)?;
    validate(&wire)?;
    wire["summary"]["semantic_inventory_complete"] = json!(true);
    assert!(validate(&wire).is_err());
    wire["summary"]["semantic_inventory_complete"] = json!(false);
    wire["summary"]["expanded_sites"] = json!(0);
    assert!(validate(&wire).is_err());
    Ok(())
}

#[test]
fn missing_or_recovered_inventory_is_not_known_empty() {
    let mut fact = FileFacts::test_fact("src/lib.rs", "lib");
    assert!(!inventory_complete(std::slice::from_ref(&fact), 1));
    fact.items.macro_invocations = Some(vec![]);
    assert!(inventory_complete(std::slice::from_ref(&fact), 1));
    assert!(!inventory_complete(std::slice::from_ref(&fact), 2));
    fact.parse_status = "recovered".into();
    assert!(!inventory_complete(&[fact], 1));
}

#[test]
fn unavailable_expansion_cannot_carry_generated_source() {
    let mut site = observation();
    site.expanded_source = Some("struct Invented;".into());
    assert!(validate_expansion(site).is_err());
}
