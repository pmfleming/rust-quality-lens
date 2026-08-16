use proc_macro2::Span;
use quote::ToTokens;
use rust_quality_lens_helpers::{
    exit_on_error, module_key_for_path, normalize_path, qualify, read_paths_file,
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fs;
use std::path::Path;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{
    Attribute, BinOp, ExprBinary, ExprBreak, ExprCall, ExprClosure, ExprContinue, ExprForLoop,
    ExprIf, ExprLoop, ExprMatch, ExprPath, ExprRawAddr, ExprTry, ExprWhile, Fields, ForeignItem,
    ImplItem, ImplItemFn, Item, ItemEnum, ItemFn, ItemForeignMod, ItemImpl, ItemStatic, ItemStruct,
    ItemUse, ReturnType, Type, TypePath, UseTree,
};

#[derive(Serialize)]
struct FileFacts {
    path: String,
    module_key: String,
    target_kind: String,
    entrypoint_kind: Option<String>,
    is_entrypoint: bool,
    parse_status: String,
    source_metrics_available: bool,
    dependencies: Vec<String>,
    dependency_references: Vec<DependencyReferenceFact>,
    child_modules: Vec<String>,
    module_files: Vec<ModuleFileFact>,
    unsupported_patterns: Vec<String>,
    public_api_count: usize,
    documented_public_api_count: usize,
    has_crate_docs: bool,
    has_inline_tests: bool,
    source_line_count: usize,
    source_nonblank_line_count: usize,
    source_comment_line_count: usize,
    function_count: usize,
    types: Vec<TypeFact>,
    impls: Vec<ImplFact>,
    tests: Vec<TestFact>,
    functions: Vec<FunctionFact>,
    escape_counts: BTreeMap<String, usize>,
    escape_locations: Vec<LocationFact>,
    quality_findings: Vec<QualityFinding>,
}

#[derive(Serialize)]
struct TypeFact {
    type_name: String,
    qualified_name: String,
    module_key: String,
    path: String,
    line: usize,
    kind: String,
    shape: String,
    field_count: usize,
    variant_count: usize,
    variant_field_count: usize,
    declaration_span: usize,
}

#[derive(Serialize)]
struct ImplFact {
    type_name: String,
    qualified_type_name: String,
    module_key: String,
    path: String,
    line: usize,
    method_count: usize,
}

#[derive(Serialize)]
struct TestFact {
    name: String,
    qualified_name: String,
    path: String,
    line: usize,
    attribute: String,
    module_key: String,
}

#[derive(Serialize)]
struct FunctionFact {
    name: String,
    qualified_name: String,
    module_key: String,
    path: String,
    start_line: usize,
    end_line: usize,
    source_line_count: usize,
    branch_pressure: usize,
    path_pressure: usize,
    max_nesting_depth: usize,
    cyclomatic_complexity: usize,
    cognitive_complexity: usize,
}

#[derive(Serialize)]
struct DependencyReferenceFact {
    raw_path: String,
    line: usize,
    column: usize,
}

#[derive(Serialize)]
struct ModuleFileFact {
    module_key: String,
    path: String,
    line: usize,
}

#[derive(Serialize)]
struct LocationFact {
    kind: String,
    line: usize,
}

#[derive(Serialize)]
struct QualityFinding {
    rule_id: String,
    kind: String,
    line: usize,
    message: String,
    test_code: bool,
}

struct FactVisitor {
    path: String,
    module_key: String,
    dependencies: Vec<String>,
    dependency_references: Vec<DependencyReferenceFact>,
    child_modules: Vec<String>,
    module_files: Vec<ModuleFileFact>,
    unsupported_patterns: Vec<String>,
    module_stack: Vec<String>,
    public_api_count: usize,
    documented_public_api_count: usize,
    has_crate_docs: bool,
    has_inline_tests: bool,
    source_line_count: usize,
    source_nonblank_line_count: usize,
    source_comment_line_count: usize,
    function_count: usize,
    types: Vec<TypeFact>,
    impls: Vec<ImplFact>,
    tests: Vec<TestFact>,
    functions: Vec<FunctionFact>,
    escape_counts: BTreeMap<String, usize>,
    escape_locations: Vec<LocationFact>,
    quality_findings: Vec<QualityFinding>,
    source_lines: Vec<String>,
    test_scope_depth: usize,
}

impl FactVisitor {
    fn new(path: &str, content: &str) -> Self {
        let source_line_count = content.lines().count();
        let source_nonblank_line_count = content
            .lines()
            .filter(|line| !line.trim().is_empty() && !line.trim().starts_with("//"))
            .count();
        let source_comment_line_count = content
            .lines()
            .filter(|line| line.trim().starts_with("//"))
            .count();
        Self {
            path: normalize_path(path),
            module_key: module_key_for_path(path),
            dependencies: Vec::new(),
            dependency_references: Vec::new(),
            child_modules: Vec::new(),
            module_files: Vec::new(),
            unsupported_patterns: Vec::new(),
            module_stack: Vec::new(),
            public_api_count: 0,
            documented_public_api_count: 0,
            has_crate_docs: content
                .lines()
                .any(|line| line.trim_start().starts_with("//!")),
            has_inline_tests: false,
            source_line_count,
            source_nonblank_line_count,
            source_comment_line_count,
            function_count: 0,
            types: Vec::new(),
            impls: Vec::new(),
            tests: Vec::new(),
            functions: Vec::new(),
            escape_counts: BTreeMap::new(),
            escape_locations: Vec::new(),
            quality_findings: Vec::new(),
            source_lines: content.lines().map(str::to_string).collect(),
            test_scope_depth: 0,
        }
    }

    fn into_facts(self) -> FileFacts {
        self.into_facts_with_status("ok".to_string())
    }

    fn into_facts_with_status(mut self, parse_status: String) -> FileFacts {
        let target_kind = target_kind_for_path(&self.path).to_string();
        let entrypoint_kind = entrypoint_kind_for_path(&self.path).map(str::to_string);
        let is_entrypoint = entrypoint_kind.is_some();
        self.dependencies.sort();
        self.dependencies.dedup();
        self.dependency_references.sort_by(|left, right| {
            (&left.raw_path, left.line, left.column).cmp(&(
                &right.raw_path,
                right.line,
                right.column,
            ))
        });
        self.dependency_references.dedup_by(|left, right| {
            left.raw_path == right.raw_path
                && left.line == right.line
                && left.column == right.column
        });
        self.child_modules.sort();
        self.child_modules.dedup();
        self.module_files
            .sort_by(|a, b| a.module_key.cmp(&b.module_key));
        self.unsupported_patterns.sort();
        self.unsupported_patterns.dedup();
        FileFacts {
            path: self.path,
            module_key: self.module_key,
            target_kind,
            entrypoint_kind,
            is_entrypoint,
            parse_status,
            source_metrics_available: true,
            dependencies: self.dependencies,
            dependency_references: self.dependency_references,
            child_modules: self.child_modules,
            module_files: self.module_files,
            unsupported_patterns: self.unsupported_patterns,
            public_api_count: self.public_api_count,
            documented_public_api_count: self.documented_public_api_count,
            has_crate_docs: self.has_crate_docs,
            has_inline_tests: self.has_inline_tests,
            source_line_count: self.source_line_count,
            source_nonblank_line_count: self.source_nonblank_line_count,
            source_comment_line_count: self.source_comment_line_count,
            function_count: self.function_count,
            types: self.types,
            impls: self.impls,
            tests: self.tests,
            functions: self.functions,
            escape_counts: self.escape_counts,
            escape_locations: self.escape_locations,
            quality_findings: self.quality_findings,
        }
    }

    fn bump_escape(&mut self, kind: &str, span: Span) {
        *self.escape_counts.entry(kind.to_string()).or_insert(0) += 1;
        self.escape_locations.push(LocationFact {
            kind: kind.to_string(),
            line: span_start_line(span),
        });
    }

    fn record_finding(&mut self, rule_id: &str, kind: &str, span: Span, message: &str) {
        self.quality_findings.push(QualityFinding {
            rule_id: rule_id.to_string(),
            kind: kind.to_string(),
            line: span_start_line(span),
            message: message.to_string(),
            test_code: self.test_scope_depth > 0
                || self
                    .module_stack
                    .iter()
                    .any(|module| matches!(module.as_str(), "test" | "tests")),
        });
    }

    fn has_safety_rationale(&self, span: Span) -> bool {
        let line = span_start_line(span);
        let start = line.saturating_sub(4);
        self.source_lines
            .get(start..line.saturating_sub(1))
            .into_iter()
            .flatten()
            .any(|line| line.to_ascii_uppercase().contains("SAFETY:"))
    }

    fn add_dependency_at(&mut self, path: String, span: Span) {
        if path.is_empty() {
            return;
        }
        let start = span.start();
        self.dependencies.push(path.clone());
        self.dependency_references.push(DependencyReferenceFact {
            raw_path: path,
            line: start.line,
            column: start.column,
        });
    }

    fn add_expression_dependency(&mut self, path: &syn::Path) {
        let first = path
            .segments
            .first()
            .map(|segment| segment.ident.to_string());
        if path.leading_colon.is_some()
            || path.segments.len() > 1
            || first
                .as_deref()
                .is_some_and(|name| matches!(name, "crate" | "self" | "super"))
        {
            let span = path
                .segments
                .last()
                .map_or_else(|| path.span(), |segment| segment.ident.span());
            self.add_dependency_at(path_to_string(path), span);
        }
    }

    fn record_public_api(&mut self, visibility: &syn::Visibility, attrs: &[Attribute]) {
        if !is_public(visibility) {
            return;
        }
        self.public_api_count += 1;
        if has_docs(attrs) {
            self.documented_public_api_count += 1;
        }
    }

    fn scan_attrs(&mut self, attrs: &[Attribute]) {
        for attr in attrs {
            let path = path_to_string(attr.path());
            if path == "repr" {
                self.bump_escape("repr_escape", attr.span());
            }
            if matches!(
                path.as_str(),
                "no_mangle" | "export_name" | "link_name" | "link_section" | "used"
            ) {
                self.bump_escape("linkage_escape", attr.span());
            }
            if path == "allow" || path == "expect" {
                if attr.meta.to_token_stream().to_string().contains("clippy") {
                    self.bump_escape("clippy_suppression", attr.span());
                } else {
                    self.bump_escape("lint_suppression", attr.span());
                }
            }
            if path == "cfg" && attr.meta.to_token_stream().to_string().contains("test") {
                self.has_inline_tests = true;
            }
        }
    }

    fn maybe_record_test(&mut self, func: &ItemFn) {
        for attr in &func.attrs {
            let path = path_to_string(attr.path());
            if is_test_attribute(&path) {
                self.tests.push(TestFact {
                    name: func.sig.ident.to_string(),
                    qualified_name: self.qualified_test_name(&func.sig.ident.to_string()),
                    path: self.path.clone(),
                    line: span_start_line(attr.span()),
                    attribute: path,
                    module_key: self.current_module_key(),
                });
                self.has_inline_tests = true;
                return;
            }
        }
    }

    fn current_module_key(&self) -> String {
        let base = if self.module_key == "lib" || self.module_key == "main" {
            String::new()
        } else {
            self.module_key.clone()
        };
        let mut parts = Vec::new();
        if !base.is_empty() {
            parts.push(base);
        }
        parts.extend(self.module_stack.iter().cloned());
        if parts.is_empty() {
            self.module_key.clone()
        } else {
            parts.join("::")
        }
    }

    fn qualified_test_name(&self, name: &str) -> String {
        qualify(&self.module_stack, name)
    }

    fn record_function_metrics(
        &mut self,
        name: String,
        qualified_name: String,
        span: proc_macro2::Span,
        body: &syn::Block,
    ) {
        let mut complexity = FunctionComplexity {
            path_pressure: 1,
            ..FunctionComplexity::default()
        };
        complexity.visit_block(body);
        let mut standard_complexity = StandardComplexity {
            cyclomatic_complexity: 1,
            ..StandardComplexity::default()
        };
        standard_complexity.visit_block(body);
        let start_line = span.start().line;
        let end_line = body.brace_token.span.close().end().line;
        self.functions.push(FunctionFact {
            name,
            qualified_name,
            module_key: self.current_module_key(),
            path: self.path.clone(),
            start_line,
            end_line,
            source_line_count: end_line.saturating_sub(start_line) + 1,
            branch_pressure: complexity.branch_pressure,
            path_pressure: complexity.path_pressure,
            max_nesting_depth: complexity.max_nesting_depth,
            cyclomatic_complexity: standard_complexity.cyclomatic_complexity,
            cognitive_complexity: standard_complexity.cognitive_complexity,
        });
    }
}

#[derive(Default)]
struct FunctionComplexity {
    branch_pressure: usize,
    path_pressure: usize,
    nesting_depth: usize,
    max_nesting_depth: usize,
}

impl FunctionComplexity {
    fn enter_branch(&mut self, path_increment: usize) {
        self.branch_pressure += 1 + self.nesting_depth;
        self.path_pressure += path_increment;
        self.nesting_depth += 1;
        self.max_nesting_depth = self.max_nesting_depth.max(self.nesting_depth);
    }

    fn leave_branch(&mut self) {
        self.nesting_depth = self.nesting_depth.saturating_sub(1);
    }
}

macro_rules! branch_visitor {
    ($method:ident, $visit:ident, $expression:ty) => {
        fn $method(&mut self, expression: &'ast $expression) {
            self.enter_branch(1);
            visit::$visit(self, expression);
            self.leave_branch();
        }
    };
}

impl<'ast> Visit<'ast> for FunctionComplexity {
    branch_visitor!(visit_expr_if, visit_expr_if, ExprIf);

    fn visit_expr_match(&mut self, expression: &'ast ExprMatch) {
        self.enter_branch(expression.arms.len().saturating_sub(1).max(1));
        visit::visit_expr_match(self, expression);
        self.leave_branch();
    }

    branch_visitor!(visit_expr_for_loop, visit_expr_for_loop, ExprForLoop);
    branch_visitor!(visit_expr_while, visit_expr_while, ExprWhile);
    branch_visitor!(visit_expr_loop, visit_expr_loop, ExprLoop);

    fn visit_expr_binary(&mut self, expression: &'ast ExprBinary) {
        if matches!(expression.op, BinOp::And(_) | BinOp::Or(_)) {
            self.branch_pressure += 1;
            self.path_pressure += 1;
        }
        visit::visit_expr_binary(self, expression);
    }

    fn visit_expr_try(&mut self, expression: &'ast ExprTry) {
        self.branch_pressure += 1;
        visit::visit_expr_try(self, expression);
    }
}

#[derive(Default)]
struct StandardComplexity {
    cyclomatic_complexity: usize,
    cognitive_complexity: usize,
    cognitive_nesting: usize,
    logical_expression_depth: usize,
    next_if_is_else_if: bool,
}

impl StandardComplexity {
    fn enter_cognitive_structure(&mut self) {
        self.cognitive_complexity += 1 + self.cognitive_nesting;
        self.cognitive_nesting += 1;
    }

    fn leave_cognitive_structure(&mut self) {
        self.cognitive_nesting = self.cognitive_nesting.saturating_sub(1);
    }

    fn logical_sequence_complexity(expression: &ExprBinary) -> usize {
        fn operators(expression: &syn::Expr, result: &mut Vec<bool>) {
            if let syn::Expr::Binary(binary) = expression
                && matches!(binary.op, BinOp::And(_) | BinOp::Or(_))
            {
                operators(&binary.left, result);
                result.push(matches!(binary.op, BinOp::And(_)));
                operators(&binary.right, result);
            }
        }

        let mut sequence = Vec::new();
        operators(&syn::Expr::Binary(expression.clone()), &mut sequence);
        sequence
            .windows(2)
            .filter(|pair| pair[0] != pair[1])
            .count()
            + usize::from(!sequence.is_empty())
    }
}

impl<'ast> Visit<'ast> for StandardComplexity {
    fn visit_expr_if(&mut self, expression: &'ast ExprIf) {
        self.cyclomatic_complexity += 1;
        let is_else_if = std::mem::take(&mut self.next_if_is_else_if);
        if !is_else_if {
            self.enter_cognitive_structure();
        }

        self.visit_expr(&expression.cond);
        self.visit_block(&expression.then_branch);
        if let Some((_, alternative)) = &expression.else_branch {
            self.cognitive_complexity += 1;
            if matches!(alternative.as_ref(), syn::Expr::If(_)) {
                self.next_if_is_else_if = true;
            }
            self.visit_expr(alternative);
        }

        if !is_else_if {
            self.leave_cognitive_structure();
        }
    }

    fn visit_expr_match(&mut self, expression: &'ast ExprMatch) {
        self.cyclomatic_complexity += expression.arms.len();
        self.enter_cognitive_structure();
        visit::visit_expr_match(self, expression);
        self.leave_cognitive_structure();
    }

    fn visit_expr_for_loop(&mut self, expression: &'ast ExprForLoop) {
        self.cyclomatic_complexity += 1;
        self.enter_cognitive_structure();
        visit::visit_expr_for_loop(self, expression);
        self.leave_cognitive_structure();
    }

    fn visit_expr_while(&mut self, expression: &'ast ExprWhile) {
        self.cyclomatic_complexity += 1;
        self.enter_cognitive_structure();
        visit::visit_expr_while(self, expression);
        self.leave_cognitive_structure();
    }

    fn visit_expr_loop(&mut self, expression: &'ast ExprLoop) {
        self.cyclomatic_complexity += 1;
        self.enter_cognitive_structure();
        visit::visit_expr_loop(self, expression);
        self.leave_cognitive_structure();
    }

    fn visit_expr_binary(&mut self, expression: &'ast ExprBinary) {
        if matches!(expression.op, BinOp::And(_) | BinOp::Or(_)) {
            self.cyclomatic_complexity += 1;
            if self.logical_expression_depth == 0 {
                self.cognitive_complexity += Self::logical_sequence_complexity(expression);
            }
            self.logical_expression_depth += 1;
            visit::visit_expr_binary(self, expression);
            self.logical_expression_depth = self.logical_expression_depth.saturating_sub(1);
        } else {
            visit::visit_expr_binary(self, expression);
        }
    }

    fn visit_expr_try(&mut self, expression: &'ast ExprTry) {
        self.cyclomatic_complexity += 1;
        visit::visit_expr_try(self, expression);
    }

    fn visit_expr_closure(&mut self, expression: &'ast ExprClosure) {
        self.cognitive_nesting += 1;
        visit::visit_expr_closure(self, expression);
        self.cognitive_nesting = self.cognitive_nesting.saturating_sub(1);
    }

    fn visit_expr_break(&mut self, expression: &'ast ExprBreak) {
        if expression.label.is_some() {
            self.cognitive_complexity += 1;
        }
        visit::visit_expr_break(self, expression);
    }

    fn visit_expr_continue(&mut self, expression: &'ast ExprContinue) {
        if expression.label.is_some() {
            self.cognitive_complexity += 1;
        }
        visit::visit_expr_continue(self, expression);
    }
}

impl<'ast> Visit<'ast> for FactVisitor {
    fn visit_item(&mut self, i: &'ast Item) {
        match i {
            Item::Const(item) => {
                self.scan_attrs(&item.attrs);
                self.record_public_api(&item.vis, &item.attrs);
            }
            Item::Enum(item) => self.record_enum(item),
            Item::Fn(item) => {
                self.record_fn(item);
                let test_scope = has_test_attrs(&item.attrs);
                self.test_scope_depth += usize::from(test_scope);
                visit::visit_item(self, i);
                self.test_scope_depth = self
                    .test_scope_depth
                    .saturating_sub(usize::from(test_scope));
                return;
            }
            Item::ForeignMod(item) => self.record_foreign_mod(item),
            Item::Impl(item) => self.record_impl(item),
            Item::Macro(item) => self.record_item_macro(item),
            Item::Mod(item) => {
                self.record_mod(item);
                if item.content.is_some() {
                    let test_scope = item.ident == "tests" || has_test_attrs(&item.attrs);
                    self.test_scope_depth += usize::from(test_scope);
                    self.module_stack.push(item.ident.to_string());
                    visit::visit_item(self, i);
                    self.module_stack.pop();
                    self.test_scope_depth = self
                        .test_scope_depth
                        .saturating_sub(usize::from(test_scope));
                } else {
                    visit::visit_item(self, i);
                }
                return;
            }
            Item::Static(item) => self.record_static(item),
            Item::Struct(item) => self.record_struct(item),
            Item::Trait(item) => {
                self.scan_attrs(&item.attrs);
                self.record_public_api(&item.vis, &item.attrs);
                if item.unsafety.is_some() {
                    self.bump_escape("unsafe_trait", item.trait_token.span);
                    if !has_safety_docs(&item.attrs) {
                        self.record_finding(
                            "rust.safety.missing-safety-docs",
                            "unsafe-trait",
                            item.trait_token.span,
                            "unsafe trait does not document its Safety contract",
                        );
                    }
                }
            }
            Item::Type(item) => {
                self.scan_attrs(&item.attrs);
                self.record_public_api(&item.vis, &item.attrs);
            }
            Item::Union(item) => {
                self.scan_attrs(&item.attrs);
                self.record_public_api(&item.vis, &item.attrs);
                self.bump_escape("union", item.union_token.span);
            }
            Item::Use(item) => self.record_use(item),
            _ => {}
        }
        visit::visit_item(self, i);
    }

    fn visit_expr_call(&mut self, i: &'ast ExprCall) {
        if let syn::Expr::Path(path) = i.func.as_ref() {
            if path_ends_with(&path.path, "transmute")
                || path_ends_with(&path.path, "transmute_copy")
            {
                self.bump_escape("transmute", path.path.span());
            }
            self.add_expression_dependency(&path.path);
        }
        visit::visit_expr_call(self, i);
    }

    fn visit_expr_path(&mut self, i: &'ast ExprPath) {
        self.add_expression_dependency(&i.path);
        visit::visit_expr_path(self, i);
    }

    fn visit_expr_raw_addr(&mut self, i: &'ast ExprRawAddr) {
        self.bump_escape("raw_borrow", i.and_token.span);
        visit::visit_expr_raw_addr(self, i);
    }

    fn visit_expr_unsafe(&mut self, i: &'ast syn::ExprUnsafe) {
        self.bump_escape("unsafe_block", i.unsafe_token.span);
        if !self.has_safety_rationale(i.unsafe_token.span) {
            self.record_finding(
                "rust.safety.undocumented-unsafe",
                "unsafe-block",
                i.unsafe_token.span,
                "unsafe block has no nearby SAFETY rationale",
            );
        }
        visit::visit_expr_unsafe(self, i);
    }

    fn visit_expr_method_call(&mut self, expression: &'ast syn::ExprMethodCall) {
        let method = expression.method.to_string();
        if matches!(method.as_str(), "unwrap" | "expect") {
            self.record_finding(
                &format!("rust.reliability.{method}"),
                "panic-path",
                expression.method.span(),
                &format!(
                    "{method} may panic; verify and document the invariant or propagate the error"
                ),
            );
        }
        visit::visit_expr_method_call(self, expression);
    }

    fn visit_macro(&mut self, i: &'ast syn::Macro) {
        let path = path_to_string(&i.path);
        self.add_dependency_at(
            path.clone(),
            i.path
                .segments
                .last()
                .map_or(i.path.span(), |segment| segment.ident.span()),
        );
        if path == "asm" || path == "global_asm" {
            self.bump_escape("asm_macro", i.path.span());
        }
        let macro_name = path.rsplit("::").next().unwrap_or(&path);
        if matches!(macro_name, "panic" | "todo" | "unimplemented") {
            self.record_finding(
                &format!("rust.reliability.{macro_name}"),
                "panic-path",
                i.path.span(),
                &format!("{macro_name}! introduces an explicit panic path"),
            );
        }
        visit::visit_macro(self, i);
    }

    fn visit_type_path(&mut self, i: &'ast TypePath) {
        let path = path_to_string(&i.path);
        if path_ends_with(&i.path, "MaybeUninit") {
            self.bump_escape("maybe_uninit", i.path.span());
        }
        if let Some(qself) = &i.qself
            && let Some(value) = type_dependency_string(&qself.ty)
        {
            self.add_dependency_at(value, qself.ty.span());
        }
        if i.path.leading_colon.is_some() || i.path.segments.len() > 1 {
            self.add_dependency_at(
                path,
                i.path
                    .segments
                    .last()
                    .map_or_else(|| i.path.span(), |segment| segment.ident.span()),
            );
        }
        visit::visit_type_path(self, i);
    }
}

impl FactVisitor {
    fn record_enum(&mut self, item: &ItemEnum) {
        self.scan_attrs(&item.attrs);
        self.record_public_api(&item.vis, &item.attrs);
        let line = span_start_line(item.enum_token.span);
        let module_key = self.current_module_key();
        let variant_field_count = item
            .variants
            .iter()
            .map(|variant| match &variant.fields {
                Fields::Named(fields) => fields.named.len(),
                Fields::Unnamed(fields) => fields.unnamed.len(),
                Fields::Unit => 0,
            })
            .sum();
        self.types.push(TypeFact {
            type_name: item.ident.to_string(),
            qualified_name: format!("{module_key}::{}", item.ident),
            module_key,
            path: self.path.clone(),
            line,
            kind: "enum".to_string(),
            shape: "enum".to_string(),
            field_count: variant_field_count,
            variant_count: item.variants.len(),
            variant_field_count,
            declaration_span: span_line_span(item.span()),
        });
    }

    fn record_fn(&mut self, item: &ItemFn) {
        self.scan_attrs(&item.attrs);
        self.function_count += 1;
        self.record_public_api(&item.vis, &item.attrs);
        if item.sig.unsafety.is_some() {
            self.bump_escape("unsafe_fn", item.sig.fn_token.span);
            if is_public(&item.vis) && !has_safety_docs(&item.attrs) {
                self.record_finding(
                    "rust.safety.missing-safety-docs",
                    "unsafe-function",
                    item.sig.fn_token.span,
                    "public unsafe function has no # Safety documentation section",
                );
            }
        }
        if returns_container_ref(&item.sig.output) {
            self.bump_escape("container_ref_return", item.sig.output.span());
        }
        self.maybe_record_test(item);
        let name = item.sig.ident.to_string();
        self.record_function_metrics(
            name.clone(),
            format!("{}::{name}", self.current_module_key()),
            item.sig.fn_token.span,
            &item.block,
        );
    }

    fn record_foreign_mod(&mut self, item: &ItemForeignMod) {
        self.scan_attrs(&item.attrs);
        self.bump_escape("extern_block", item.abi.extern_token.span);
        for child in &item.items {
            if let ForeignItem::Fn(func) = child {
                self.bump_escape("extern_fn", func.sig.fn_token.span);
            }
        }
    }

    fn record_impl(&mut self, item: &ItemImpl) {
        self.scan_attrs(&item.attrs);
        if item.unsafety.is_some() {
            self.bump_escape("unsafe_impl", item.impl_token.span);
            if !self.has_safety_rationale(item.impl_token.span) {
                self.record_finding(
                    "rust.safety.undocumented-unsafe",
                    "unsafe-impl",
                    item.impl_token.span,
                    "unsafe impl has no nearby SAFETY rationale",
                );
            }
        }
        if let Some((_, trait_path, _)) = &item.trait_ {
            if path_ends_with(trait_path, "Deref") {
                self.bump_escape("deref_impl", trait_path.span());
            }
            if path_ends_with(trait_path, "DerefMut") {
                self.bump_escape("deref_mut_impl", trait_path.span());
            }
        }
        let method_count = item
            .items
            .iter()
            .filter(|child| matches!(child, ImplItem::Fn(_)))
            .count();
        let owner = impl_type_path(&item.self_ty).unwrap_or_else(|| "unknown".to_string());
        for child in &item.items {
            if let ImplItem::Fn(method) = child {
                self.record_method_metrics(&owner, method);
            }
        }
        if let Some(qualified_type_name) = impl_type_path(&item.self_ty) {
            let type_name = qualified_type_name
                .rsplit("::")
                .next()
                .unwrap_or(&qualified_type_name)
                .to_string();
            self.impls.push(ImplFact {
                type_name,
                qualified_type_name,
                module_key: self.current_module_key(),
                path: self.path.clone(),
                line: span_start_line(item.impl_token.span),
                method_count,
            });
        }
    }

    fn record_method_metrics(&mut self, owner: &str, method: &ImplItemFn) {
        let name = method.sig.ident.to_string();
        self.record_function_metrics(
            name.clone(),
            format!("{}::{owner}::{name}", self.current_module_key()),
            method.sig.fn_token.span,
            &method.block,
        );
    }

    fn record_item_macro(&mut self, item: &syn::ItemMacro) {
        self.scan_attrs(&item.attrs);
        let path = path_to_string(&item.mac.path);
        self.add_dependency_at(
            path.clone(),
            item.mac
                .path
                .segments
                .last()
                .map_or_else(|| item.mac.path.span(), |segment| segment.ident.span()),
        );
        let tokens = item.mac.tokens.to_string();
        if path == "include" || tokens.contains("mod ") || tokens.contains("mod\n") {
            self.unsupported_patterns.push(format!(
                "{}:{}: possible macro-generated module wiring via {}!",
                self.path,
                span_start_line(item.mac.path.span()),
                path
            ));
        }
        if is_test_generating_macro(&path)
            || tokens.contains("# [ test ]")
            || tokens.contains("#[test]")
        {
            self.unsupported_patterns.push(format!(
                "{}:{}: possible macro-generated tests via {}!",
                self.path,
                span_start_line(item.mac.path.span()),
                path
            ));
        }
    }

    fn record_mod(&mut self, item: &syn::ItemMod) {
        self.scan_attrs(&item.attrs);
        self.record_public_api(&item.vis, &item.attrs);
        let module_key = child_module_key(&self.module_key, &item.ident.to_string());
        self.child_modules.push(module_key.clone());
        if item.content.is_none()
            && let Some(path) = module_file_path(
                &self.path,
                &self.module_key,
                &item.ident.to_string(),
                &item.attrs,
            )
        {
            self.module_files.push(ModuleFileFact {
                module_key,
                path,
                line: span_start_line(item.mod_token.span),
            });
        }
        if item.ident == "tests" {
            self.has_inline_tests = true;
        }
    }

    fn record_static(&mut self, item: &ItemStatic) {
        self.scan_attrs(&item.attrs);
        self.record_public_api(&item.vis, &item.attrs);
        if matches!(item.mutability, syn::StaticMutability::Mut(_)) {
            self.bump_escape("static_mut", item.static_token.span);
        }
    }

    fn record_struct(&mut self, item: &ItemStruct) {
        self.scan_attrs(&item.attrs);
        self.record_public_api(&item.vis, &item.attrs);
        let (shape, field_count) = match &item.fields {
            Fields::Named(fields) => ("named", fields.named.len()),
            Fields::Unnamed(fields) => ("tuple", fields.unnamed.len()),
            Fields::Unit => ("unit", 0),
        };
        let line = span_start_line(item.struct_token.span);
        let module_key = self.current_module_key();
        self.types.push(TypeFact {
            type_name: item.ident.to_string(),
            qualified_name: format!("{module_key}::{}", item.ident),
            module_key,
            path: self.path.clone(),
            line,
            kind: "struct".to_string(),
            shape: shape.to_string(),
            field_count,
            variant_count: 0,
            variant_field_count: 0,
            declaration_span: span_line_span(item.span()),
        });
    }

    fn record_use(&mut self, item: &ItemUse) {
        self.scan_attrs(&item.attrs);
        self.record_public_api(&item.vis, &item.attrs);
        collect_use_tree(
            "",
            &item.tree,
            &mut self.dependencies,
            &mut self.dependency_references,
            &mut self.escape_counts,
            &mut self.escape_locations,
        );
    }
}

fn main() {
    exit_on_error(run);
}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print_usage();
        return Ok(());
    }
    if args.len() != 2 {
        print_usage();
        return Err("expected exactly one paths file argument".into());
    }

    let paths = read_paths_file(&args[1])?;
    let mut results = Vec::new();

    for path in &paths {
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(error) => {
                results.push(failed_record(path, format!("read_error: {error}")));
                continue;
            }
        };
        let file = match syn::parse_file(&content) {
            Ok(file) => file,
            Err(error) => {
                results.push(
                    FactVisitor::new(path, &content)
                        .into_facts_with_status(format!("parse_error: {error}")),
                );
                continue;
            }
        };
        let mut visitor = FactVisitor::new(path, &content);
        visitor.visit_file(&file);
        results.push(visitor.into_facts());
    }

    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}

