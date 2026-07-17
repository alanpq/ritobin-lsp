use ltk_hash::{BinHash, Hash};
use ltk_ritobin::{
    cst::{
        NodeId, TreeKind, Visitor,
        visitor::{Visit, VisitCtx},
    },
    parse::{Span, Token, TokenKind},
};

use crate::{document::Document, linter::Lint, lol_meta::service::Classes};

#[derive(Debug)]
pub struct Class {
    depth: usize,
    span: Span,
    hash: BinHash,
}

pub struct Linter<'a> {
    document: &'a Document,
    classes: &'a Classes,

    stack: Vec<TreeKind>,
    depth: usize,
    pub class_stack: Vec<Class>,
    pub lints: Vec<Lint>,
}

impl<'a> Linter<'a> {
    pub fn new(document: &'a Document, classes: &'a Classes) -> Self {
        Self {
            document,
            classes,

            depth: 0,
            stack: vec![],
            class_stack: vec![],
            lints: vec![],
        }
    }

    fn cur_class(&self) -> Option<&Class> {
        self.class_stack.last().and_then(|class| {
            if class.depth + 1 == self.depth {
                Some(class)
            } else {
                None
            }
        })
    }
}

impl Visitor for Linter<'_> {
    fn enter_tree(&mut self, ctx: &VisitCtx<'_>, tree: NodeId) -> Visit {
        let tree = ctx.node(tree).unwrap();

        match tree.kind {
            TreeKind::Class => {
                if let Some(c) = tree
                    .children
                    .get(ctx.cst)
                    .first()
                    .and_then(|c| c.token(ctx.cst))
                {
                    // eprintln!("{c:?} d:{}", self.depth);
                    self.class_stack.push(Class {
                        depth: self.depth,
                        span: c.span,
                        hash: match c.kind {
                            TokenKind::Name => BinHash::hash_str(&self.document.text[c.span]),
                            TokenKind::HexLit => {
                                BinHash::from_str_radix(&self.document.text[c.span][2..], 16)
                                    .unwrap()
                            }
                            _ => return Visit::Continue,
                        },
                    });
                    // eprintln!("-> {}: {:?}", self.stack.len(), &self.document.text[c.span]);
                }
            }
            TreeKind::Block => {
                self.depth += 1;
            }
            TreeKind::EntryKey => {
                let key = match tree
                    .children
                    .get(ctx.cst)
                    .first()
                    .and_then(|k| k.token(ctx.cst))
                {
                    Some(Token {
                        kind: TokenKind::Name,
                        span,
                    }) => BinHash::hash_str(&self.document.text[span]),
                    Some(Token {
                        kind: TokenKind::HexLit,
                        span,
                    }) => BinHash::from_str_radix(&self.document.text[span][2..], 16).unwrap(),
                    _ => {
                        return Visit::Continue;
                    }
                };

                // eprintln!(
                //     "ENTRY KEY -> {key:?} s:({}) d:({})",
                //     self.stack.len(),
                //     self.depth
                // );

                if let Some(class_entry) = self.cur_class()
                    && self.classes.get(class_entry.hash).is_some()
                    && self.classes.find_property(class_entry.hash, key).is_none()
                {
                    // eprintln!("{}", &self.document.text[tree.span]);
                    self.lints.push(Lint::UnknownField {
                        span: tree.span,
                        class: class_entry.span,
                    });
                }
            }
            kind => {
                // eprintln!("{kind:?} -> {}", &self.document.text[tree.span]);
            }
        }
        self.stack.push(tree.kind);

        Visit::Continue
    }
    fn exit_tree(&mut self, ctx: &VisitCtx, node: NodeId) -> Visit {
        let tree = ctx.node(node).unwrap();

        match tree.kind {
            TreeKind::Block => {
                self.depth -= 1;
            }
            _ => {}
        }

        if let Some(taken) = self.class_stack.pop_if(|class| self.depth == class.depth) {
            // eprintln!(
            //     "<- s:{}: {:?} ({}) (d:{})",
            //     self.stack.len(),
            //     &self.document.text[taken.span],
            //     tree.kind,
            //     self.depth
            // );
        }
        self.stack.pop();
        Visit::Continue
    }
}
