//! Shared tracking of the `Class` bodies enclosing a position in the tree.
//!
//! Hover, the linter and completion all need the same answer — "which class body am I inside?" —
//! and each used to re-derive it with its own class stack and its own token hashing. This is that
//! logic, once.

use ltk_hash::{BinHash, Hash as _};
use ltk_ritobin::{
    Cst,
    cst::{
        Kind as TreeKind, NodeId, Visitor,
        visitor::{Visit, VisitCtx, VisitorExt as _},
    },
    parse::{Span, Token, TokenKind},
};

/// A `Class` whose body encloses the current position.
#[derive(Debug, Clone, Copy)]
pub struct ClassScope {
    pub hash: BinHash,
    /// Span of the class name token, for diagnostics that point at it.
    pub span: Span,
    /// Block nesting depth at which this class was declared; its body sits at `depth + 1`.
    depth: usize,
}

/// Feed every `enter_tree`/`exit_tree` through this, then ask it which class you are in.
#[derive(Debug, Default, Clone)]
pub struct ClassScopes {
    depth: usize,
    stack: Vec<ClassScope>,
}

impl ClassScopes {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enter(&mut self, ctx: &VisitCtx, node: NodeId, text: &str) {
        match ctx.node(node).map(|tree| tree.kind) {
            Some(TreeKind::Class) => {
                if let Some((hash, span)) = node_hash(ctx.cst, text, node) {
                    self.stack.push(ClassScope {
                        hash,
                        span,
                        depth: self.depth,
                    });
                }
            }
            Some(TreeKind::Block | TreeKind::ListItemBlock) => self.depth += 1,
            _ => {}
        }
    }

    pub fn exit(&mut self, ctx: &VisitCtx, node: NodeId) {
        if let Some(TreeKind::Block | TreeKind::ListItemBlock) = ctx.node(node).map(|tree| tree.kind)
        {
            self.depth = self.depth.saturating_sub(1);
        }
        self.stack.pop_if(|class| class.depth == self.depth);
    }

    /// The class whose body *directly* contains this position. `None` inside a nested
    /// container/map block, where entry keys are map keys rather than properties.
    pub fn innermost(&self) -> Option<&ClassScope> {
        self.stack.last().filter(|class| class.depth + 1 == self.depth)
    }

    /// The nearest enclosing class, even when a container block sits in between.
    pub fn enclosing(&self) -> Option<&ClassScope> {
        self.stack.last()
    }
}

/// Resolve a `Name`/`HexLit` identifier token to the hash it denotes. `Name` is hashed;
/// `HexLit` already *is* the hash and must be parsed, not hashed.
pub fn hash_token(text: &str, token: &Token) -> Option<BinHash> {
    match token.kind {
        TokenKind::Name => Some(BinHash::hash_str(&text[token.span])),
        TokenKind::HexLit => {
            BinHash::from_str_radix(text[token.span].trim_start_matches("0x"), 16).ok()
        }
        _ => None,
    }
}

/// The first `Name`/`HexLit` child of a node, as a hash — the class name of a `Class`, or the key
/// of an `EntryKey`.
pub fn node_hash(cst: &Cst, text: &str, node: NodeId) -> Option<(BinHash, Span)> {
    let token = cst.node(node)?.children.get(cst).first()?.token(cst)?;
    hash_token(text, token).map(|hash| (hash, token.span))
}

/// Walk only as far as `offset`, returning the class scopes enclosing it.
///
/// Subtrees that cannot contain `offset` are pruned with [`Visit::Skip`], so this descends the one
/// root-to-cursor path rather than the whole tree.
pub fn scopes_at(cst: &Cst, text: &str, offset: u32) -> ClassScopes {
    Finder {
        offset,
        text,
        entered: Vec::new(),
        scopes: ClassScopes::new(),
        deepest: None,
    }
    .walk(cst)
    .deepest
    .map_or_else(ClassScopes::new, |(_, scopes)| scopes)
}

struct Finder<'a> {
    offset: u32,
    text: &'a str,
    entered: Vec<NodeId>,
    scopes: ClassScopes,
    /// The live stack is unwound by the time the walk ends, so snapshot it at the deepest node
    /// that still contains the cursor.
    deepest: Option<(usize, ClassScopes)>,
}

impl Visitor for Finder<'_> {
    fn enter_tree(&mut self, ctx: &VisitCtx, node: NodeId) -> Visit {
        match ctx.node(node) {
            Some(tree) if tree.span.contains(self.offset) => {}
            _ => return Visit::Skip,
        }

        self.entered.push(node);
        self.scopes.enter(ctx, node, self.text);

        if self
            .deepest
            .as_ref()
            .is_none_or(|(depth, _)| *depth <= self.entered.len())
        {
            self.deepest = Some((self.entered.len(), self.scopes.clone()));
        }
        Visit::Continue
    }

    fn exit_tree(&mut self, ctx: &VisitCtx, node: NodeId) -> Visit {
        if self.entered.last() == Some(&node) {
            self.entered.pop();
            self.scopes.exit(ctx, node);
        }
        Visit::Continue
    }
}

#[cfg(test)]
mod tests;
