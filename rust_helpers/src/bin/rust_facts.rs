use proc_macro2::Span;
use quote::ToTokens;
use serde::Serialize;
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fs;
use std::path::Path;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{
    Attribute, ExprCall, ExprPath, ExprRawAddr, Fields, ForeignItem, ImplItem, Item, ItemEnum,
    ItemFn, ItemForeignMod, ItemImpl, ItemStatic, ItemStruct, ItemUse, ReturnType, Type, TypePath,
    UseTree,
};

#[derive(Serialize)]
struct FileFacts {
    path: String,
    module_key: String,
    parse_status: String,
    dependencies: Vec<String>,
    child_modules: Vec<String>,
    module_files: Vec<ModuleFileFact>,
    unsupported_patterns: Vec<String>,
    public_api_count: usize,
    has_inline_tests: bool,
    types: Vec<TypeFact>,
    impls: Vec<ImplFact>,
    tests: Vec<TestFact>,
    escape_counts: BTreeMap<String, usize>,
    escape_locations: Vec<LocationFact>,
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

struct FactVisitor {
    path: String,
    module_key: String,
    dependencies: Vec<String>,
    child_modules: Vec<String>,
    module_files: Vec<ModuleFileFact>,
    unsupported_patterns: Vec<String>,
    module_stack: Vec<String>,
    public_api_count: usize,
    has_inline_tests: bool,
    types: Vec<TypeFact>,
    impls: Vec<ImplFact>,
    tests: Vec<TestFact>,
    escape_counts: BTreeMap<String, usize>,
    escape_locations: Vec<LocationFact>,
}

impl FactVisitor {
    fn new(path: &str) -> Self {
        Self {
            path: normalize_path(path),
            module_key: module_key_for_path(path),
            dependencies: Vec::new(),
            child_modules: Vec::new(),
            module_files: Vec::new(),
            unsupported_patterns: Vec::new(),
            module_stack: Vec::new(),
            public_api_count: 0,
            has_inline_tests: false,
            types: Vec::new(),
            impls: Vec::new(),
            tests: Vec::new(),
            escape_counts: BTreeMap::new(),
            escape_locations: Vec::new(),
        }
    }

    fn into_facts(mut self) -> FileFacts {
        self.dependencies.sort();
        self.dependencies.dedup();
        self.child_modules.sort();
        self.child_modules.dedup();
        self.module_files
            .sort_by(|a, b| a.module_key.cmp(&b.module_key));
        self.unsupported_patterns.sort();
        self.unsupported_patterns.dedup();
        FileFacts {
            path: self.path,
            module_key: self.module_key,
            parse_status: "ok".to_string(),
            dependencies: self.dependencies,
            child_modules: self.child_modules,
            module_files: self.module_files,
            unsupported_patterns: self.unsupported_patterns,
            public_api_count: self.public_api_count,
            has_inline_tests: self.has_inline_tests,
            types: self.types,
            impls: self.impls,
            tests: self.tests,
            escape_counts: self.escape_counts,
            escape_locations: self.escape_locations,
        }
    }

    fn bump_escape(&mut self, kind: &str, span: Span) {
        *self.escape_counts.entry(kind.to_string()).or_insert(0) += 1;
        self.escape_locations.push(LocationFact {
            kind: kind.to_string(),
            line: span_start_line(span),
        });
    }

    fn add_dependency(&mut self, path: String) {
        if !path.is_empty() {
            self.dependencies.push(path);
        }
    }

