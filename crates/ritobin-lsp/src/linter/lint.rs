use lsp_types::{Diagnostic as LspDiag, DiagnosticSeverity, DiagnosticTag};
use ltk_ritobin::{RitoType, cst::NodeId, parse::Span};

use crate::{
    config::{LintSeverity, LintsConfig},
    document::Document,
    worker::code_actions::CodeActionData,
};

pub enum Lint {
    /// Field doesn't exist in known meta class
    UnknownField {
        entry: NodeId,
        span: Span,
        class: Span,
    },
    MismatchedMetaTypeArg {
        entry: NodeId,
        class: Span,
        key: Span,
        type_expr: Span,
        expected: RitoType,
        got: RitoType,
    },
    /// Entry has the same value as the class' default
    DefaultValue { entry: NodeId, span: Span },
}

impl Lint {
    pub fn into_lsp_diagnostic(
        self,
        document: &Document,
        lints: &LintsConfig,
    ) -> Option<(LspDiag, Option<CodeActionData>)> {
        match self {
            Lint::UnknownField {
                entry,
                span,
                class: _,
            } => Some((
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
                entry: _,
                class: _,
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