fn failed_record(path: &str, parse_status: String) -> FileFacts {
    FileFacts {
        path: normalize_path(path),
        module_key: module_key_for_path(path),
        target_kind: target_kind_for_path(path).to_string(),
        entrypoint_kind: entrypoint_kind_for_path(path).map(str::to_string),
        is_entrypoint: entrypoint_kind_for_path(path).is_some(),
        parse_status,
        source_metrics_available: false,
        dependencies: Vec::new(),
        dependency_references: Vec::new(),
        child_modules: Vec::new(),
        module_files: Vec::new(),
        unsupported_patterns: Vec::new(),
        public_api_count: 0,
        documented_public_api_count: 0,
        has_crate_docs: false,
        has_inline_tests: false,
        source_line_count: 0,
        source_nonblank_line_count: 0,
        source_comment_line_count: 0,
        function_count: 0,
        types: Vec::new(),
        impls: Vec::new(),
        tests: Vec::new(),
        functions: Vec::new(),
        escape_counts: BTreeMap::new(),
        escape_locations: Vec::new(),
        quality_findings: Vec::new(),
    }
}

fn target_kind_for_path(path: &str) -> &'static str {
    let path = normalize_path(path);
    let relative = path.split("/src/").nth(1).map(|rest| format!("src/{rest}"));
    let path = relative.as_deref().unwrap_or(&path);
    let parts = path.split('/').collect::<Vec<_>>();
    match parts.as_slice() {
        ["src", "lib.rs"] => "lib",
        ["src", "main.rs"] => "bin",
        ["src", "bin", ..] => "bin",
        ["tests", file] if file.ends_with(".rs") => "test",
        ["benches", file] if file.ends_with(".rs") => "bench",
        ["examples", file] if file.ends_with(".rs") => "example",
        _ => "module",
    }
}