    fn scan_attrs(&mut self, attrs: &[Attribute]) {
        for attr in attrs {
            let path = path_to_string(attr.path());
            self.add_dependency(path.clone());
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
        if self.module_stack.is_empty() {
            name.to_string()
        } else {
            format!("{}::{name}", self.module_stack.join("::"))
        }
    }
}

impl<'ast> Visit<'ast> for FactVisitor {
    fn visit_item(&mut self, i: &'ast Item) {
        match i {
            Item::Const(item) => {
                self.scan_attrs(&item.attrs);
                if is_public(&item.vis) {
                    self.public_api_count += 1;
                }
            }
            Item::Enum(item) => self.record_enum(item),
            Item::Fn(item) => self.record_fn(item),
            Item::ForeignMod(item) => self.record_foreign_mod(item),
            Item::Impl(item) => self.record_impl(item),
            Item::Macro(item) => self.record_item_macro(item),
            Item::Mod(item) => {
                self.record_mod(item);
                if item.content.is_some() {
                    self.module_stack.push(item.ident.to_string());
                    visit::visit_item(self, i);
                    self.module_stack.pop();
                } else {
                    visit::visit_item(self, i);
                }
                return;
            }
            Item::Static(item) => self.record_static(item),
            Item::Struct(item) => self.record_struct(item),
            Item::Trait(item) => {
                self.scan_attrs(&item.attrs);
                if is_public(&item.vis) {
                    self.public_api_count += 1;
                }
                if item.unsafety.is_some() {
                    self.bump_escape("unsafe_trait", item.trait_token.span);
                }
            }
            Item::Type(item) => {
                self.scan_attrs(&item.attrs);
                if is_public(&item.vis) {
                    self.public_api_count += 1;
                }
            }
            Item::Union(item) => {
                self.scan_attrs(&item.attrs);
                if is_public(&item.vis) {
                    self.public_api_count += 1;
                }
                self.bump_escape("union", item.union_token.span);
            }
            Item::Use(item) => self.record_use(item),
            _ => {}
        }
        visit::visit_item(self, i);
    }

    fn visit_expr_call(&mut self, i: &'ast ExprCall) {
        if let syn::Expr::Path(path) = i.func.as_ref() {
            let value = path_to_string(&path.path);
            if path_ends_with(&path.path, "transmute")
                || path_ends_with(&path.path, "transmute_copy")
            {
                self.bump_escape("transmute", path.path.span());
            }
            self.add_dependency(value);
        }
        visit::visit_expr_call(self, i);
    }

    fn visit_expr_path(&mut self, i: &'ast ExprPath) {
        self.add_dependency(path_to_string(&i.path));
        visit::visit_expr_path(self, i);
    }

    fn visit_expr_raw_addr(&mut self, i: &'ast ExprRawAddr) {
        self.bump_escape("raw_borrow", i.and_token.span);
        visit::visit_expr_raw_addr(self, i);
    }

    fn visit_expr_unsafe(&mut self, i: &'ast syn::ExprUnsafe) {
        self.bump_escape("unsafe_block", i.unsafe_token.span);
        visit::visit_expr_unsafe(self, i);
    }

    fn visit_macro(&mut self, i: &'ast syn::Macro) {
        let path = path_to_string(&i.path);
        self.add_dependency(path.clone());
        if path == "asm" || path == "global_asm" {
            self.bump_escape("asm_macro", i.path.span());
        }
        visit::visit_macro(self, i);
    }

    fn visit_path(&mut self, i: &'ast syn::Path) {
        self.add_dependency(path_to_string(i));
        visit::visit_path(self, i);
    }

    fn visit_type_path(&mut self, i: &'ast TypePath) {
        let path = path_to_string(&i.path);
        if path_ends_with(&i.path, "MaybeUninit") {
            self.bump_escape("maybe_uninit", i.path.span());
        }
        if let Some(qself) = &i.qself {
            if let Some(value) = type_dependency_string(&qself.ty) {
                self.add_dependency(value);
            }
        }
        self.add_dependency(path);
        visit::visit_type_path(self, i);
    }
}

impl FactVisitor {
    fn record_enum(&mut self, item: &ItemEnum) {
        self.scan_attrs(&item.attrs);
        if is_public(&item.vis) {
            self.public_api_count += 1;
        }
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
        if is_public(&item.vis) {
            self.public_api_count += 1;
        }
        if item.sig.unsafety.is_some() {
            self.bump_escape("unsafe_fn", item.sig.fn_token.span);
        }
        if returns_container_ref(&item.sig.output) {
            self.bump_escape("container_ref_return", item.sig.output.span());
        }
        self.maybe_record_test(item);
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
        if let Some(type_name) = impl_type_name(&item.self_ty) {
            self.impls.push(ImplFact {
                type_name,
                module_key: self.current_module_key(),
                path: self.path.clone(),
                line: span_start_line(item.impl_token.span),
                method_count,
            });
        }
    }

    fn record_item_macro(&mut self, item: &syn::ItemMacro) {
        self.scan_attrs(&item.attrs);
        let path = path_to_string(&item.mac.path);
        self.add_dependency(path.clone());
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
        if is_public(&item.vis) {
            self.public_api_count += 1;
        }
        let module_key = child_module_key(&self.module_key, &item.ident.to_string());
        self.child_modules.push(module_key.clone());
        if item.content.is_none() {
            if let Some(path) = module_file_path(
                &self.path,
                &self.module_key,
                &item.ident.to_string(),
                &item.attrs,
            ) {
                self.module_files.push(ModuleFileFact {
                    module_key,
                    path,
                    line: span_start_line(item.mod_token.span),
                });
            }
        }
        if item.ident == "tests" {
            self.has_inline_tests = true;
        }
    }

