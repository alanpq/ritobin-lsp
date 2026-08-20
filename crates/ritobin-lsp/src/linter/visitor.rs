use itertools::Itertools;
use ltk_meta::PropertyKind;
use ltk_ritobin::{
    RitoType,
    cst::{
        NodeId, TreeKind, Visitor,
        visitor::{Visit, VisitCtx},
    },
    typecheck,
};
use ritobin_lsp::scope::{ClassContextExt as _, ClassTracker, TokenExt};

use crate::{document::Document, linter::Lint};
use meta_wiki::{
    schema::{EqExt, U32Hash},
    service::Classes,
};

pub struct Linter<'a> {
    document: &'a Document,
    class_meta: &'a Classes,

    class_scopes: ClassTracker<'a>,
    // stack: Vec<NodeId>,
    pub lints: Vec<Lint>,
}

impl<'a> Linter<'a> {
    pub fn new(document: &'a Document, class_meta: &'a Classes) -> Self {
        Self {
            document,
            class_meta,
            class_scopes: ClassTracker::new(&document.text),
            lints: vec![],
            // stack: vec![],
        }
    }
}

impl Visitor for Linter<'_> {
    fn enter_tree(&mut self, ctx: &VisitCtx<'_>, tree: NodeId) -> Visit {
        let _ = self.class_scopes.enter_tree(ctx, tree);
        // self.stack.push(tree);

        let node = match ctx.node(tree) {
            Some(node) if matches!(node.kind, TreeKind::Entry) => node,
            _ => return Visit::Continue,
        };

        let Some(class_scope) = self.class_scopes.current().copied() else {
            return Visit::Continue;
        };
        let Some(class_hash) = class_scope.hash else {
            return Visit::Continue;
        };

        let children = node.children.get(ctx.cst);

        let Some(key) = children.first().and_then(|c| c.tree(ctx.cst)) else {
            return Visit::Continue;
        };

        let Some(key_hash) = key
            .children
            .get(ctx.cst)
            .first()
            .and_then(|c| c.token(ctx.cst))
            .and_then(|t| t.as_bin_hash(&self.document.text))
        else {
            return Visit::Continue;
        };

        let Some(type_expr) = children.get(2).and_then(|c| c.tree(ctx.cst)) else {
            return Visit::Continue;
        };

        let mut tctx = typecheck::state::Ctx {
            text: &self.document.text,
            diagnostics: vec![],
        };
        let Some(ritotype) = typecheck::resolve::resolve_rito_type(&mut tctx, ctx, type_expr).ok()
        else {
            return Visit::Continue;
        };

        if let Some(class) = self.class_meta.get(class_hash) {
            match self.class_meta.find_property(class_hash, key_hash) {
                Some(prop) => {
                    let expected = prop.rito_type();
                    if expected != ritotype {
                        self.lints.push(Lint::MismatchedMetaTypeArg {
                            entry: tree,
                            class: class_scope.token.span,
                            key: key.span,
                            type_expr: type_expr.span,
                            expected,
                            got: ritotype,
                        });
                    }
                    if let Some(entry_value) = children.get(4).and_then(|c| c.tree(ctx.cst))
                        && let Some(default) = class
                            .defaults
                            .as_ref()
                            .and_then(|d| d.get(&U32Hash(*key_hash)))
                        && let Ok(Some(value)) = typecheck::resolve::resolve_value(
                            &mut tctx,
                            ctx,
                            entry_value,
                            Some(expected),
                            None,
                        )
                        && EqExt::eq(default, &value)
                    {
                        self.lints.push(Lint::DefaultValue {
                            entry: tree,
                            span: node.span,
                        });
                    }
                }
                None => {
                    self.lints.push(Lint::UnknownField {
                        entry: tree,
                        span: key.span,
                        class: class_scope.token.span,
                    });
                }
            }
        }

        Visit::Continue
    }

    fn exit_tree(&mut self, ctx: &VisitCtx, node: NodeId) -> Visit {
        let _ = self.class_scopes.exit_tree(ctx, node);
        // self.stack.pop();
        Visit::Continue
    }
}

#[cfg(test)]
mod tests;