fn entrypoint_kind_for_path(path: &str) -> Option<&'static str> {
    match target_kind_for_path(path) {
        "bin" => Some("bin"),
        "test" => Some("test"),
        "bench" => Some("bench"),
        "example" => Some("example"),
        _ => None,
    }
}

fn collect_use_tree(
    prefix: &str,
    tree: &UseTree,
    dependencies: &mut Vec<String>,
    dependency_references: &mut Vec<DependencyReferenceFact>,
    escape_counts: &mut BTreeMap<String, usize>,
    escape_locations: &mut Vec<LocationFact>,
) {
    match tree {
        UseTree::Path(path) => {
            let next = join_path(prefix, &path.ident.to_string());
            dependencies.push(next.clone());
            push_dependency_reference(dependency_references, next.clone(), path.ident.span());
            collect_use_tree(
                &next,
                &path.tree,
                dependencies,
                dependency_references,
                escape_counts,
                escape_locations,
            );
        }
        UseTree::Name(name) => {
            let path = join_path(prefix, &name.ident.to_string());
            dependencies.push(path.clone());
            push_dependency_reference(dependency_references, path, name.ident.span());
        }
        UseTree::Rename(rename) => {
            let path = join_path(prefix, &rename.ident.to_string());
            dependencies.push(path.clone());
            push_dependency_reference(dependency_references, path, rename.ident.span());
        }
        UseTree::Glob(glob) => {
            dependencies.push(format!("{prefix}::*"));
            push_dependency_reference(
                dependency_references,
                format!("{prefix}::*"),
                glob.star_token.span,
            );
            *escape_counts.entry("glob_import".to_string()).or_insert(0) += 1;
            escape_locations.push(LocationFact {
                kind: "glob_import".to_string(),
                line: span_start_line(glob.star_token.span),
            });
        }
        UseTree::Group(group) => {
            for child in &group.items {
                collect_use_tree(
                    prefix,
                    child,
                    dependencies,
                    dependency_references,
                    escape_counts,
                    escape_locations,
                );
            }
        }
    }
}