    fn record_static(&mut self, item: &ItemStatic) {
        self.scan_attrs(&item.attrs);
        if is_public(&item.vis) {
            self.public_api_count += 1;
        }
        if matches!(item.mutability, syn::StaticMutability::Mut(_)) {
            self.bump_escape("static_mut", item.static_token.span);
        }
    }

    fn record_struct(&mut self, item: &ItemStruct) {
        self.scan_attrs(&item.attrs);
        if is_public(&item.vis) {
            self.public_api_count += 1;
        }
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
        collect_use_tree(
            "",
            &item.tree,
            &mut self.dependencies,
            &mut self.escape_counts,
            &mut self.escape_locations,
        );
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
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

    let paths_content = fs::read_to_string(&args[1])
        .map_err(|error| format!("error reading paths file '{}': {error}", args[1]))?;
    let mut results = Vec::new();

    for line in paths_content.lines() {
        let path = line.trim();
        if path.is_empty() {
            continue;
        }
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
                results.push(failed_record(path, format!("parse_error: {error}")));
                continue;
            }
        };
        let mut visitor = FactVisitor::new(path);
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
        parse_status,
        dependencies: Vec::new(),
        child_modules: Vec::new(),
        module_files: Vec::new(),
        unsupported_patterns: Vec::new(),
        public_api_count: 0,
        has_inline_tests: false,
        types: Vec::new(),
        impls: Vec::new(),
        tests: Vec::new(),
        escape_counts: BTreeMap::new(),
        escape_locations: Vec::new(),
    }
}

fn collect_use_tree(
    prefix: &str,
    tree: &UseTree,
    dependencies: &mut Vec<String>,
    escape_counts: &mut BTreeMap<String, usize>,
    escape_locations: &mut Vec<LocationFact>,
) {
    match tree {
        UseTree::Path(path) => {
            let next = join_path(prefix, &path.ident.to_string());
            dependencies.push(next.clone());
            collect_use_tree(
                &next,
                &path.tree,
                dependencies,
                escape_counts,
                escape_locations,
            );
        }
        UseTree::Name(name) => dependencies.push(join_path(prefix, &name.ident.to_string())),
        UseTree::Rename(rename) => dependencies.push(join_path(prefix, &rename.ident.to_string())),
        UseTree::Glob(glob) => {
            dependencies.push(format!("{prefix}::*"));
            *escape_counts.entry("glob_import".to_string()).or_insert(0) += 1;
            escape_locations.push(LocationFact {
                kind: "glob_import".to_string(),
                line: span_start_line(glob.star_token.span),
            });
        }
        UseTree::Group(group) => {
            for child in &group.items {
                collect_use_tree(prefix, child, dependencies, escape_counts, escape_locations);
            }
        }
    }
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

fn path_attr_value(attrs: &[Attribute]) -> Option<String> {
    for attr in attrs {
        if path_to_string(attr.path()) != "path" {
            continue;
        }
        if let syn::Meta::NameValue(name_value) = &attr.meta {
            if let syn::Expr::Lit(expr_lit) = &name_value.value {
                if let syn::Lit::Str(lit) = &expr_lit.lit {
                    return Some(lit.value());
                }
            }
        }
    }
    None
}

fn impl_type_name(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(path) => path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string()),
        Type::Reference(reference) => impl_type_name(&reference.elem),
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

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn module_key_for_path(path: &str) -> String {
    let normalized = normalize_path(path);
    let mut rel = normalized.as_str();
    if let Some(roots) = env::var_os("RQLENS_SOURCE_ROOTS") {
        for root in env::split_paths(&roots) {
            let root = normalize_path(&root.to_string_lossy());
            let root = root.trim_end_matches('/');
            if let Some(rest) = normalized.strip_prefix(&format!("{root}/")) {
                rel = rest;
                break;
            }
        }
    }
    let without_src = rel.strip_prefix("src/").unwrap_or(rel);
    let without_extension = without_src.strip_suffix(".rs").unwrap_or(without_src);
    let module_path = Path::new(without_extension);
    module_path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .filter(|part| *part != "mod")
        .collect::<Vec<_>>()
        .join("::")
}

fn print_usage() {
    eprintln!("Usage: rust_facts <paths_file>");
}
