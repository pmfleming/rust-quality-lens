//! Syntax observations in backend text, never compiler identities or authored spans.
use crate::facts::MacroContext;
use serde::{Deserialize, Serialize};
use syn::{
    spanned::Spanned,
    visit::{self, Visit},
};

#[derive(Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct GeneratedSyntax {
    pub(super) parsed: bool,
    reason: Option<String>,
    declarations: Vec<Declaration>,
}

#[derive(Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Declaration {
    kind: String,
    name: String,
    start_byte: usize,
    end_byte: usize,
    observed_field_count: Option<usize>,
}

struct Collector {
    declarations: Vec<Declaration>,
    prefix: usize,
}

pub(super) fn parse(text: &str, context: &MacroContext) -> GeneratedSyntax {
    // Conservative lexical bound includes literal/comment contents. It can
    // reject valid text, but cannot hide deep syntax behind strings or comments.
    if !within_budget(text) {
        return GeneratedSyntax {
            parsed: false,
            reason: Some("fragment parser budget exceeded".into()),
            declarations: Vec::new(),
        };
    }
    let mut collector = Collector {
        declarations: Vec::new(),
        prefix: 0,
    };
    let parsed = match context {
        MacroContext::Item => syn::parse_file(text).map(|file| collector.visit_file(&file)),
        MacroContext::Impl => {
            let prefix = "impl __RqlensFragment {\n";
            collector.prefix = prefix.len();
            // Parse exactly one impl, not a file: text cannot escape the wrapper.
            syn::parse_str::<syn::ItemImpl>(&format!("{prefix}{text}\n}}"))
                .map(|item| collector.visit_item_impl(&item))
        }
    }
    .is_ok();
    GeneratedSyntax {
        parsed,
        reason: (!parsed).then(|| "invalid item/impl fragment syntax".into()),
        declarations: collector.declarations,
    }
}

fn within_budget(text: &str) -> bool {
    let mut word = false;
    text.len() <= 65_536
        && text
            .chars()
            .filter(|c| {
                let alphanumeric = c.is_alphanumeric();
                let counted = !c.is_whitespace() && (!alphanumeric || !word);
                word = alphanumeric;
                counted
            })
            .take(129)
            .count()
            <= 128
}

impl Collector {
    fn record(
        &mut self,
        kind: &str,
        name: &syn::Ident,
        span: proc_macro2::Span,
        fields: Option<usize>,
    ) {
        let range = span.byte_range();
        self.declarations.push(Declaration {
            kind: kind.into(),
            name: name.to_string(),
            start_byte: range.start - self.prefix,
            end_byte: range.end - self.prefix,
            observed_field_count: fields,
        });
    }
}

impl<'ast> Visit<'ast> for Collector {
    fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
        self.record("struct", &item.ident, item.span(), Some(item.fields.len()));
        visit::visit_item_struct(self, item);
    }
    fn visit_item_enum(&mut self, item: &'ast syn::ItemEnum) {
        self.record(
            "enum",
            &item.ident,
            item.span(),
            Some(item.variants.iter().map(|v| v.fields.len()).sum()),
        );
        visit::visit_item_enum(self, item);
    }
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        self.record("function", &item.sig.ident, item.span(), None);
        visit::visit_item_fn(self, item);
    }
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        self.record("impl-method", &item.sig.ident, item.span(), None);
        visit::visit_impl_item_fn(self, item);
    }
    fn visit_trait_item_fn(&mut self, item: &'ast syn::TraitItemFn) {
        self.record("trait-method", &item.sig.ident, item.span(), None);
        visit::visit_trait_item_fn(self, item);
    }
}
