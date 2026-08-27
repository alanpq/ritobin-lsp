use std::fmt::Display;

use lsp_types::{
    Diagnostic as LspDiag, DiagnosticRelatedInformation, DiagnosticSeverity, DiagnosticTag,
    Location,
};
use ltk_mimir_cache::Table;
use ltk_ritobin::{RitoType, Spanned, ast::diagnostics::RitoTypeOrVirtual, parse::Span};

use crate::{
    config::{LintSeverity, LintsConfig},
    document::Document,
    worker::code_actions::CodeActionData,
};

pub enum Lint {
    /// Field doesn't exist in known meta class
    UnknownField { entry: Span, span: Span },
    MismatchedMetaTypeArg {
        key: Span,
        type_expr: Span,
        expected: RitoType,
        got: RitoTypeOrVirtual,
    },
    /// Entry has the same value as the class' default
    DefaultValue { entry: Span, span: Span },
    /// Entry is shadowed by earlier entry with same key
    ShadowedEntry { entry: Span, shadowed_by: Span },
    KnownHash {
        table: Table,
        hash: Spanned<u64>,
        value: String,
    },
}

fn human_hash_name(table: Table) -> Ascii<'static> {
    match table {
        Table::Game => Ascii("wad"),
        Table::Lcu => Ascii("LCU"),
        Table::BinEntries => Ascii("bin entry"),
        Table::BinTypes => Ascii("bin type"),
        Table::BinFields => Ascii("bin field"),
        Table::BinHashes => Ascii("bin"),
        Table::Rst => Ascii("RST"),
        Table::RstXxh3 => Ascii("RST (xxh3)"),
    }
}

/// A string that is known to be *entirely* ASCII.
struct Ascii<'a>(&'a str);
struct Cap<'a>(pub Ascii<'a>);

impl Display for Cap<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0.0.len() {
            1 => write!(
                f,
                "{}",
                self.0.0.chars().next().unwrap().to_ascii_uppercase()
            ),
            _ => write!(
                f,
                "{}{}",
                self.0.0.chars().next().unwrap().to_ascii_uppercase(),
                &self.0.0[1..]
            ),
        }
    }
}

impl Lint {
    pub fn into_lsp_diagnostic(
        self,
        document: &Document,
        lints: &LintsConfig,
    ) -> Option<(LspDiag, Option<CodeActionData>)> {
        match self {
            Lint::ShadowedEntry { entry, shadowed_by } => Some((
                LspDiag {
                    range: document.line_numbers.from_span(entry),
                    message: format!(
                        "Entry '{}' shadows (overrides) an already defined earlier entry in this block.",
                        &document.text[entry]
                    ),
                    severity: Some(DiagnosticSeverity::WARNING),
                    related_information: Some(vec![DiagnosticRelatedInformation {
                        location: Location {
                            uri: document.uri.clone(),
                            range: document.line_numbers.from_span(shadowed_by),
                        },
                        message: "The shadowed/overriden entry is here".into(),
                    }]),

                    ..Default::default()
                },
                None,
            )),
            Lint::KnownHash { table, hash, value } => {
                let table = Cap(human_hash_name(table));
                Some((
                    LspDiag {
                        range: document.line_numbers.from_span(hash.span),
                        message: format!("{table} hash 0x{} has known value.", hash.value),
                        severity: Some(DiagnosticSeverity::WARNING),
                        ..Default::default()
                    },
                    Some(CodeActionData::Replace {
                        span: hash.span,
                        with: value,
                    }),
                ))
            }

            Lint::UnknownField { entry, span } => Some((
                LspDiag {
                    range: document.line_numbers.from_span(span),
                    message: format!("Unknown field '{}'", &document.text[span]),
                    severity: Some(DiagnosticSeverity::WARNING),
                    ..Default::default()
                },
                Some(CodeActionData::RemoveEntry(entry)),
            )),
            Lint::DefaultValue { entry, span } => {
                let severity = lints
                    .default_value
                    .and_then(|c| c.severity)
                    .unwrap_or(LintSeverity::Hint)
                    .to_lsp()?;
                Some((
                    LspDiag {
                        range: document.line_numbers.from_span(span),
                        message: "Entry has default value".into(),
                        severity: Some(severity),
                        tags: Some(vec![DiagnosticTag::UNNECESSARY]),
                        ..Default::default()
                    },
                    Some(CodeActionData::RemoveEntry(entry)),
                ))
            }
            Lint::MismatchedMetaTypeArg {
                key,
                type_expr,
                expected,
                got,
            } => Some((
                LspDiag {
                    range: document.line_numbers.from_span(type_expr),
                    message: format!(
                        "Class property type mismatch - {} has type {expected}, but got {got}",
                        &document.text[key]
                    ),
                    severity: Some(DiagnosticSeverity::WARNING),
                    ..Default::default()
                },
                Some(CodeActionData::TypeMismatch {
                    type_expr,
                    expected,
                }),
            )),
        }
    }
}
