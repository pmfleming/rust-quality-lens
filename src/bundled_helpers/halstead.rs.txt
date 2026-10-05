//! Versioned syntax-token Halstead measurements. See docs/halstead.md.

use proc_macro2::{Delimiter, Spacing, TokenStream, TokenTree};
use quote::ToTokens;
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Clone, Debug, Serialize)]
pub struct HalsteadMetrics {
    pub distinct_operators: usize,
    pub total_operators: usize,
    pub distinct_operands: usize,
    pub total_operands: usize,
    pub vocabulary: usize,
    pub length: usize,
    pub volume: f64,
    pub difficulty: Option<f64>,
    pub effort: Option<f64>,
}

#[derive(Default)]
struct Counts {
    operators: BTreeSet<String>,
    operands: BTreeSet<String>,
    total_operators: usize,
    total_operands: usize,
}

/// Measures one scope directly, retaining distinct identities until finalization.
/// Input is syn's syntax-token stream, not formatted text or a LOC proxy.
pub fn measure(syntax: &impl ToTokens) -> HalsteadMetrics {
    let mut counts = Counts::default();
    counts.scan(syntax.to_token_stream());
    counts.finish()
}

impl Counts {
    fn operator(&mut self, value: impl Into<String>) {
        self.operators.insert(value.into());
        self.total_operators += 1;
    }

    fn operand(&mut self, category: &str, value: impl std::fmt::Display) {
        self.operands.insert(format!("{category}:{value}"));
        self.total_operands += 1;
    }

    fn scan(&mut self, stream: TokenStream) {
        let tokens = stream.into_iter().collect::<Vec<_>>();
        let mut index = 0;
        while index < tokens.len() {
            if let Some(end) = attribute_end(&tokens, index) {
                index = end;
                continue;
            }
            match &tokens[index] {
                TokenTree::Group(group) => {
                    match group.delimiter() {
                        Delimiter::Parenthesis => self.operator("()"),
                        Delimiter::Brace => self.operator("{}"),
                        Delimiter::Bracket => self.operator("[]"),
                        Delimiter::None => {}
                    }
                    self.scan(group.stream());
                }
                TokenTree::Ident(ident) => {
                    let spelling = ident.to_string();
                    if is_keyword_operator(&spelling) {
                        self.operator(spelling);
                    } else {
                        self.operand("ident", spelling);
                    }
                }
                TokenTree::Literal(literal) => self.operand("literal", literal),
                TokenTree::Punct(punct) => {
                    if punct.as_char() == '\''
                        && punct.spacing() == Spacing::Joint
                        && let Some(TokenTree::Ident(lifetime)) = tokens.get(index + 1)
                    {
                        self.operand("lifetime", lifetime);
                        index += 1;
                    } else {
                        let (operator, consumed) = punctuation(&tokens[index..]);
                        self.operator(operator);
                        index += consumed - 1;
                    }
                }
            }
            index += 1;
        }
    }

    fn finish(self) -> HalsteadMetrics {
        let distinct_operators = self.operators.len();
        let distinct_operands = self.operands.len();
        let vocabulary = distinct_operators + distinct_operands;
        let length = self.total_operators + self.total_operands;
        let volume = if vocabulary == 0 {
            0.0
        } else {
            length as f64 * (vocabulary as f64).log2()
        };
        // D is undefined for a nonempty scope with no operands. The empty
        // scope has zero effort by explicit convention, not by dividing 0/0.
        let difficulty = if length == 0 {
            Some(0.0)
        } else if distinct_operands == 0 {
            None
        } else {
            Some(
                distinct_operators as f64 / 2.0 * self.total_operands as f64
                    / distinct_operands as f64,
            )
        };
        HalsteadMetrics {
            distinct_operators,
            total_operators: self.total_operators,
            distinct_operands,
            total_operands: self.total_operands,
            vocabulary,
            length,
            volume,
            difficulty,
            effort: difficulty.map(|difficulty| difficulty * volume),
        }
    }
}