fn push_dependency_reference(
    references: &mut Vec<DependencyReferenceFact>,
    raw_path: String,
    span: Span,
) {
    let start = span.start();
    references.push(DependencyReferenceFact {
        raw_path,
        line: start.line,
        column: start.column,
    });
}

fn join_path(prefix: &str, next: &str) -> String {
    if prefix.is_empty() {
        next.to_string()
    } else {
        format!("{prefix}::{next}")
    }
}

fn child_module_key(parent: &str, child: &str) -> String {
    if parent == "lib" || parent == "main" || parent.is_empty() {
        child.to_string()
    } else {
        format!("{parent}::{child}")
    }
}

fn module_file_path(
    current_path: &str,
    parent_module: &str,
    child_name: &str,
    attrs: &[Attribute],
) -> Option<String> {
    let current = Path::new(current_path);
    let current_dir = current.parent().unwrap_or_else(|| Path::new(""));
    if let Some(attr_path) = path_attr_value(attrs) {
        return Some(normalize_path(
            &current_dir.join(attr_path).to_string_lossy(),
        ));
    }

    let child_dir = if current
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "lib.rs" || name == "main.rs" || name == "mod.rs")
    {
        current_dir.to_path_buf()
    } else {
        current_dir.join(parent_module.rsplit("::").next().unwrap_or(parent_module))
    };
    let candidates = [
        child_dir.join(format!("{child_name}.rs")),
        child_dir.join(child_name).join("mod.rs"),
    ];
    candidates
        .iter()
        .find(|candidate| candidate.exists())
        .map(|candidate| normalize_path(&candidate.to_string_lossy()))
}

