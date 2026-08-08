use ltk_hash::BinHash;
use ltk_ritobin::{
    Cst,
    cst::{
        Kind as TreeKind, Node, NodeId, Visitor,
        visitor::{Visit, VisitCtx, VisitorExt as _},
    },
    parse::TokenKind,
};
use ritobin_lsp::scope::TokenExt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorContext {
    PropertyKey {
        class: BinHash,
    },
    PropertyType {
        class: BinHash,
        property: BinHash,
    },
    PropertyValue {
        class: BinHash,
        property: BinHash,
    },
    ContainerItem {
        class: BinHash,
        property: BinHash,
    },
}

pub fn resolve(cst: &Cst, text: &str, offset: u32) -> Option<CursorContext> {
    let path = PathFinder {
        offset,
        stack: Vec::new(),
        path: None,
    }
    .walk(cst)
    .path?;

    classify(cst, text, &path, offset)
}

struct PathFinder {
    offset: u32,
    stack: Vec<NodeId>,
    path: Option<Vec<NodeId>>,
}

impl Visitor for PathFinder {
    fn enter_tree(&mut self, ctx: &VisitCtx, node: NodeId) -> Visit {
        let Some(tree) = ctx.node(node) else {
            return Visit::Skip;
        };
        if !tree.span.contains(self.offset) {
            return Visit::Skip;
        }

        self.stack.push(node);
        if self.path.as_ref().is_none_or(|p| p.len() <= self.stack.len()) {
            self.path = Some(self.stack.clone());
        }
        Visit::Continue
    }

    fn exit_tree(&mut self, _ctx: &VisitCtx, node: NodeId) -> Visit {
        if self.stack.last() == Some(&node) {
            self.stack.pop();
        }
        Visit::Continue
    }
}

#[derive(Clone, Copy)]
enum Scope {
    Class {
        class: BinHash,
    },
    Container {
        class: BinHash,
        property: BinHash,
    },
    Opaque,
}

enum Region {
    Key,
    Type,
    Value,
}

fn classify(cst: &Cst, text: &str, path: &[NodeId], offset: u32) -> Option<CursorContext> {
    let mut scope = None;
    let mut scope_idx = 0;

    for i in 1..path.len() {
        let parent = cst.node(path[i - 1])?;
        let next = match cst.node(path[i])?.kind {
            TreeKind::Block => match parent.kind {
                TreeKind::Class => match class_hash(cst, text, parent) {
                    Some(class) => Scope::Class { class },
                    None => Scope::Opaque,
                },
                TreeKind::EntryValue => owning_entry(cst, text, path, i)
                    .zip(match scope {
                        Some(Scope::Class { class }) => Some(class),
                        _ => None,
                    })
                    .map_or(Scope::Opaque, |(property, class)| Scope::Container {
                        class,
                        property,
                    }),
                _ => Scope::Opaque,
            },
            TreeKind::ListItemBlock => Scope::Opaque,
            _ => continue,
        };

        scope = Some(next);
        scope_idx = i;
    }

    let entry = path[scope_idx..].iter().rev().find_map(|&id| {
        let node = cst.node(id)?;
        (node.kind == TreeKind::Entry).then_some(node)
    });

    match (scope?, entry) {
        (Scope::Class { class }, None) => Some(CursorContext::PropertyKey { class }),
        (Scope::Class { class }, Some(entry)) => match region(cst, entry, offset) {
            Region::Key => Some(CursorContext::PropertyKey { class }),
            Region::Type => Some(CursorContext::PropertyType {
                class,
                property: entry_key(cst, text, entry)?,
            }),
            Region::Value => Some(CursorContext::PropertyValue {
                class,
                property: entry_key(cst, text, entry)?,
            }),
        },
        (Scope::Container { class, property }, None) => {
            Some(CursorContext::ContainerItem { class, property })
        }
        (Scope::Container { class, property }, Some(entry)) => {
            match region(cst, entry, offset) {
                Region::Value => Some(CursorContext::ContainerItem { class, property }),
                _ => None,
            }
        }
        (Scope::Opaque, _) => None,
    }
}

fn owning_entry(cst: &Cst, text: &str, path: &[NodeId], block_idx: usize) -> Option<BinHash> {
    let entry = cst.node(*path.get(block_idx.checked_sub(2)?)?)?;
    (entry.kind == TreeKind::Entry)
        .then(|| entry_key(cst, text, entry))
        .flatten()
}

fn region(cst: &Cst, entry: &Node, offset: u32) -> Region {
    let (mut colon, mut eq) = (None, None);

    for token in entry.children.get(cst).iter().filter_map(|c| c.token(cst)) {
        match token.kind {
            TokenKind::Colon => colon = colon.or(Some(token.span)),
            TokenKind::Eq => eq = eq.or(Some(token.span)),
            _ => {}
        }
    }

    match (colon, eq) {
        (_, Some(eq)) if offset >= eq.end => Region::Value,
        (Some(colon), _) if offset > colon.start => Region::Type,
        _ => Region::Key,
    }
}

fn entry_key(cst: &Cst, text: &str, entry: &Node) -> Option<BinHash> {
    let key = entry.children.get(cst).iter().find_map(|child| {
        let node = child.tree(cst)?;
        (node.kind == TreeKind::EntryKey).then_some(node)
    })?;

    key.children.get(cst).first()?.token(cst)?.as_bin_hash(text)
}

fn class_hash(cst: &Cst, text: &str, class: &Node) -> Option<BinHash> {
    class.children.get(cst).first()?.token(cst)?.as_bin_hash(text)
}

#[cfg(test)]
mod tests;
