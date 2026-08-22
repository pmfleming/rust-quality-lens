use quote::ToTokens;
use rust_quality_lens_helpers::{exit_on_error, qualify, read_paths_file, stable_hash};
use serde::Serialize;
use std::env;
use std::error::Error;
use std::fs;
use syn::{
    Expr, ImplItemFn, ItemFn, ItemMod, Lit, Stmt, Type, UnOp,
    visit::{self, Visit},
};

const MIN_AST_NODES: usize = 10;

#[derive(Serialize)]
struct FnInfo {
    name: String,
    qualified_name: String,
    file: String,
    start_line: usize,
    end_line: usize,
    node_count: usize,
    ast_hash: String,
}

#[derive(Default)]
struct FunctionCollector {
    file: String,
    module_stack: Vec<String>,
    functions: Vec<FnInfo>,
}

impl FunctionCollector {
    fn push_function(&mut self, name: String, span: proc_macro2::Span, body: &syn::Block) {
        let mut normalizer = AstNormalizer::default();
        normalizer.visit_block(body);
        if normalizer.node_count < MIN_AST_NODES {
            return;
        }
        let qualified_name = qualify(&self.module_stack, &name);
        self.functions.push(FnInfo {
            name,
            qualified_name,
            file: self.file.clone(),
            start_line: span.start().line,
            end_line: body.brace_token.span.close().end().line,
            node_count: normalizer.node_count,
            ast_hash: stable_hash(&normalizer.signature),
        });
    }

    fn push_signature(&mut self, signature: &syn::Signature, body: &syn::Block) {
        self.push_function(signature.ident.to_string(), signature.fn_token.span, body);
    }
}

impl<'ast> Visit<'ast> for FunctionCollector {
    fn visit_item_mod(&mut self, item: &'ast ItemMod) {
        self.module_stack.push(item.ident.to_string());
        visit::visit_item_mod(self, item);
        self.module_stack.pop();
    }

    fn visit_item_fn(&mut self, item: &'ast ItemFn) {
        self.push_signature(&item.sig, &item.block);
    }

    fn visit_impl_item_fn(&mut self, item: &'ast ImplItemFn) {
        self.push_signature(&item.sig, &item.block);
    }
}

#[derive(Default)]
struct AstNormalizer {
    signature: String,
    node_count: usize,
}

impl AstNormalizer {
    fn tag(&mut self, value: impl AsRef<str>) {
        self.node_count += 1;
        self.signature.push_str(value.as_ref());
        self.signature.push('|');
    }

    fn path_tail(path: &syn::Path) -> String {
        path.segments
            .last()
            .map(|segment| segment.ident.to_string())
            .unwrap_or_default()
    }
}

impl<'ast> Visit<'ast> for AstNormalizer {
    fn visit_expr(&mut self, expr: &'ast Expr) {
        let tag = flow_expr_tag(expr)
            .or_else(|| value_expr_tag(expr))
            .unwrap_or_else(|| detailed_expr_tag(expr));
        self.tag(tag);
        visit::visit_expr(self, expr);
    }

    fn visit_stmt(&mut self, stmt: &'ast Stmt) {
        self.tag(match stmt {
            Stmt::Local(_) => "stmt:local",
            Stmt::Item(_) => "stmt:item",
            Stmt::Expr(_, Some(_)) => "stmt:expr-semi",
            Stmt::Expr(_, None) => "stmt:expr",
            Stmt::Macro(_) => "stmt:macro",
        });
        visit::visit_stmt(self, stmt);
    }

    fn visit_type(&mut self, ty: &'ast Type) {
        match ty {
            Type::Path(path) => self.tag(format!("type:path:{}", Self::path_tail(&path.path))),
            Type::Reference(_) => self.tag("type:reference"),
            Type::Tuple(_) => self.tag("type:tuple"),
            _ => self.tag("type:other"),
        }
        visit::visit_type(self, ty);
    }

    fn visit_lit(&mut self, lit: &'ast Lit) {
        self.tag(match lit {
            Lit::Str(_) => "lit:str",
            Lit::ByteStr(_) => "lit:bytes",
            Lit::Byte(_) => "lit:byte",
            Lit::Char(_) => "lit:char",
            Lit::Int(_) => "lit:int",
            Lit::Float(_) => "lit:float",
            Lit::Bool(_) => "lit:bool",
            Lit::Verbatim(_) => "lit:verbatim",
            _ => "lit:other",
        });
    }
}