fn has_docs(attrs: &[Attribute]) -> bool {
    attrs
        .iter()
        .any(|attr| path_to_string(attr.path()) == "doc")
}

fn has_safety_docs(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        path_to_string(attr.path()) == "doc"
            && attr
                .meta
                .to_token_stream()
                .to_string()
                .to_ascii_lowercase()
                .contains("safety")
    })
}

fn path_attr_value(attrs: &[Attribute]) -> Option<String> {
    for attr in attrs {
        if path_to_string(attr.path()) != "path" {
            continue;
        }
        if let syn::Meta::NameValue(name_value) = &attr.meta
            && let syn::Expr::Lit(expr_lit) = &name_value.value
            && let syn::Lit::Str(lit) = &expr_lit.lit
        {
            return Some(lit.value());
        }
    }
    None
}

fn impl_type_path(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(path) => Some(path_to_string(&path.path)),
        Type::Reference(reference) => impl_type_path(&reference.elem),
        _ => None,
    }
}

fn type_dependency_string(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(path) => Some(path_to_string(&path.path)),
        Type::Reference(reference) => type_dependency_string(&reference.elem),
        Type::Ptr(pointer) => type_dependency_string(&pointer.elem),
        Type::Slice(slice) => type_dependency_string(&slice.elem),
        Type::Array(array) => type_dependency_string(&array.elem),
        Type::Paren(paren) => type_dependency_string(&paren.elem),
        Type::Group(group) => type_dependency_string(&group.elem),
        _ => None,
    }
}

