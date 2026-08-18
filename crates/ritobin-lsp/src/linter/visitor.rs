use ltk_ritobin::cst::{
    NodeId, TreeKind, Visitor,
    visitor::{Visit, VisitCtx},
};
use ritobin_lsp::scope::{ClassContextExt as _, ClassTracker, TokenExt};

use crate::{document::Document, linter::Lint};
use meta_wiki::service::Classes;

pub struct Linter<'a> {
    document: &'a Document,
    class_meta: &'a Classes,

    class_scopes: ClassTracker<'a>,
    pub lints: Vec<Lint>,
}

impl<'a> Linter<'a> {
    pub fn new(document: &'a Document, class_meta: &'a Classes) -> Self {
        Self {
            document,
            class_meta,
            class_scopes: ClassTracker::new(&document.text),
            lints: vec![],
        }
    }
}

impl Visitor for Linter<'_> {
    fn enter_tree(&mut self, ctx: &VisitCtx<'_>, tree: NodeId) -> Visit {
        let _ = self.class_scopes.enter_tree(ctx, tree);

        let node = match ctx.node(tree) {
            Some(node) if node.kind == TreeKind::EntryKey => node,
            _ => return Visit::Continue,
        };

        let Some(class) = self.class_scopes.current().copied() else {
            return Visit::Continue;
        };
        let Some(class_hash) = class.hash else {
            return Visit::Continue;
        };
        let Some(key) = ctx
            .cst
            .node(tree)
            .and_then(|n| n.children.get(ctx.cst).first())
            .and_then(|c| c.token(ctx.cst))
            .and_then(|t| t.as_bin_hash(&self.document.text))
        else {
            return Visit::Continue;
        };

        if self.class_meta.get(class_hash).is_some()
            && self.class_meta.find_property(class_hash, key).is_none()
        {
            self.lints.push(Lint::UnknownField {
                span: node.span,
                class: class.token.span,
            });
        }

        Visit::Continue
    }

    fn exit_tree(&mut self, ctx: &VisitCtx, node: NodeId) -> Visit {
        let _ = self.class_scopes.exit_tree(ctx, node);
        Visit::Continue
    }
}

#[cfg(test)]
mod tests;
