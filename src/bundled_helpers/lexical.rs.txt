//! Snapshot-local type scopes. This is deliberately not a Rust compiler resolver.
use proc_macro2::Span;
use serde::Serialize;
use std::collections::BTreeMap;
use syn::{GenericParam, Generics, Item, UseTree};

#[derive(Serialize)]
pub struct ScopeRecord {
    id: String,
    parent_id: Option<String>,
    kind: String,
    module_key: String,
}

struct Frame {
    id: String,
    module: bool,
    bindings: BTreeMap<String, Option<String>>,
    glob: bool,
}

#[derive(Default, Serialize)]
pub struct Scopes {
    lexical_scopes: Vec<ScopeRecord>,
    #[serde(skip)]
    frames: Vec<Frame>,
}

impl Scopes {
    pub fn enter<'a>(
        &mut self,
        kind: &str,
        span: Span,
        module: &str,
        items: impl Iterator<Item = &'a Item>,
    ) {
        let start = span.start();
        let id = format!("{module}::@{kind}_{}_{}", start.line, start.column);
        let parent_id = self.frames.last().map(|frame| frame.id.clone());
        let mut frame = Frame {
            id: id.clone(),
            module: kind == "module",
            bindings: BTreeMap::new(),
            glob: false,
        };
        for item in items {
            let nominal = match item {
                Item::Struct(item) => Some(&item.ident),
                Item::Enum(item) => Some(&item.ident),
                _ => None,
            };
            if let Some(name) = nominal {
                let owner = if frame.module {
                    format!("{module}::{name}")
                } else {
                    format!("{id}::{name}")
                };
                bind(&mut frame.bindings, name.to_string(), Some(owner));
            } else {
                match item {
                    Item::Type(item) => bind(&mut frame.bindings, item.ident.to_string(), None),
                    Item::Trait(item) => bind(&mut frame.bindings, item.ident.to_string(), None),
                    Item::Union(item) => bind(&mut frame.bindings, item.ident.to_string(), None),
                    Item::Mod(item) => bind(&mut frame.bindings, item.ident.to_string(), None),
                    Item::Use(item) => imports(&item.tree, &mut frame),
                    _ => {}
                }
            }
        }
        self.lexical_scopes.push(ScopeRecord {
            id,
            parent_id,
            kind: kind.into(),
            module_key: module.into(),
        });
        self.frames.push(frame);
    }

    pub fn generics(&mut self, generics: &Generics) {
        if let Some(frame) = self.frames.last_mut() {
            for parameter in &generics.params {
                if let GenericParam::Type(ty) = parameter {
                    frame.bindings.insert(ty.ident.to_string(), None);
                }
            }
        }
    }

    pub fn leave(&mut self) {
        self.frames.pop();
    }

    pub fn owner(&self, name: &str) -> Option<&str> {
        for frame in self.frames.iter().rev() {
            if let Some(owner) = frame.bindings.get(name) {
                return owner.as_deref();
            }
            if frame.module || frame.glob {
                break;
            }
        }
        None
    }

    pub fn blocked(&self, name: &str) -> bool {
        for frame in self.frames.iter().rev() {
            if let Some(owner) = frame.bindings.get(name) {
                return owner.is_none();
            }
            if frame.glob {
                return true;
            }
            if frame.module {
                break;
            }
        }
        false
    }

    pub fn declaration_name(&self, module: &str, name: &str) -> String {
        match self.frames.last() {
            Some(frame) if !frame.module => format!("{}::{name}", frame.id),
            _ => format!("{module}::{name}"),
        }
    }
}

fn bind(bindings: &mut BTreeMap<String, Option<String>>, name: String, owner: Option<String>) {
    use std::collections::btree_map::Entry;
    match bindings.entry(name) {
        Entry::Vacant(entry) => {
            entry.insert(owner);
        }
        Entry::Occupied(mut entry) => {
            entry.insert(None);
        }
    }
}

fn imports(tree: &UseTree, frame: &mut Frame) {
    match tree {
        UseTree::Path(path) => imports(&path.tree, frame),
        UseTree::Name(name) => bind(&mut frame.bindings, name.ident.to_string(), None),
        UseTree::Rename(rename) => bind(&mut frame.bindings, rename.rename.to_string(), None),
        UseTree::Glob(_) => frame.glob = true,
        UseTree::Group(group) => group.items.iter().for_each(|tree| imports(tree, frame)),
    }
}