fn returns_container_ref(output: &ReturnType) -> bool {
    let ReturnType::Type(_, ty) = output else {
        return false;
    };
    let Type::Reference(reference) = ty.as_ref() else {
        return false;
    };
    if reference.mutability.is_some() {
        return false;
    }
    let Type::Path(path) = reference.elem.as_ref() else {
        return false;
    };
    path.path
        .segments
        .last()
        .map(|segment| {
            matches!(
                segment.ident.to_string().as_str(),
                "Vec"
                    | "HashMap"
                    | "BTreeMap"
                    | "HashSet"
                    | "BTreeSet"
                    | "Option"
                    | "Box"
                    | "Rc"
                    | "Arc"
                    | "String"
            )
        })
        .unwrap_or(false)
}

fn is_public(vis: &syn::Visibility) -> bool {
    matches!(vis, syn::Visibility::Public(_))
}

fn has_test_attrs(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        let path = path_to_string(attr.path());
        is_test_attribute(&path)
            || (path == "cfg" && attr.meta.to_token_stream().to_string().contains("test"))
    })
}

fn is_test_attribute(path: &str) -> bool {
    let tail = path.rsplit("::").next().unwrap_or(path);
    matches!(
        tail,
        "test" | "rstest" | "test_case" | "wasm_bindgen_test" | "quickcheck"
    ) || path.ends_with("::test")
        || path.ends_with("::rstest")
        || path.ends_with("::test_case")
}