fn attribute_end(tokens: &[TokenTree], start: usize) -> Option<usize> {
    if !matches!(&tokens[start], TokenTree::Punct(punct) if punct.as_char() == '#') {
        return None;
    }
    let mut next = start + 1;
    if matches!(tokens.get(next), Some(TokenTree::Punct(punct)) if punct.as_char() == '!') {
        next += 1;
    }
    matches!(tokens.get(next), Some(TokenTree::Group(group)) if group.delimiter() == Delimiter::Bracket)
        .then_some(next + 1)
}

fn punctuation(tokens: &[TokenTree]) -> (String, usize) {
    let mut spelling = String::new();
    let mut selected = (String::new(), 0);
    for (index, token) in tokens.iter().take(3).enumerate() {
        let TokenTree::Punct(punct) = token else {
            break;
        };
        spelling.push(punct.as_char());
        if index == 0 || is_compound_operator(&spelling) {
            selected = (spelling.clone(), index + 1);
        }
        if punct.spacing() != Spacing::Joint {
            break;
        }
    }
    selected
}

fn is_compound_operator(value: &str) -> bool {
    matches!(
        value,
        "+=" | "-="
            | "*="
            | "/="
            | "%="
            | "^="
            | "&="
            | "|="
            | "<<="
            | ">>="
            | "=="
            | "!="
            | "<="
            | ">="
            | "&&"
            | "||"
            | "<<"
            | ">>"
            | "::"
            | "->"
            | "=>"
            | ".."
            | "..="
            | "..."
    )
}

