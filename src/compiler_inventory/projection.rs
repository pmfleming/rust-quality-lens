use super::{CompilerContext, ImplRecord, Inventory, Origin, Profile, TypeRecord, definition};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::BTreeSet;

impl Inventory {
    /// Replace the syntax-only type projection, never append it: a compiler
    /// definition is the unit of identity, not an authored name or macro name.
    pub(crate) fn type_rows(&self, fingerprint_complete: bool) -> Value {
        let confidence = self.confidence(fingerprint_complete);
        Value::Array(
            self.profiles
                .iter()
                .flat_map(|profile| {
                    profile
                        .types
                        .iter()
                        .map(|ty| type_row(profile, ty, &confidence))
                })
                .collect(),
        )
    }
}

pub(crate) fn validate_type_projection(row: &Value) -> Result<()> {
    let context: CompilerContext = serde_json::from_value(row["compiler_context"].clone())?;
    context.validate()?;
    ensure!(
        row["compiler_context_id"] == context.id(),
        "compiler context fingerprint mismatch"
    );
    let id = row["definition_id"]
        .as_str()
        .context("missing compiler type ID")?;
    let origin: Origin = serde_json::from_value(row["origin"].clone())?;
    let impls: Vec<ImplRecord> = serde_json::from_value(row["implementations"].clone())?;
    let methods = validate_implementations(&impls, id, &origin)?;
    let files = impl_files(impls.iter());
    let complete = row["measurement_confidence"]["complete"] == true;
    ensure!(
        !complete
            || (origin.location.is_some() && impls.iter().all(|imp| imp.origin.location.is_some())),
        "complete compiler type projection has unavailable source attribution"
    );
    ensure!(
        row["path"] == json!(origin.location.as_ref().map(|location| &location.path))
            && row["line"] == json!(origin.location.as_ref().map(|location| location.line)),
        "type location differs from compiler origin"
    );
    ensure!(
        row["impl_files"] == json!(files),
        "implementation file attribution differs from compiler origins"
    );
    for (field, observed, count) in [
        ("method_count", "observed_method_count", methods),
        ("impl_block_count", "observed_impl_block_count", impls.len()),
        ("impl_file_count", "observed_impl_file_count", files.len()),
    ] {
        ensure!(
            row[observed] == count && row[field] == json!(complete.then_some(count)),
            "compiler count {field} does not match definitions"
        );
    }
    Ok(())
}

fn validate_implementations(impls: &[ImplRecord], id: &str, origin: &Origin) -> Result<usize> {
    let mut ids = BTreeSet::new();
    definition(&mut ids, id, origin)?;
    let mut methods = BTreeSet::new();
    for imp in impls {
        definition(&mut ids, &imp.id, &imp.origin)?;
        ensure!(
            imp.owner_kind == "local_adt" && imp.owner_id.as_deref() == Some(id),
            "type projection uses a different compiler owner"
        );
        for method in &imp.method_ids {
            ensure!(
                !method.is_empty() && methods.insert(method),
                "duplicate compiler method in type projection"
            );
        }
    }
    Ok(methods.len())
}

fn impl_files<'a>(impls: impl Iterator<Item = &'a ImplRecord>) -> BTreeSet<&'a std::path::PathBuf> {
    impls
        .filter_map(|imp| imp.origin.location.as_ref().map(|location| &location.path))
        .collect()
}

fn type_row(profile: &Profile, ty: &TypeRecord, confidence: &Value) -> Value {
    let impls = profile
        .impls
        .iter()
        .filter(|imp| imp.owner_id.as_deref() == Some(&ty.id))
        .collect::<Vec<_>>();
    let methods = impls
        .iter()
        .flat_map(|imp| &imp.method_ids)
        .collect::<BTreeSet<_>>();
    let files = impl_files(impls.iter().copied());
    let location = ty.origin.location.as_ref();
    let context_id = profile.context_id();
    let module = ty
        .qualified_name
        .rsplit_once("::")
        .map_or(profile.crate_name.as_str(), |(module, _)| module);
    let complete = confidence["complete"] == true;
    json!({
        "type_name": ty.name, "qualified_name": ty.qualified_name, "kind": ty.kind,
        "definition_id": ty.id, "compiler_context_id": context_id,
        "compiler_context": profile.context(),
        "module_key": module, "module_id": format!("{}::{}::{module}", profile.package_name, context_id),
        "package_name": profile.package_name, "target_name": profile.crate_name,
        "identity_backend": "rustc-hir", "origin": ty.origin,
        "path": location.map(|location| &location.path), "line": location.map(|location| location.line),
        "field_count": ty.field_count, "variant_count": ty.variant_count, "variant_field_count": ty.variant_field_count,
        "method_count": complete.then_some(methods.len()), "impl_block_count": complete.then_some(impls.len()),
        "impl_file_count": complete.then_some(files.len()),
        "observed_method_count": methods.len(), "observed_impl_block_count": impls.len(),
        "observed_impl_file_count": files.len(), "impl_files": files,
        "implementations": impls,
        "structural_risk": null, "structural_score": null,
        "structural_score_status": "not_calibrated_for_compiler_inventory", "score_components": [],
        "signals": ["compiler-resolved per-context counts; source-calibrated structural scoring does not apply"],
        "measurement_confidence": confidence,
        "source": "compiler_type_inventory", "inventory_model_version": 1, "mock": false,
    })
}
