use ltk_ritobin::cst::{
    NodeId, TreeKind, Visitor,
    visitor::{Visit, VisitCtx},
};
use ritobin_lsp::scope::{ClassScopes, node_hash};

use crate::{document::Document, linter::Lint, lol_meta::service::Classes};

pub struct Linter<'a> {
    document: &'a Document,
    classes: &'a Classes,

    scopes: ClassScopes,
    pub lints: Vec<Lint>,
}

impl<'a> Linter<'a> {
    pub fn new(document: &'a Document, classes: &'a Classes) -> Self {
        Self {
            document,
            classes,
            scopes: ClassScopes::new(),
            lints: vec![],
        }
    }
}

impl Visitor for Linter<'_> {
    fn enter_tree(&mut self, ctx: &VisitCtx<'_>, tree: NodeId) -> Visit {
        self.scopes.enter(ctx, tree, &self.document.text);

        let node = match ctx.node(tree) {
            Some(node) if node.kind == TreeKind::EntryKey => node,
            _ => return Visit::Continue,
        };

        // Only entries sitting directly in a class body are properties; inside a container or map
        // block the key is an element key, which no class declares.
        let Some(class) = self.scopes.innermost().copied() else {
            return Visit::Continue;
        };
        let Some((key, _)) = node_hash(ctx.cst, &self.document.text, tree) else {
            return Visit::Continue;
        };

        if self.classes.get(class.hash).is_some()
            && self.classes.find_property(class.hash, key).is_none()
        {
            self.lints.push(Lint::UnknownField {
                span: node.span,
                class: class.span,
            });
        }

        Visit::Continue
    }

    fn exit_tree(&mut self, ctx: &VisitCtx, node: NodeId) -> Visit {
        self.scopes.exit(ctx, node);
        Visit::Continue
    }
}

#[cfg(test)]
mod tests;
