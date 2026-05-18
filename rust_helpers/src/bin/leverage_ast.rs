use serde::Serialize;
use std::env;
use std::error::Error;
use std::fs;
use std::path::Path;
use syn::visit::{self, Visit};
use syn::{ExprForLoop, ExprMethodCall, TypePath};

#[derive(Serialize)]
struct LeverageRecord {
    path: String,
    module_key: String,
    module_name: String,
    heap_allocating_type_count: usize,
    inline_type_count: usize,
    iterator_method_count: usize,
    for_loop_count: usize,
    unsafe_expr_count: usize,
    unsafe_fn_count: usize,
    unsafe_trait_or_impl_count: usize,
    parse_status: String,
    indirection_ratio: f64,
    iterator_leverage_score: f64,
    unsafe_blocks: usize,
    total_leverage_score: f64,
    signals: Vec<String>,
}

#[derive(Clone, Copy, Default)]
struct LeverageCounts {
    heap_allocating_type_count: usize,
    inline_type_count: usize,
    iterator_method_count: usize,
    for_loop_count: usize,
    unsafe_expr_count: usize,
    unsafe_fn_count: usize,
    unsafe_trait_or_impl_count: usize,
}

struct LeverageScores {
    indirection_ratio: f64,
    iterator_leverage_score: f64,
    unsafe_blocks: usize,
    total_leverage_score: f64,
    signals: Vec<String>,
}

impl LeverageCounts {
    fn unsafe_blocks(self) -> usize {
        self.unsafe_expr_count + self.unsafe_fn_count + self.unsafe_trait_or_impl_count
    }
}

struct LeverageVisitor {
    counts: LeverageCounts,
}

impl LeverageVisitor {
    fn new() -> Self {
        Self {
            counts: LeverageCounts::default(),
        }
    }
}

impl<'ast> Visit<'ast> for LeverageVisitor {
    fn visit_type_path(&mut self, i: &'ast TypePath) {
        if let Some(segment) = i.path.segments.last() {
            let ident = segment.ident.to_string();
            match ident.as_str() {
                "Box" | "Rc" | "Arc" | "Mutex" | "RwLock" => {
                    self.counts.heap_allocating_type_count += 1;
                }
                "Vec" | "Option" | "Result" | "String" | "HashMap" | "HashSet" => {
                    self.counts.inline_type_count += 1;
                }
                _ => {}
            }
        }
        visit::visit_type_path(self, i);
    }

    fn visit_expr_for_loop(&mut self, i: &'ast ExprForLoop) {
        self.counts.for_loop_count += 1;
        visit::visit_expr_for_loop(self, i);
    }

    fn visit_expr_method_call(&mut self, i: &'ast ExprMethodCall) {
        let method_name = i.method.to_string();
        if [
            "iter",
            "into_iter",
            "iter_mut",
            "map",
            "filter",
            "fold",
            "reduce",
            "collect",
        ]
        .contains(&method_name.as_str())
        {
            self.counts.iterator_method_count += 1;
        }
        visit::visit_expr_method_call(self, i);
    }

    fn visit_expr_unsafe(&mut self, i: &'ast syn::ExprUnsafe) {
        self.counts.unsafe_expr_count += 1;
        visit::visit_expr_unsafe(self, i);
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if i.unsafety.is_some() {
            self.counts.unsafe_trait_or_impl_count += 1;
        }
        visit::visit_item_impl(self, i);
    }

    fn visit_item_trait(&mut self, i: &'ast syn::ItemTrait) {
        if i.unsafety.is_some() {
            self.counts.unsafe_trait_or_impl_count += 1;
        }
        visit::visit_item_trait(self, i);
    }

    fn visit_item_fn(&mut self, i: &'ast syn::ItemFn) {
        if i.sig.unsafety.is_some() {
            self.counts.unsafe_fn_count += 1;
        }
        visit::visit_item_fn(self, i);
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

    let paths_file = &args[1];
    let paths_content = fs::read_to_string(paths_file)
        .map_err(|error| format!("error reading paths file '{paths_file}': {error}"))?;

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
                results.push(failed_record(path, "read_error", format!("read error {e}")));
                continue;
            }
        };

        let file = match syn::parse_file(&content) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Error parsing file '{}': {}", path, e);
                results.push(failed_record(
                    path,
                    "parse_error",
                    format!("parse error {e}"),
                ));
                continue;
            }
        };

        let mut visitor = LeverageVisitor::new();
        visitor.visit_file(&file);

        let counts = visitor.counts;
        let scores = score_leverage(&counts);
        results.push(LeverageRecord {
            path: normalize_path(path),
            module_key: module_key_for_path(path),
            module_name: normalize_path(path),
            heap_allocating_type_count: counts.heap_allocating_type_count,
            inline_type_count: counts.inline_type_count,
            iterator_method_count: counts.iterator_method_count,
            for_loop_count: counts.for_loop_count,
            unsafe_expr_count: counts.unsafe_expr_count,
            unsafe_fn_count: counts.unsafe_fn_count,
            unsafe_trait_or_impl_count: counts.unsafe_trait_or_impl_count,
            parse_status: "ok".to_string(),
            indirection_ratio: scores.indirection_ratio,
            iterator_leverage_score: scores.iterator_leverage_score,
            unsafe_blocks: scores.unsafe_blocks,
            total_leverage_score: scores.total_leverage_score,
            signals: scores.signals,
        });
    }

    let json = serde_json::to_string_pretty(&results)?;
    println!("{json}");
    Ok(())
}

