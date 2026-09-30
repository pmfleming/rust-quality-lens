use super::{impl_identity, impl_index, type_identity};
use crate::facts::{FileFacts, ImplFact, TypeFact};
use std::collections::BTreeSet;

#[test]
fn type_implementations_are_package_and_target_local() -> anyhow::Result<()> {
    let mut facts = Vec::new();
    for (package, target, methods) in [("a", "lib", 1), ("b", "lib", 2), ("a", "bin", 4)] {
        let mut fact = FileFacts::test_fact("src/lib.rs", "lib");
        fact.package_name = package.to_string();
        fact.target_name = target.to_string();
        fact.items.types.push(TypeFact {
            type_name: "Widget".into(),
            qualified_name: "lib::Widget".into(),
            module_key: "lib".into(),
            path: fact.path.clone(),
            line: 1,
            kind: "struct".into(),
            shape: "unit".into(),
            field_count: 0,
            variant_count: 0,
            variant_field_count: 0,
            declaration_span: 1,
        });
        fact.items.impls.push(ImplFact {
            type_name: "Widget".into(),
            qualified_type_name: "Widget".into(),
            module_key: "lib".into(),
            path: fact.path.clone(),
            method_count: methods,
        });
        facts.push(fact);
    }
    let index = impl_index(&facts);
    for (fact, methods) in facts.iter().zip([1, 2, 4]) {
        let summary = &index[&type_identity(fact, "lib::Widget")];
        assert_eq!(summary.method_count, methods);
        assert_eq!(summary.block_count, 1);
        let metadata = crate::measurement::tool_score_metadata("type_health");
        let provenance = crate::measurement::provenance();
        let confidence = serde_json::json!({"complete": true});
        let row = super::type_row(
            fact,
            &fact.items.types[0],
            &index,
            &metadata,
            &provenance,
            &confidence,
        );
        assert_eq!(row.method_count, methods);
        let wire = serde_json::to_value(row)?;
        assert_eq!(wire["package_name"], fact.package_name);
        assert_eq!(wire["qualified_name"], "lib::Widget");
        assert_eq!(wire["score_components"][4]["raw"]["files"], 1);
        assert!(
            wire.get("declaration").is_none(),
            "typed fields preserve the flat wire format"
        );
    }
    Ok(())
}

#[test]
fn ambiguous_implementation_names_are_not_joined() {
    let definitions = BTreeSet::from(["a::Widget".into(), "b::Widget".into()]);
    let imp = ImplFact {
        type_name: "Widget".into(),
        qualified_type_name: "Widget".into(),
        module_key: "elsewhere".into(),
        path: "elsewhere.rs".into(),
        method_count: 1,
    };
    assert!(impl_identity(&imp, &definitions).is_none());
}
