//! Static post-expansion HIR references. No runtime call-graph inference.
use crate::{identity, origin, origin_at};
use rustc_hir::{
    self as hir,
    def::{DefKind, Res},
    def_id::{CRATE_DEF_ID, DefId, LOCAL_CRATE},
    intravisit::{self, Visitor},
};
use rustc_middle::{hir::nested_filter, ty::TyCtxt};
use rustc_span::{ExpnId, ExpnKind, Span, hygiene::AstPass};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub fn capture(tcx: TyCtxt<'_>) -> Value {
    let root = CRATE_DEF_ID.to_def_id();
    let mut visitor = References {
        tcx,
        owner: root,
        kind: "type",
        declarations: BTreeMap::new(),
        references: Vec::new(),
        limitations: BTreeSet::new(),
        excluded: BTreeSet::new(),
    };
    let mut modules = Vec::new();
    for local in tcx.hir_crate_items(()).definitions() {
        let id = local.to_def_id();
        if tcx.def_kind(id) == DefKind::Mod {
            let parent = if id == root {
                None
            } else {
                Some(identity(tcx, module_of(tcx, tcx.parent(id))))
            };
            modules.push(
                json!({"id": identity(tcx, id), "path": module_path(tcx, id), "parent_id": parent,
                "origin": declaration_origin(tcx, id)}),
            );
            visitor.declaration(id);
        }
    }
    tcx.hir_walk_toplevel_module(&mut visitor);
    modules.sort_by_key(|v| v["id"].as_str().unwrap_or_default().to_string());
    visitor.references.sort_by_key(Value::to_string);
    visitor.references.dedup();
    json!({"model_version": 1, "crate_id": crate_id(tcx, root), "modules": modules,
        "declarations": visitor.declarations.into_values().collect::<Vec<_>>(), "references": visitor.references,
        "covered_categories": ["bodies", "imports", "traits", "types"], "limitations": visitor.limitations,
        "excluded_compiler_infrastructure": visitor.excluded})
}

fn crate_id(tcx: TyCtxt<'_>, id: DefId) -> String {
    format!("{:?}", tcx.stable_crate_id(id.krate))
}

fn module_of(tcx: TyCtxt<'_>, mut id: DefId) -> DefId {
    while tcx.def_kind(id) != DefKind::Mod {
        match tcx.opt_parent(id) {
            Some(parent) => id = parent,
            None => break,
        }
    }
    id
}

fn module_path(tcx: TyCtxt<'_>, module: DefId) -> String {
    let path = tcx.def_path_str(module);
    let name = tcx.crate_name(module.krate).to_string();
    if path == name {
        String::new()
    } else {
        path.strip_prefix(&format!("{name}::"))
            .unwrap_or(&path)
            .to_string()
    }
}

fn infrastructure(tcx: TyCtxt<'_>, id: DefId) -> Option<&'static str> {
    for mut expansion in [
        tcx.expn_that_defined(id),
        tcx.def_span(id).ctxt().outer_expn(),
    ] {
        while expansion != ExpnId::root() {
            let data = expansion.expn_data();
            if let ExpnKind::AstPass(pass) = data.kind {
                return Some(match pass {
                    AstPass::StdImports => "standard_library_injection",
                    AstPass::TestHarness => "test_harness",
                    AstPass::ProcMacroHarness => "proc_macro_harness",
                });
            }
            expansion = data.parent;
        }
    }
    None
}

fn declaration_origin(tcx: TyCtxt<'_>, id: DefId) -> Value {
    if id == CRATE_DEF_ID.to_def_id() {
        origin_at(tcx, id, tcx.hir_root_module().spans.inner_span)
    } else {
        origin(tcx, id)
    }
}

struct References<'tcx> {
    tcx: TyCtxt<'tcx>,
    owner: DefId,
    kind: &'static str,
    declarations: BTreeMap<String, Value>,
    references: Vec<Value>,
    limitations: BTreeSet<String>,
    excluded: BTreeSet<String>,
}

impl<'tcx> References<'tcx> {
    fn declaration(&mut self, id: DefId) {
        let key = identity(self.tcx, id);
        self.declarations.entry(key.clone()).or_insert_with(|| json!({
            "id": key, "module_id": identity(self.tcx, module_of(self.tcx, id)),
            "kind": format!("{:?}", self.tcx.def_kind(id)), "origin": declaration_origin(self.tcx, id),
        }));
    }

    fn target(&mut self, id: DefId) -> Value {
        if id.krate == LOCAL_CRATE {
            self.declaration(id);
            json!({"status": "local", "declaration_id": identity(self.tcx, id)})
        } else {
            json!({"status": "external", "crate_id": crate_id(self.tcx, id), "crate_name": self.tcx.crate_name(id.krate).to_string(),
                "definition_id": identity(self.tcx, id), "module_path": module_path(self.tcx, module_of(self.tcx, id)),
                "module_id": identity(self.tcx, module_of(self.tcx, id))})
        }
    }

    fn record(&mut self, res: Res, span: Span) {
        let target = match res {
            Res::Def(_, id) | Res::SelfCtor(id) => self.target(id),
            Res::SelfTyParam { trait_ } => self.target(trait_),
            Res::SelfTyAlias { alias_to, .. } => self.target(alias_to),
            Res::Err => {
                json!({"status": "unknown", "reason": "HIR reference has no resolved definition"})
            }
            // Locals, primitives, tool modules and inert attributes do not denote
            // inter-module Rust declarations. Macro invocations are expanded.
            Res::Local(_) | Res::PrimTy(_) | Res::ToolMod | Res::NonMacroAttr(_) => return,
        };
        self.declaration(self.owner);
        self.references.push(
            json!({"source_id": identity(self.tcx, self.owner), "target": target,
            "kind": self.kind, "origin": origin_at(self.tcx, self.owner, span)}),
        );
    }