fn flow_expr_tag(expr: &Expr) -> Option<String> {
    match expr {
        Expr::If(_) => Some("expr:if".to_string()),
        Expr::Loop(_) => Some("expr:loop".to_string()),
        Expr::Match(_) => Some("expr:match".to_string()),
        Expr::Return(_) => Some("expr:return".to_string()),
        Expr::Try(_) => Some("expr:try".to_string()),
        Expr::While(_) => Some("expr:while".to_string()),
        _ => None,
    }
}

fn value_expr_tag(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Call(_) => Some("expr:call".to_string()),
        Expr::Cast(_) => Some("expr:cast".to_string()),
        Expr::Index(_) => Some("expr:index".to_string()),
        Expr::Let(_) => Some("expr:let".to_string()),
        Expr::Lit(_) => Some("expr:lit".to_string()),
        Expr::Path(_) => Some("expr:path".to_string()),
        Expr::Reference(_) => Some("expr:reference".to_string()),
        Expr::Tuple(_) => Some("expr:tuple".to_string()),
        _ => None,
    }
}

fn detailed_expr_tag(expr: &Expr) -> String {
    match expr {
        Expr::Binary(binary) => format!("expr:binary:{}", binary.op.to_token_stream()),
        Expr::Field(field) => format!("expr:field:{}", field.member.to_token_stream()),
        Expr::Macro(mac) => format!("expr:macro:{}", AstNormalizer::path_tail(&mac.mac.path)),
        Expr::MethodCall(method) => format!("expr:method:{}", method.method),
        Expr::Struct(expr_struct) => {
            format!(
                "expr:struct:{}",
                AstNormalizer::path_tail(&expr_struct.path)
            )
        }
        Expr::Unary(unary) => format!("expr:unary:{}", unary_op_tag(&unary.op)),
        _ => format!("expr:{}", expr.to_token_stream()),
    }
}

fn unary_op_tag(op: &UnOp) -> &'static str {
    match op {
        UnOp::Deref(_) => "deref",
        UnOp::Not(_) => "not",
        UnOp::Neg(_) => "neg",
        _ => "other",
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

    let mut results = Vec::new();
    for path in read_paths_file(&args[1])? {
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) => {
                eprintln!("Error reading file '{path}': {error}");
                continue;
            }
        };
        let file = match syn::parse_file(&content) {
            Ok(file) => file,
            Err(error) => {
                eprintln!("Error parsing file '{path}': {error}");
                continue;
            }
        };
        let mut collector = FunctionCollector {
            file: path,
            ..FunctionCollector::default()
        };
        collector.visit_file(&file);
        results.extend(collector.functions);
    }

    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}

fn print_usage() {
    eprintln!("Usage: ast_hasher <paths_file>");
}

#[cfg(test)]
mod tests {
    use super::{FnInfo, FunctionCollector};
    use syn::visit::Visit;

    fn collect(source: &str) -> syn::Result<Vec<FnInfo>> {
        let file = syn::parse_file(source)?;
        let mut collector = FunctionCollector {
            file: "src/lib.rs".to_string(),
            ..FunctionCollector::default()
        };
        collector.visit_file(&file);
        Ok(collector.functions)
    }

    #[test]
    fn records_nested_functions_and_impl_methods() -> syn::Result<()> {
        let facts = collect(
            r#"
mod inner {
    pub fn nested(input: i32) -> i32 {
        let value = input + 1;
        if value > 0 { value } else { 0 }
    }
}

struct Thing;
impl Thing {
    fn method(&self, input: i32) -> i32 {
        let value = input + 1;
        if value > 0 { value } else { 0 }
    }
}
"#,
        )?;
        assert!(
            facts
                .iter()
                .any(|item| item.qualified_name == "inner::nested")
        );
        assert!(facts.iter().any(|item| item.qualified_name == "method"));
        Ok(())
    }

    #[test]
    fn hash_distinguishes_operators_but_normalizes_names() -> syn::Result<()> {
        let same_shape = collect(
            r#"
fn add_one(input: i32) -> i32 {
    let value = input + 1;
    if value > 0 { value } else { 0 }
}

fn add_two(other: i32) -> i32 {
    let renamed = other + 2;
    if renamed > 3 { renamed } else { 0 }
}

fn subtract_one(input: i32) -> i32 {
    let value = input - 1;
    if value > 0 { value } else { 0 }
}
"#,
        )?;
        let hash = |name: &str| {
            same_shape
                .iter()
                .find(|item| item.name == name)
                .map(|item| item.ast_hash.as_str())
        };
        assert_eq!(hash("add_one"), hash("add_two"));
        assert_ne!(hash("add_one"), hash("subtract_one"));
        assert!(hash("add_one").is_some());
        Ok(())
    }
}
