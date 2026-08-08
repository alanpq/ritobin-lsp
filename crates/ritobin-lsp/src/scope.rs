//! Shared tracking of the `Class` bodies enclosing a position in the tree.

use ltk_hash::{BinHash, Hash as _};
use ltk_ritobin::{
    Cst, Node,
    cst::{
        Kind as TreeKind, NodeId, Visitor,
        visitor::{Visit, VisitCtx, VisitorExt as _},
    },
    parse::{Token, TokenKind},
};

#[cfg(test)]
mod tests;

pub trait CstExt {
    /// Get all class related information about the target offset
    fn class_context_at(&self, offset: u32, text: &str) -> ClassContext;
}

impl CstExt for Cst {
    fn class_context_at(&self, offset: u32, text: &str) -> ClassContext {
        Finder::new(offset, text).walk(self).ctx
    }
}

pub trait ClassContextExt {
    /// Get the class we are *directly* inside of.
    fn current(&self) -> Option<&ClassScope>;

    /// Get the nearest class we are inside of. Goes through other blocks like containers/maps.
    fn nearest(&self) -> Option<&ClassScope>;
}

impl ClassContextExt for ClassContext {
    fn current(&self) -> Option<&ClassScope> {
        match self.state {
            ClassContextState::NotInScope => None,
            ClassContextState::InScope => self.nearest(),
        }
    }

    fn nearest(&self) -> Option<&ClassScope> {
        self.scopes.last()
    }
}
impl ClassContextExt for ClassTracker<'_> {
    fn current(&self) -> Option<&ClassScope> {
        self.ctx.current()
    }

    fn nearest(&self) -> Option<&ClassScope> {
        self.ctx.nearest()
    }
}

#[derive(Debug, Default, Clone)]
pub struct ClassContext {
    pub scopes: Vec<ClassScope>,
    pub state: ClassContextState,
}

/// A `Class` whose body encloses the current position.
#[derive(Debug, Clone, Copy)]
pub struct ClassScope {
    /// The class (name/hash)'s token
    pub token: Token,
    /// The class' hash, if known
    pub hash: Option<BinHash>,
}

impl ClassScope {
    pub fn new(token: Token) -> Self {
        Self { token, hash: None }
    }

    pub fn resolved(mut self, text: &str) -> Self {
        self.update_hash(text);
        self
    }

    /// Update our class hash from the given source text
    pub fn update_hash(&mut self, text: &str) {
        self.hash = self.token.as_bin_hash(text);
    }
}

pub trait TokenExt {
    fn as_bin_hash(&self, text: &str) -> Option<BinHash>;
}
impl TokenExt for Token {
    fn as_bin_hash(&self, text: &str) -> Option<BinHash> {
        match self.kind {
            TokenKind::Name => Some(BinHash::hash_str(&text[self.span])),
            TokenKind::HexLit => {
                // parser should guarantee HexLit's are valid so we shouldn't need to propagate this error
                BinHash::from_str_radix(text[self.span].trim_start_matches("0x"), 16).ok()
            }
            _ => None,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, Hash, PartialEq, Eq)]
pub enum ClassContextState {
    /// We are not directly inside a class' scope
    #[default]
    NotInScope,
    /// We are currently directly inside a class' scope
    InScope,
}

/// Feed every `enter_tree`/`exit_tree` through this, then ask it which class you are in.
#[derive(Debug, Clone)]
pub struct ClassTracker<'a> {
    /// The source document
    pub text: &'a str,

    pub ctx: ClassContext,
    pub just_saw_class: bool,
}

impl Visitor for ClassTracker<'_> {
    fn enter_tree(&mut self, ctx: &VisitCtx<'_>, tree: NodeId) -> Visit {
        match ctx.node(tree) {
            Some(Node {
                kind: TreeKind::Class,
                children,
                ..
            }) => {
                if let Some(token) = children.get(ctx.cst).first().and_then(|t| t.token(ctx.cst)) {
                    self.ctx
                        .scopes
                        .push(ClassScope::new(*token).resolved(self.text));
                }
                self.ctx.state = ClassContextState::InScope;
                self.just_saw_class = true;
            }
            Some(Node {
                kind: TreeKind::Block | TreeKind::ListItemBlock,
                ..
            }) => {
                if !self.just_saw_class {
                    self.ctx.state = ClassContextState::NotInScope;
                }
                self.just_saw_class = false;
            }
            _ => (),
        }
        Visit::Continue
    }

    fn exit_tree(&mut self, ctx: &VisitCtx<'_>, tree: NodeId) -> Visit {
        if matches!(ctx.node(tree).map(|n| n.kind), Some(TreeKind::Class)) {
            self.ctx.scopes.pop();
        }
        Visit::Continue
    }
}

impl<'a> ClassTracker<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            ctx: ClassContext::default(),
            just_saw_class: false,
        }
    }
}

pub struct Finder<'a> {
    text: &'a str,
    offset: u32,
    entered: Vec<NodeId>,
    ctx: ClassContext,
    just_found_class: bool,
}

impl<'a> Finder<'a> {
    pub fn new(offset: u32, text: &'a str) -> Self {
        Self {
            text,
            offset,
            entered: Vec::new(),
            ctx: ClassContext::default(),
            just_found_class: false,
        }
    }
}

impl<'a> Visitor for Finder<'a> {
    fn enter_tree(&mut self, ctx: &VisitCtx, node: NodeId) -> Visit {
        match ctx.node(node) {
            Some(tree) if tree.span.contains(self.offset) => {}
            _ => return Visit::Skip,
        }

        match ctx.node(node) {
            Some(Node {
                kind: TreeKind::Class,
                children,
                ..
            }) => {
                if let Some(token) = children.get(ctx.cst).first().and_then(|c| c.token(ctx.cst)) {
                    self.entered.push(node);
                    self.ctx
                        .scopes
                        .push(ClassScope::new(*token).resolved(self.text));
                }
                self.ctx.state = ClassContextState::InScope;
                self.just_found_class = true;
            }
            Some(Node {
                kind: TreeKind::Block,
                ..
            }) => {
                if !self.just_found_class {
                    self.ctx.state = ClassContextState::NotInScope;
                }
                self.just_found_class = false;
            }
            Some(Node {
                kind: TreeKind::ListItemBlock,
                ..
            }) => {
                self.ctx.state = ClassContextState::NotInScope;
            }
            _ => {}
        }
        Visit::Continue
    }
}
