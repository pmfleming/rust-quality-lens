// Experimental helper for AST-based clone research.
// Wired through clone_alert.py when the experimental engine is requested.

use proc_macro2::Span;
use serde::Serialize;
use std::collections::hash_map::DefaultHasher;
use std::env;
use std::fs;
use std::hash::{Hash, Hasher};
use syn::{
    Expr, ItemFn, Lit,
    visit::{self, Visit},
};

#[derive(Serialize)]
struct FnInfo {
    name: String,
    file: String,
    start_line: usize,
    end_line: usize,
    ast_hash: String,
}

struct AstNormalizer {
    hasher: DefaultHasher,
}

impl<'ast> Visit<'ast> for AstNormalizer {
    fn visit_item_fn(&mut self, i: &'ast ItemFn) {
        // Skip function name during hash to detect clones with different names
        // But visit parameters and body
        for param in &i.sig.inputs {
            visit::visit_fn_arg(self, param);
        }
        visit::visit_block(self, &i.block);
    }

    fn visit_expr(&mut self, i: &'ast Expr) {
        // Hash the "type" of expression but maybe not the specific ident if we want Type-3
        // For now, let's hash a simplified structure
        std::mem::discriminant(i).hash(&mut self.hasher);
        visit::visit_expr(self, i);
    }

    fn visit_stmt(&mut self, i: &'ast syn::Stmt) {
        std::mem::discriminant(i).hash(&mut self.hasher);
        visit::visit_stmt(self, i);
    }

    fn visit_macro(&mut self, i: &'ast syn::Macro) {
        if let Some(segment) = i.path.segments.last() {
            segment.ident.to_string().hash(&mut self.hasher);
        }
        visit::visit_macro(self, i);
    }

    fn visit_lit(&mut self, _i: &'ast Lit) {
        // Normalize literals: "LIT".hash
        "LIT".hash(&mut self.hasher);
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: ast_hasher <paths_file>");
        return;
    }

    let paths_file = &args[1];
    let paths_content = match fs::read_to_string(paths_file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading paths file '{}': {}", paths_file, e);
            return;
        }
    };

    let mut results = Vec::new();

    for line in paths_content.lines() {
        let path = line.trim();
        if path.is_empty() {
            continue;
        }

        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Error reading file '{}': {}", path, e);
                continue;
            }
        };

        let file = match syn::parse_file(&content) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Error parsing file '{}': {}", path, e);
                continue;
            }
        };

        for item in file.items {
            if let syn::Item::Fn(func) = item {
                let mut normalizer = AstNormalizer {
                    hasher: DefaultHasher::new(),
                };
                normalizer.visit_item_fn(&func);

                let hash = format!("{:x}", normalizer.hasher.finish());
                let start_line = span_start_line(func.sig.fn_token.span);
                let end_line = span_end_line(func.block.brace_token.span.close());

                results.push(FnInfo {
                    name: func.sig.ident.to_string(),
                    file: path.to_string(),
                    start_line,
                    end_line,
                    ast_hash: hash,
                });
            }
        }
    }

    println!("{}", serde_json::to_string_pretty(&results).unwrap());
}

fn span_start_line(span: Span) -> usize {
    span.start().line
}

fn span_end_line(span: Span) -> usize {
    span.end().line
}
