use std::{
    borrow::Cow,
    fmt::{self, Display},
    marker::PhantomData,
    ops::Deref,
};

use lsp_types::{Range, TextEdit};
use ltk_mimir_cache::Table;
use ltk_ritobin::{
    Spanned,
    ast::{
        Ast, AstObject, AstProperty, AstStruct, AstValue,
        hash::HashedLiteral,
        visitor::{Visit, Visitor, VisitorExt},
    },
};

use crate::{server::HashesSnapshot, worker::Worker};

impl Worker {
    pub fn unhash(&self, _range: Option<Range>) -> anyhow::Result<Option<Vec<TextEdit>>> {
        let Some(ast) = self.ast.as_ref() else {
            return Ok(None);
        };

        let Some(hashes) = self.server.hashes.as_ref().map(|h| h.snapshot()) else {
            // TODO: propagate this err to client
            return Ok(None);
        };

        let edits = Unhasher::new(&hashes, |report: Report<'_>| TextEdit {
            range: self.document.line_numbers.from_span(report.hash.span),
            new_text: report.unhash.to_string(),
        })
        .walk(ast);

        Ok(Some(edits))
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Report<'a> {
    pub hash: Spanned<u64>,
    pub table: Table,
    pub unhash: Unhash<'a>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Unhash<'a> {
    pub value: Cow<'a, str>,
    pub format: OutputFormat,
}

impl<'a> Unhash<'a> {
    pub fn new(value: impl Into<Cow<'a, str>>, format: OutputFormat) -> Self {
        Self {
            value: value.into(),
            format,
        }
    }
}

impl Display for Unhash<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Unhash { value, format } = self;
        match format {
            OutputFormat::Name => value.fmt(f),
            OutputFormat::String => write!(f, "\"{value}\""),
        }
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum OutputFormat {
    String,
    Name,
}

pub trait Map<T, U> {
    fn map(&self, from: T) -> U;
}

impl<T, U, F: Fn(T) -> U> Map<T, U> for F {
    fn map(&self, from: T) -> U {
        (self)(from)
    }
}

/// Looks up hashed AST literals against the given hash tables.
///
/// Provides [`Self::unhash_struct`],[`Self::unhash_property`],[`Self::unhash_object`],[`Self::unhash_value`]
#[derive(Clone, Copy)]
pub struct Unhasher<'a, M: Map<Report<'a>, O>, O> {
    hashes: &'a HashesSnapshot,
    mapper: M,
    _p: PhantomData<O>,
}

impl<'a, M: Map<Report<'a>, O>, O> Unhasher<'a, M, O> {
    pub fn new(hashes: &'a HashesSnapshot, mapper: M) -> Self {
        Self {
            hashes,
            mapper,
            _p: PhantomData,
        }
    }

    pub fn walk(self, ast: &Ast) -> Vec<O> {
        Walker::new(self).run(ast)
    }

    fn unhash<H, T>(&self, hash: &HashedLiteral<H>, table: Table, format: OutputFormat) -> Option<O>
    where
        H: ltk_hash::Hash + Deref<Target = T>,
        T: Into<u64> + Copy,
    {
        if !hash.was_hash() {
            return None;
        }

        let hash_val = (*hash.value).into();
        let unhash = self.hashes.lookup(table, hash_val)?;
        Some(self.mapper.map(Report {
            table,
            hash: Spanned::new(hash.span(), hash_val),
            unhash: Unhash::new(unhash, format),
        }))
    }

    pub fn unhash_struct(&self, s: &AstStruct) -> Option<O> {
        self.unhash(&s.class_hash, Table::BinTypes, OutputFormat::Name)
    }

    pub fn unhash_property(&self, property: &AstProperty) -> Option<O> {
        self.unhash(&property.name, Table::BinFields, OutputFormat::Name)
    }

    pub fn unhash_object(&self, object: &AstObject) -> Option<O> {
        self.unhash(&object.path_hash, Table::BinEntries, OutputFormat::String)
    }

    pub fn unhash_value(&self, value: &AstValue) -> Option<O> {
        match value {
            AstValue::Hash(hash) => self.unhash(hash, Table::BinHashes, OutputFormat::String),
            AstValue::WadChunkLink(hash) => self.unhash(hash, Table::Game, OutputFormat::String),
            AstValue::ObjectLink(hash) => {
                self.unhash(hash, Table::BinEntries, OutputFormat::String)
            }
            _ => None,
        }
    }
}

struct Walker<'a, M: Map<Report<'a>, O>, O> {
    unhasher: Unhasher<'a, M, O>,
    items: Vec<O>,
}

impl<'a, M: Map<Report<'a>, O>, O> Walker<'a, M, O> {
    pub fn new(unhasher: Unhasher<'a, M, O>) -> Self {
        Self {
            unhasher,
            items: vec![],
        }
    }

    pub fn run(self, ast: &Ast) -> Vec<O> {
        self.walk(ast).items
    }
}

impl<'a, M: Map<Report<'a>, O>, O> Visitor for Walker<'a, M, O> {
    fn enter_struct(&mut self, s: &AstStruct) -> Visit {
        if let Some(item) = self.unhasher.unhash_struct(s) {
            self.items.push(item);
        }
        Visit::Continue
    }

    fn enter_property(&mut self, property: &AstProperty) -> Visit {
        if let Some(item) = self.unhasher.unhash_property(property) {
            self.items.push(item);
        }
        Visit::Continue
    }

    fn enter_object(&mut self, object: &AstObject) -> Visit {
        if let Some(item) = self.unhasher.unhash_object(object) {
            self.items.push(item);
        }
        Visit::Continue
    }

    fn enter_value(&mut self, value: &AstValue) -> Visit {
        if let Some(item) = self.unhasher.unhash_value(value) {
            self.items.push(item);
        }
        Visit::Continue
    }
}
