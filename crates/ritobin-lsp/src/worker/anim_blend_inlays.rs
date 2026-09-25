use std::{borrow::Cow, fmt::Display};

use anyhow::Context as _;
use lsp_types::{
    Command, InlayHint, InlayHintLabel, InlayHintLabelPart, InlayHintLabelPartTooltip,
    InlayHintTooltip, Position, Range, TextEdit, Url,
};
use ltk_hash::{BinHash, Hash as _};
use ltk_meta::PropertyKind;
use ltk_mimir_cache::Table;
use ltk_ritobin::{
    Spanned, SpannedExt,
    ast::{
        Property, Value,
        visitor::{Break, Descend, EnterFlow, Visitor, VisitorExt},
    },
};
use serde::{Deserialize, Serialize};

use ritobin_lsp::line_ends::LineNumbers;

use crate::{server::HashesSnapshot, worker::Unparsed, worker::Worker};

const EDIT_COMMAND: &str = "ritobin-lsp.editTransitionHash";
const BLEND_TABLE_FIELD: &str = "mBlendDataTable";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransitionHalf {
    From,
    To,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlendHash(BinHash, BinHash);

impl Display for BlendHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.pack().fmt(f)
    }
}

impl BlendHash {
    pub fn new(from: impl Into<BinHash>, to: impl Into<BinHash>) -> Self {
        Self(from.into(), to.into())
    }

    pub fn from(self) -> BinHash {
        self.0
    }
    pub fn to(self) -> BinHash {
        self.1
    }

    pub fn pack(self) -> u64 {
        ((*self.from() as u64) << 32) | *self.to() as u64
    }
    pub fn unpack(packed: u64) -> Self {
        Self::new((packed >> 32) as u32, packed as u32)
    }

    pub fn with_half(mut self, half: TransitionHalf, hash: impl Into<BinHash>) -> Self {
        self.update(half, hash);
        self
    }

    pub fn update(&mut self, half: TransitionHalf, hash: impl Into<BinHash>) {
        match half {
            TransitionHalf::From => self.0 = hash.into(),
            TransitionHalf::To => self.1 = hash.into(),
        }
    }
}

impl Worker {
    pub(super) fn inlay_hints(&self, range: Option<Range>) -> Result<Vec<InlayHint>, Unparsed> {
        let ast = self.ast()?;
        let hashes = self.server.hashes.as_ref().map(|h| h.snapshot());
        Ok(InlayVisitor {
            lines: &self.document.line_numbers,
            hashes: hashes.as_ref(),
            uri: &self.document.uri,
            blend_field: BinHash::hash_str(BLEND_TABLE_FIELD).0,
            range,
            hints: Vec::new(),
        }
        .walk(ast)
        .hints)
    }

    pub(super) fn rehash_transition(
        &self,
        range: Range,
        half: TransitionHalf,
        name: &str,
    ) -> Result<Vec<TextEdit>, anyhow::Error> {
        let ast = self.ast()?;
        let mut finder = KeyFinder {
            lines: &self.document.line_numbers,
            range,
            key: None,
        };
        ast.walk(&mut finder);
        let Some(mut key) = finder.key else {
            return Ok(Vec::new());
        };
        let hash = match name.strip_prefix("0x") {
            Some(raw) => BinHash::from_str_radix(raw, 16).context("Invalid raw hash")?,
            None => BinHash::hash_str(name),
        };
        key.value.update(half, hash);
        Ok(vec![TextEdit {
            range: self.document.line_numbers.from_span(key.span),
            new_text: key.value.to_string(),
        }])
    }
}

struct InlayVisitor<'a> {
    lines: &'a LineNumbers,
    hashes: Option<&'a HashesSnapshot>,
    uri: &'a Url,
    blend_field: u32,
    range: Option<Range>,
    hints: Vec<InlayHint>,
}

impl InlayVisitor<'_> {
    fn part(&self, key_range: Range, half: TransitionHalf, hash: BinHash) -> InlayHintLabelPart {
        let name = self
            .hashes
            .and_then(|h| h.lookup(Table::BinHashes, *hash as u64));
        let display = name
            .as_deref()
            .map(Cow::Borrowed)
            .unwrap_or_else(|| Cow::Owned(format!("0x{hash:08x}")));
        InlayHintLabelPart {
            value: display.into_owned(),
            tooltip: Some(InlayHintLabelPartTooltip::String(match name.as_ref() {
                Some(name) => format!("{name:?} ({hash:#010x})"),
                None => format!("unknown ({hash:#010x})"),
            })),
            location: None,
            command: Some(Command {
                title: "Edit animation transition".to_owned(),
                command: EDIT_COMMAND.to_owned(),
                arguments: Some(vec![serde_json::json!({
                    "uri": self.uri,
                    "range": key_range,
                    "half": half,
                    "current": name,
                })]),
            }),
        }
    }
}

impl Visitor for InlayVisitor<'_> {
    fn enter_property(&mut self, property: &Property) -> EnterFlow {
        if property.name.value.0 != self.blend_field {
            return Descend::Children.into();
        }
        let Some(Value::Map {
            key_kind: PropertyKind::U64,
            entries,
            ..
        }) = &property.value
        else {
            return Descend::Children.into();
        };
        for (key, _) in entries {
            let Value::U64(key) = key else { continue };
            let key_range = self.lines.from_span(key.span);

            // TODO: ext trait
            fn overlaps(a: Range, b: Range) -> bool {
                a.start <= b.end && b.start <= a.end
            }
            if self.range.is_some_and(|r| !overlaps(r, key_range)) {
                continue;
            }
            let hash = BlendHash::unpack(key.value);
            self.hints.push(InlayHint {
                position: Position::new(
                    key_range.end.line,
                    self.lines.line_content_end(key_range.end.line),
                ),
                label: InlayHintLabel::LabelParts(vec![
                    plain("("),
                    self.part(key_range, TransitionHalf::From, hash.from()),
                    plain(" \u{2192} "),
                    self.part(key_range, TransitionHalf::To, hash.to()),
                    plain(")"),
                ]),
                kind: None,
                text_edits: None,
                tooltip: Some(InlayHintTooltip::String(
                    "Packed animation transition (from \u{2192} to)".to_owned(),
                )),
                padding_left: Some(true),
                padding_right: Some(false),
                data: None,
            });
        }
        Descend::Children.into()
    }
}

fn plain(value: &str) -> InlayHintLabelPart {
    InlayHintLabelPart {
        value: value.to_owned(),
        tooltip: None,
        location: None,
        command: None,
    }
}

struct KeyFinder<'a> {
    lines: &'a LineNumbers,
    range: Range,
    key: Option<Spanned<BlendHash>>,
}

impl Visitor for KeyFinder<'_> {
    fn enter_value(&mut self, value: &Value) -> EnterFlow {
        if let Value::Map { entries, .. } = value {
            for (key, _) in entries {
                if let Value::U64(key) = key
                    && self.lines.from_span(key.span) == self.range
                {
                    self.key = Some(BlendHash::unpack(key.value).with_span(key.span));
                    return EnterFlow::Break(Break::Abort);
                }
            }
        }
        Descend::Children.into()
    }
}
