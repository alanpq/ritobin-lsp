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
        ClassFinder::new(offset, text).walk(self).finish()
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
        self.blocks.last()?.as_ref()
    }

    fn nearest(&self) -> Option<&ClassScope> {
        self.classes().next_back()
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

/// The blocks enclosing a position, and the class each one is the body of.
#[derive(Debug, Default, Clone)]
pub struct ClassContext {
    /// One entry per enclosing block, outermost first - `Some` when that block is a class' body.
    blocks: Vec<Option<ClassScope>>,
}

impl ClassContext {
    /// Every class whose body encloses the position, outermost first.
    pub fn classes(&self) -> impl DoubleEndedIterator<Item = &ClassScope> {
        self.blocks.iter().flatten()
    }
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

/// Feed every `enter_tree`/`exit_tree` through this, then ask it which class you are in.
///
/// Both halves are required - the scopes only stay balanced if every tree you hand to
/// [`Self::enter_tree`] also reaches [`Self::exit_tree`]. Returning [`Visit::Skip`] is fine, the
/// walker exits a skipped tree anyway.
#[derive(Debug, Clone)]
pub struct ClassTracker<'a> {
    /// The source document
    pub text: &'a str,

    pub ctx: ClassContext,
    /// The class the next block belongs to, set between a `Class` and its own block.
    next_class: Option<ClassScope>,
}

impl<'a> ClassTracker<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            ctx: ClassContext::default(),
            next_class: None,
        }
    }

    /// Records a tree we are entering, opening a scope if it is a block.
    ///
    /// A `Class` names the block that follows it, so a block is a class' body exactly when it is
    /// that class' own block. Anything else leaves the scopes alone.
    fn enter(&mut self, ctx: &VisitCtx<'_>, tree: NodeId) {
        match ctx.node(tree) {
            Some(Node {
                kind: TreeKind::Class,
                children,
                ..
            }) => {
                self.next_class = children
                    .get(ctx.cst)
                    .first()
                    .and_then(|c| c.token(ctx.cst))
                    .map(|token| ClassScope::new(*token).resolved(self.text));
            }
            Some(Node {
                kind: TreeKind::Block | TreeKind::ListItemBlock,
                ..
            }) => self.ctx.blocks.push(self.next_class.take()),
            _ => (),
        }
    }

    /// Records a tree we are leaving, closing its scope if it is a block.
    fn exit(&mut self, ctx: &VisitCtx<'_>, tree: NodeId) {
        match ctx.node(tree).map(|node| node.kind) {
            Some(TreeKind::Block | TreeKind::ListItemBlock) => {
                self.ctx.blocks.pop();
            }
            // a class we never found a block for would otherwise name the next block we meet
            Some(TreeKind::Class) => self.next_class = None,
            _ => (),
        }
    }
}

impl Visitor for ClassTracker<'_> {
    fn enter_tree(&mut self, ctx: &VisitCtx<'_>, tree: NodeId) -> Visit {
        self.enter(ctx, tree);
        Visit::Continue
    }

    fn exit_tree(&mut self, ctx: &VisitCtx<'_>, tree: NodeId) -> Visit {
        self.exit(ctx, tree);
        Visit::Continue
    }
}

/// Tracks the class scopes at one offset, descending only the trees that contain it.
struct ClassFinder<'a> {
    tracker: ClassTracker<'a>,
    offset: u32,
}

impl<'a> ClassFinder<'a> {
    fn new(offset: u32, text: &'a str) -> Self {
        Self {
            tracker: ClassTracker::new(text),
            offset,
        }
    }

    /// The scopes at the offset, once the walk is done.
    fn finish(mut self) -> ClassContext {
        if let Some(class) = self.tracker.next_class.take() {
            // we stopped on a class' name rather than in its body - being on the name still puts
            // us at that class, which is what hovering one has to render
            self.tracker.ctx.blocks.push(Some(class));
        }
        self.tracker.ctx
    }
}

impl Visitor for ClassFinder<'_> {
    fn enter_tree(&mut self, ctx: &VisitCtx<'_>, tree: NodeId) -> Visit {
        match ctx.node(tree) {
            Some(node) if node.span.contains(self.offset) => {}
            _ => return Visit::Skip,
        }

        self.tracker.enter(ctx, tree);
        Visit::Continue
    }
}
