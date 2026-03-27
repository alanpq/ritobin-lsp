use lsp_types::{Range, TextEdit};
use ltk_ritobin::{
    cst::{
        Visitor,
        visitor::{Visit, VisitorExt},
    },
    parse::{Span, TokenKind},
};
use poro_hash::{BinHash, FromStrRadix};

use crate::{server::Hashes, worker::Worker};

impl Worker {
    pub fn unhash(&self, range: Option<Range>) -> anyhow::Result<Option<Vec<TextEdit>>> {
        let Some((cst, _)) = self.bin.as_ref() else {
            return Ok(None);
        };

        let unhasher = Unhasher::new(&self.server.hashes, &self.document.text).walk(cst);

        Ok(Some(
            unhasher
                .edits
                .into_iter()
                .map(|e| TextEdit {
                    range: self.document.line_numbers.from_span(e.0),
                    new_text: e.1.to_string(),
                })
                .collect(),
        ))
    }
}

struct Unhasher<'a> {
    hashes: &'a Hashes,
    txt: &'a str,
    edits: Vec<(Span, &'a str)>,
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
    fn visit_token(
        &mut self,
        token: &ltk_ritobin::parse::Token,
        context: &ltk_ritobin::Cst,
    ) -> Visit {
        if token.kind != TokenKind::HexLit {
            return Visit::Continue;
        }

        eprintln!("[unhash] {:?}", context.kind);
        let Some(txt) = &self.txt[token.span].strip_prefix("0x") else {
            return Visit::Continue;
        };

        let unhashed = match context.kind {
            ltk_ritobin::cst::Kind::EntryKey => {
                let Some(k) = BinHash::from_str_radix(txt, 16).ok() else {
                    return Visit::Continue;
                };
                self.hashes.fields.as_ref().and_then(|h| h.hashes.get(&k))
            }
            ltk_ritobin::cst::Kind::Class => {
                let Some(k) = BinHash::from_str_radix(txt, 16).ok() else {
                    return Visit::Continue;
                };
                self.hashes.types.as_ref().and_then(|h| h.hashes.get(&k))
            }
            _ => return Visit::Continue,
        };

        if let Some(unhashed) = unhashed {
            self.edits.push((token.span, unhashed.as_str()));
        }
        eprintln!("[unhash] -> {unhashed:?}");

        Visit::Continue
    }
}