fn is_test_generating_macro(path: &str) -> bool {
    let tail = path.rsplit("::").next().unwrap_or(path);
    matches!(
        tail,
        "proptest" | "quickcheck" | "parameterized" | "rstest_reuse" | "test_suite" | "test_matrix"
    )
}

fn path_to_string(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

fn path_ends_with(path: &syn::Path, tail: &str) -> bool {
    path.segments
        .last()
        .map(|segment| segment.ident == tail)
        .unwrap_or(false)
}

fn span_start_line(span: Span) -> usize {
    span.start().line
}

fn span_line_span(span: Span) -> usize {
    let start = span.start().line;
    let end = span.end().line;
    end.saturating_sub(start) + 1
}

fn print_usage() {
    eprintln!("Usage: rust_facts <paths_file>");
}

#[cfg(test)]
mod tests {
    use super::FactVisitor;
    use syn::visit::Visit;

    fn facts(source: &str) -> super::FileFacts {
        let Ok(file) = syn::parse_file(source) else {
            panic!("test source should parse");
        };
        let mut visitor = FactVisitor::new("src/lib.rs", source);
        visitor.visit_file(&file);
        visitor.into_facts()
    }

    #[test]
    fn local_identifiers_do_not_become_module_dependencies() {
        let facts = facts(
            r#"
mod service;
fn local() {
    let service = 1;
    let _ = service;
    crate::service::run();
}
"#,
        );
        assert!(
            facts
                .dependencies
                .iter()
                .any(|dependency| dependency == "crate::service::run")
        );
        assert!(
            !facts
                .dependencies
                .iter()
                .any(|dependency| dependency == "service")
        );
    }

    #[test]
    fn public_reexports_contribute_to_api_surface() {
        let undocumented = facts("pub use crate::service::run;");
        assert_eq!(undocumented.public_api_count, 1);
        assert_eq!(undocumented.documented_public_api_count, 0);

        let documented = facts("/// Runs the service.\npub use crate::service::run;");
        assert_eq!(documented.documented_public_api_count, 1);
    }

    #[test]
    fn safety_and_panic_findings_keep_source_evidence() {
        let facts = facts(
            r#"
pub unsafe fn raw(pointer: *const i32) -> i32 {
    unsafe { *pointer }
}

fn load(value: Option<i32>) -> i32 {
    value.unwrap()
}
"#,
        );
        assert!(facts.quality_findings.iter().any(|finding| {
            finding.rule_id == "rust.safety.missing-safety-docs" && finding.line == 2
        }));
        assert!(facts.quality_findings.iter().any(|finding| {
            finding.rule_id == "rust.safety.undocumented-unsafe" && finding.line == 3
        }));
        assert!(
            facts
                .quality_findings
                .iter()
                .any(|finding| finding.rule_id == "rust.reliability.unwrap")
        );
    }

    #[test]
    fn reliability_findings_identify_test_scopes() {
        let facts = facts(
            r#"
#[cfg(test)]
mod checks {
    fn helper(value: Option<i32>) -> i32 { value.unwrap() }
}

#[test]
fn direct_test() { panic!("expected test panic"); }
"#,
        );
        let test_scoped = facts
            .quality_findings
            .iter()
            .filter(|finding| finding.test_code)
            .count();
        assert_eq!(test_scoped, 2);
        assert_eq!(test_scoped, facts.quality_findings.len());
    }

    #[test]
    fn safety_comments_and_docs_satisfy_contract_evidence() {
        let facts = facts(
            r#"
/// # Safety
/// The pointer must be valid.
pub unsafe fn raw(pointer: *const i32) -> i32 {
    // SAFETY: the caller guarantees pointer validity.
    unsafe { *pointer }
}
"#,
        );
        assert!(facts.quality_findings.is_empty());
    }

    #[test]
    fn function_metrics_preserve_nesting_and_paths() {
        let facts = facts(
            r#"
fn nested(left: bool, right: bool) {
    if left && right {
        while left {
            if right { break; }
        }
    }
}
"#,
        );
        let Some(function) = facts.functions.first() else {
            panic!("function fact should exist");
        };
        assert_eq!(function.name, "nested");
        assert_eq!(function.max_nesting_depth, 3);
        assert!(function.branch_pressure >= 6);
        assert!(function.path_pressure >= 4);
        assert_eq!(function.cyclomatic_complexity, 5);
        assert_eq!(function.cognitive_complexity, 7);
    }

    #[test]
    fn syntax_failures_preserve_text_source_metrics() {
        let source = "//! Partial file\nfn broken( {\n    // retained comment\n";
        let facts = FactVisitor::new("src/broken.rs", source)
            .into_facts_with_status("parse_error: expected pattern".to_string());

        assert!(facts.source_metrics_available);
        assert!(facts.parse_status.starts_with("parse_error:"));
        assert_eq!(facts.source_line_count, 3);
        assert_eq!(facts.source_nonblank_line_count, 1);
        assert_eq!(facts.source_comment_line_count, 2);
        assert!(facts.has_crate_docs);
        assert!(facts.functions.is_empty());
    }
}