fn failed_record(path: &str, parse_status: &str, signal: String) -> LeverageRecord {
    LeverageRecord {
        path: normalize_path(path),
        module_key: module_key_for_path(path),
        module_name: normalize_path(path),
        heap_allocating_type_count: 0,
        inline_type_count: 0,
        iterator_method_count: 0,
        for_loop_count: 0,
        unsafe_expr_count: 0,
        unsafe_fn_count: 0,
        unsafe_trait_or_impl_count: 0,
        parse_status: parse_status.to_string(),
        indirection_ratio: 0.0,
        iterator_leverage_score: 0.0,
        unsafe_blocks: 0,
        total_leverage_score: 0.0,
        signals: vec![signal],
    }
}

fn print_usage() {
    eprintln!("Usage: leverage_ast <paths_file>");
}

fn score_leverage(counts: &LeverageCounts) -> LeverageScores {
    let total_types = counts.heap_allocating_type_count + counts.inline_type_count;
    let indirection_ratio = if total_types > 0 {
        (counts.heap_allocating_type_count as f64 / total_types as f64) * 100.0
    } else {
        0.0
    };

    let total_loops = counts.iterator_method_count + counts.for_loop_count;
    let iterator_leverage_score = if total_loops > 0 {
        (counts.iterator_method_count as f64 / total_loops as f64) * 100.0
    } else {
        100.0
    };

    let unsafe_blocks = counts.unsafe_blocks();
    let mut signals = Vec::new();
    if indirection_ratio > 20.0 {
        signals.push(format!("high indirection {:.1}%", indirection_ratio));
    }
    if iterator_leverage_score < 50.0 && total_loops > 0 {
        signals.push(format!(
            "low iterator leverage {:.1}%",
            iterator_leverage_score
        ));
    }
    if unsafe_blocks > 0 {
        signals.push(format!("unsafe blocks {}", unsafe_blocks));
    }

    let safety_penalty = (unsafe_blocks as f64 * 5.0).min(50.0);
    let total_leverage_score =
        ((100.0 - indirection_ratio) * 0.4) + (iterator_leverage_score * 0.6) - safety_penalty;
    let total_leverage_score = total_leverage_score.clamp(0.0, 100.0);

    LeverageScores {
        indirection_ratio,
        iterator_leverage_score,
        unsafe_blocks,
        total_leverage_score,
        signals,
    }
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn module_key_for_path(path: &str) -> String {
    let normalized = normalize_path(path);
    let without_src = normalized.strip_prefix("src/").unwrap_or(&normalized);
    let without_extension = without_src.strip_suffix(".rs").unwrap_or(without_src);
    let module_path = Path::new(without_extension);
    module_path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .filter(|part| *part != "mod" && *part != "lib" && *part != "main")
        .collect::<Vec<_>>()
        .join("::")
}

#[cfg(test)]
mod tests {
    use super::{LeverageCounts, LeverageVisitor, score_leverage};
    use syn::visit::Visit;

    fn counts_for(source: &str) -> LeverageCounts {
        let file = syn::parse_file(source).expect("test source should parse");
        let mut visitor = LeverageVisitor::new();
        visitor.visit_file(&file);
        visitor.counts
    }

    #[test]
    fn counts_type_paths_and_iterator_methods() {
        let counts = counts_for(
            r#"
            fn demo(items: Vec<String>, boxed: Box<String>) -> Option<String> {
                items.iter().filter(|item| !item.is_empty()).map(|item| item.clone()).collect()
            }
            "#,
        );

        assert_eq!(counts.inline_type_count, 5);
        assert_eq!(counts.heap_allocating_type_count, 1);
        assert_eq!(counts.iterator_method_count, 4);
        assert_eq!(counts.for_loop_count, 0);
    }

    #[test]
    fn scores_raw_for_loops_as_low_iterator_leverage() {
        let counts = counts_for(
            r#"
            fn demo(items: &[usize]) -> usize {
                let mut total = 0;
                for index in 0..items.len() {
                    total += items[index];
                }
                total
            }
            "#,
        );
        let scores = score_leverage(&counts);

        assert_eq!(counts.for_loop_count, 1);
        assert_eq!(scores.iterator_leverage_score, 0.0);
        assert!(
            scores
                .signals
                .iter()
                .any(|signal| signal.contains("low iterator leverage"))
        );
    }

    #[test]
    fn counts_unsafe_surface_separately() {
        let counts = counts_for(
            r#"
            unsafe trait Demo {}
            unsafe impl Demo for usize {}
            unsafe fn call() {
                unsafe {}
            }
            "#,
        );
        let scores = score_leverage(&counts);

        assert_eq!(counts.unsafe_trait_or_impl_count, 2);
        assert_eq!(counts.unsafe_fn_count, 1);
        assert_eq!(counts.unsafe_expr_count, 1);
        assert_eq!(scores.unsafe_blocks, 4);
    }
}
