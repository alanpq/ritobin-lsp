use std::{fmt, ops::Deref};

use lsp_types::{Range, TextEdit};
use ltk_hash::BinHash;
use ltk_mimir_cache::Table;
use ltk_ritobin::{
    ast::{
        AstStruct, AstValue,
        hash::{HashedLiteral, Originally},
        visitor::VisitorExt,
    },
    cst::{
        NodeId, TokenId, Visitor,
        visitor::{Visit, VisitCtx},
    },
    parse::{Span, TokenKind},
};

use crate::{server::Hashes, worker::Worker};

impl Worker {
    pub fn unhash(&self, _range: Option<Range>) -> anyhow::Result<Option<Vec<TextEdit>>> {
        let Some(ast) = self.ast.as_ref() else {
            return Ok(None);
        };

        let Some(hashes) = self.server.hashes.as_ref() else {
            // TODO: propagate this err to client
            return Ok(None);
        };

        let unhasher = Unhasher::new(hashes).walk(ast);

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
    edits: Vec<(Span, String)>,
}

enum OutputFormat {
    String,
    Name,
}

impl OutputFormat {
    fn make_output(&self, inner: impl fmt::Display) -> String {
        match self {
            Self::String => format!("\"{inner}\""),
            Self::Name => inner.to_string(),
        }
    }
}

impl<'a> Unhasher<'a> {
    pub fn new(hashes: &'a Hashes) -> Self {
        Self {
            hashes,
            edits: vec![],
        }
    }

    fn unhash<H, T>(&mut self, hash: &HashedLiteral<H>, table: Table, format: OutputFormat)
    where
        H: ltk_hash::Hash + Deref<Target = T>,
        T: Into<u64> + Copy,
    {
        if hash.was_hash() {
            let table = self.hashes.table(table);
            if let Some(unhashed) = table.as_ref().and_then(|h| h.get((*hash.value).into())) {
                self.edits.push((hash.span(), format.make_output(unhashed)));
            }
        }
    }
}

impl<'a> ltk_ritobin::ast::visitor::Visitor for Unhasher<'a> {
    fn enter_struct(&mut self, s: &AstStruct) -> Visit {
        self.unhash(&s.class_hash, Table::BinTypes, OutputFormat::Name);
        Visit::Continue
    }

    fn enter_property(&mut self, property: &ltk_ritobin::ast::AstProperty) -> Visit {
        self.unhash(&property.name, Table::BinFields, OutputFormat::Name);
        Visit::Continue
    }

    fn enter_object(&mut self, object: &ltk_ritobin::ast::AstObject) -> Visit {
        self.unhash(&object.path_hash, Table::BinEntries, OutputFormat::String);
        Visit::Continue
    }

    fn enter_value(&mut self, value: &AstValue) -> Visit {
        match value {
            AstValue::Hash(hash) => {
                self.unhash(hash, Table::BinHashes, OutputFormat::String);
            }
            AstValue::WadChunkLink(hash) => {
                self.unhash(hash, Table::Game, OutputFormat::String);
            }
            AstValue::ObjectLink(hash) => {
                self.unhash(hash, Table::BinEntries, OutputFormat::String);
            }
            _ => {}
        }
        Visit::Continue
    }
}
