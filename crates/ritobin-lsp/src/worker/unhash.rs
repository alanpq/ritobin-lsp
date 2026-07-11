use lsp_types::{Range, TextEdit};
use ltk_hash::BinHash;
use ltk_mimir_cache::Table;
use ltk_ritobin::{
    cst::{
        NodeId, TokenId, Visitor,
        visitor::{Visit, VisitCtx, VisitorExt},
    },
    parse::{Span, TokenKind},
};

use crate::{server::Hashes, worker::Worker};

impl Worker {
    pub fn unhash(&self, _range: Option<Range>) -> anyhow::Result<Option<Vec<TextEdit>>> {
        let Some(data) = self.data.as_ref() else {
            return Ok(None);
        };

        let Some(hashes) = self.server.hashes.as_ref() else {
            // TODO: propagate this err to client
            return Ok(None);
        };

        let unhasher = Unhasher::new(hashes, &self.document.text).walk(&data.cst);

        Ok(Some(
            unhasher
                .edits
                .into_iter()
                .map(|e| TextEdit {
                    range: self.document.line_numbers.from_span(e.0),
                    new_text: e.1,
                })
                .collect(),
        ))
    }
}

struct Unhasher<'a> {
    hashes: &'a Hashes,
    txt: &'a str,
    edits: Vec<(Span, String)>,
}

impl<'a> Unhasher<'a> {
    pub fn new(hashes: &'a Hashes, txt: &'a str) -> Self {
        Self {
            hashes,
            txt,
            edits: vec![],
        }
    }
}

impl<'a> Visitor for Unhasher<'a> {
    fn visit_token(&mut self, ctx: &VisitCtx, token: TokenId, parent: NodeId) -> Visit {
        let token = ctx.cst.token(token).unwrap();
        let parent = ctx.cst.node(parent).unwrap();

        if token.kind != TokenKind::HexLit {
            return Visit::Continue;
        }

        eprintln!("[unhash] {:?}", parent.kind);
        let Some(txt) = &self.txt[token.span].strip_prefix("0x") else {
            return Visit::Continue;
        };

        let bin_fields = self.hashes.table(Table::BinFields);
        let bin_types = self.hashes.table(Table::BinTypes);

        let unhashed = match parent.kind {
            ltk_ritobin::cst::Kind::EntryKey => {
                let Some(k) = BinHash::from_str_radix(txt, 16).ok() else {
                    return Visit::Continue;
                };
                bin_fields.as_ref().and_then(|h| h.get((*k).into()))
            }
            ltk_ritobin::cst::Kind::Class => {
                let Some(k) = BinHash::from_str_radix(txt, 16).ok() else {
                    return Visit::Continue;
                };
                bin_types.as_ref().and_then(|h| h.get((*k).into()))
            }
            _ => return Visit::Continue,
        };

        if let Some(unhashed) = unhashed.as_ref() {
            self.edits.push((token.span, unhashed.to_string()));
        }
        eprintln!("[unhash] -> {unhashed:?}");

        Visit::Continue
    }
}