fn is_keyword_operator(value: &str) -> bool {
    // self/Self, booleans, primitive type names and raw identifiers are operands.
    // Include reserved keywords for opaque macro input, without expanding it.
    matches!(
        value,
        "as" | "async"
            | "await"
            | "break"
            | "const"
            | "continue"
            | "crate"
            | "dyn"
            | "else"
            | "enum"
            | "extern"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
            | "abstract"
            | "become"
            | "box"
            | "do"
            | "final"
            | "gen"
            | "macro"
            | "override"
            | "priv"
            | "try"
            | "typeof"
            | "unsized"
            | "virtual"
            | "yield"
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{HalsteadMetrics, measure};

    fn file(source: &str) -> HalsteadMetrics {
        measure(&syn::parse_file(source).unwrap())
    }

    fn assert_counts(metrics: &HalsteadMetrics, counts: [usize; 4]) {
        assert_eq!(
            [
                metrics.distinct_operators,
                metrics.total_operators,
                metrics.distinct_operands,
                metrics.total_operands,
            ],
            counts
        );
    }

    #[test]
    fn matches_published_mozilla_rust_example() {
        // Independently tabulated in rust-code-analysis's Rust Halstead test.
        let metrics = file(
            r#"fn main() {
            let a = 5; let b = 5; let c = 5;
            let avg = (a + b + c) / 3;
            println!("{}", avg);
        }"#,
        );
        assert_counts(&metrics, [10, 23, 9, 15]);
        assert_eq!(metrics.vocabulary, 19);
        assert_eq!(metrics.length, 38);
        assert!((metrics.volume - 161.42124551085624).abs() < 1e-10);
        assert!((metrics.difficulty.unwrap() - 8.333333333333334).abs() < 1e-10);
        assert!((metrics.effort.unwrap() - 1345.177045923802).abs() < 1e-10);
    }

    #[test]
    fn hand_counted_repeated_operands_and_operators() {
        // Operators: fn, (), :, ->, {}, +, +; operands: f, x, i32, i32, x, x, 1.
        let metrics = file("fn f(x: i32) -> i32 { x + x + 1 }");
        assert_counts(&metrics, [6, 7, 4, 7]);
        assert_eq!(metrics.difficulty, Some(5.25));
        assert_eq!(metrics.effort, Some(5.25 * 14.0 * 10_f64.log2()));
    }

    #[test]
    fn ignores_layout_comments_and_attributes_not_literal_contents() {
        let plain = file("fn f() { let x = 1; x + x; }");
        let decorated = file(
            "#![allow(unused)]\n/// doc operators + -\n#[inline]\nfn f ( ) { /* x */ let x=1; // hello\nx + x; }",
        );
        assert_eq!(
            serde_json::to_value(plain).unwrap(),
            serde_json::to_value(decorated).unwrap()
        );
        let literals =
            file(r##"fn f() { ("+ // fn", r#"+ // fn"#, b"+ // fn", 'x', b'x', 1u8, 1); }"##);
        assert_counts(&literals, [5, 11, 8, 8]);
    }

    #[test]
    fn handles_compound_operators_generics_lifetimes_and_raw_identifiers() {
        let syntax: syn::Expr = syn::parse_str("a >>= 1").unwrap();
        assert_counts(&measure(&syntax), [1, 1, 2, 2]);
        let syntax: syn::Expr = syn::parse_str("a && b || a != b").unwrap();
        assert_counts(&measure(&syntax), [3, 3, 2, 4]);
        let ty: syn::Type = syn::parse_str("Vec<Vec<&'a r#type>>").unwrap();
        // Generic closing angles are separate syntax tokens, not a shift.
        assert_counts(&measure(&ty), [3, 5, 3, 4]);
        let reference: syn::Expr = syn::parse_str("&&x").unwrap();
        assert_counts(&measure(&reference), [1, 2, 1, 1]);
    }

    #[test]
    fn rust_expression_taxonomy_has_hand_counted_cases() {
        for (source, expected) in [
            ("f(x, x)", [2, 2, 2, 3]),
            ("xs[i]?", [2, 2, 2, 2]),
            ("x.method(y).field", [2, 3, 4, 4]),
            ("if ok { a } else { b }", [3, 4, 3, 3]),
            ("|x| x + 1", [2, 3, 2, 3]),
            ("move || true", [2, 3, 1, 1]),
            ("a..=b", [1, 1, 2, 2]),
            ("&raw const x", [2, 2, 2, 2]),
            ("'label: loop { break 'label; }", [5, 5, 1, 2]),
            ("vec![x, x + 1]", [4, 4, 3, 4]),
            ("δ + δ", [1, 1, 1, 2]),
            ("true && false", [1, 1, 2, 2]),
            ("pair.0", [1, 1, 2, 2]),
            ("-1 + 1u8 + 0x1", [2, 3, 3, 3]),
        ] {
            let syntax: syn::Expr = syn::parse_str(source).unwrap();
            assert_counts(&measure(&syntax), expected);
        }
        assert_counts(&file("const X: i32 = 1;"), [4, 4, 3, 3]);
    }

    #[test]
    fn zero_and_undefined_cases_are_explicit_and_json_safe() {
        let empty = file("// no program");
        assert_counts(&empty, [0, 0, 0, 0]);
        assert_eq!(empty.effort, Some(0.0));
        let operators = measure(&syn::parse_str::<syn::Expr>("{}").unwrap());
        assert_counts(&operators, [1, 1, 0, 0]);
        assert_eq!(operators.volume, 0.0);
        assert_eq!(operators.difficulty, None);
        assert_eq!(operators.effort, None);
        assert_eq!(
            serde_json::to_value(operators).unwrap()["effort"],
            serde_json::Value::Null
        );
        let operand = measure(&syn::parse_str::<syn::Expr>("42").unwrap());
        assert_eq!(operand.effort, Some(0.0));
    }

    #[test]
    fn whole_file_counts_union_vocabulary_and_do_not_sum_effort() {
        let one = file("fn a() { 1 }");
        let two = file("fn b() { 1 }");
        let combined = file("fn a() { 1 } fn b() { 1 }");
        assert_counts(&combined, [3, 6, 3, 4]);
        assert_eq!(
            combined.total_operators,
            one.total_operators + two.total_operators
        );
        assert_ne!(
            combined.distinct_operators,
            one.distinct_operators + two.distinct_operators
        );
        assert_ne!(
            combined.effort.unwrap(),
            one.effort.unwrap() + two.effort.unwrap()
        );
    }
}