    fn set_owner(&mut self, id: DefId) -> DefId {
        self.declaration(id);
        std::mem::replace(&mut self.owner, id)
    }
}

impl<'tcx> Visitor<'tcx> for References<'tcx> {
    type NestedFilter = nested_filter::All;
    fn maybe_tcx(&mut self) -> TyCtxt<'tcx> {
        self.tcx
    }

    fn visit_item(&mut self, item: &'tcx hir::Item<'tcx>) {
        let id = item.owner_id.def_id.to_def_id();
        // Exclude only compiler-tagged infrastructure, never by name or path.
        if let Some(kind) = infrastructure(self.tcx, id) {
            self.excluded.insert(kind.into());
            return;
        }
        let previous = self.set_owner(id);
        if matches!(item.kind, hir::ItemKind::ExternCrate(..)) {
            let kind = std::mem::replace(&mut self.kind, "import");
            let res = self
                .tcx
                .extern_mod_stmt_cnum(item.owner_id.def_id)
                .map_or(Res::Err, |krate| Res::Def(DefKind::Mod, krate.as_def_id()));
            self.record(res, item.span);
            self.kind = kind;
        }
        intravisit::walk_item(self, item);
        self.owner = previous;
    }

    fn visit_impl_item(&mut self, item: &'tcx hir::ImplItem<'tcx>) {
        let previous = self.set_owner(item.owner_id.def_id.to_def_id());
        intravisit::walk_impl_item(self, item);
        self.owner = previous;
    }

    fn visit_trait_item(&mut self, item: &'tcx hir::TraitItem<'tcx>) {
        let previous = self.set_owner(item.owner_id.def_id.to_def_id());
        intravisit::walk_trait_item(self, item);
        self.owner = previous;
    }

    fn visit_foreign_item(&mut self, item: &'tcx hir::ForeignItem<'tcx>) {
        let previous = self.set_owner(item.owner_id.def_id.to_def_id());
        intravisit::walk_foreign_item(self, item);
        self.owner = previous;
    }

    fn visit_body(&mut self, body: &hir::Body<'tcx>) {
        let previous = self.set_owner(self.tcx.hir_body_owner_def_id(body.id()).to_def_id());
        let kind = std::mem::replace(&mut self.kind, "body");
        intravisit::walk_body(self, body);
        self.owner = previous;
        self.kind = kind;
    }

    fn visit_use(&mut self, path: &'tcx hir::UsePath<'tcx>, id: hir::HirId) {
        let kind = std::mem::replace(&mut self.kind, "import");
        for res in path.res.present_items() {
            self.record(res, path.span);
        }
        intravisit::walk_use(self, path, id);
        self.kind = kind;
    }

    fn visit_ty(&mut self, ty: &'tcx hir::Ty<'tcx, hir::AmbigArg>) {
        let kind = std::mem::replace(&mut self.kind, "type");
        intravisit::walk_ty(self, ty);
        self.kind = kind;
    }

    fn visit_trait_ref(&mut self, reference: &'tcx hir::TraitRef<'tcx>) {
        let kind = std::mem::replace(&mut self.kind, "trait");
        intravisit::walk_trait_ref(self, reference);
        self.kind = kind;
    }

    fn visit_path(&mut self, path: &hir::Path<'tcx>, _id: hir::HirId) {
        self.record(path.res, path.span);
        intravisit::walk_path(self, path);
    }

    fn visit_qpath(&mut self, path: &'tcx hir::QPath<'tcx>, id: hir::HirId, span: Span) {
        if matches!(path, hir::QPath::TypeRelative(..)) {
            let res = if self.tcx.has_typeck_results(id.owner.def_id) {
                self.tcx.typeck(id.owner.def_id).qpath_res(path, id)
            } else {
                Res::Err
            };
            self.record(res, span);
        }
        intravisit::walk_qpath(self, path, id);
    }

    fn visit_expr(&mut self, expr: &'tcx hir::Expr<'tcx>) {
        if self.tcx.has_typeck_results(expr.hir_id.owner.def_id) {
            let typeck = self.tcx.typeck(expr.hir_id.owner.def_id);
            if let Some(id) = typeck.type_dependent_def_id(expr.hir_id) {
                let kind = std::mem::replace(&mut self.kind, "method");
                self.record(Res::Def(self.tcx.def_kind(id), id), expr.span);
                self.kind = kind;
            } else if matches!(expr.kind, hir::ExprKind::MethodCall(..)) {
                self.record(Res::Err, expr.span);
            }
            if let hir::ExprKind::Field(base, _) = expr.kind
                && let rustc_middle::ty::Adt(adt, _) = typeck.expr_ty_adjusted(base).kind()
                && let Some(index) = typeck.field_indices().get(expr.hir_id)
            {
                self.record(
                    Res::Def(DefKind::Field, adt.non_enum_variant().fields[*index].did),
                    expr.span,
                );
            }
        }
        if matches!(expr.kind, hir::ExprKind::InlineAsm(..)) {
            self.limitations
                .insert("assembly dependencies require separate symbol analysis".into());
        }
        intravisit::walk_expr(self, expr);
    }
}
