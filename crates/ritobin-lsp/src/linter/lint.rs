use lsp_types::{Diagnostic as LspDiag, DiagnosticSeverity};
use ltk_ritobin::parse::Span;

use crate::{document::Document, worker::code_actions::CodeActionData};

pub enum Lint {
    /// Field doesn't exist in known meta class
    UnknownField { span: Span, class: Span },
}

impl Lint {
    pub fn into_lsp_diagnostic(self, document: &Document) -> (LspDiag, Option<CodeActionData>) {
        match self {
            Lint::UnknownField { span, class: _ } => (
                LspDiag {
                    range: document.line_numbers.from_span(span),
                    message: format!("Unknown field '{}'", &document.text[span]),
                    severity: Some(DiagnosticSeverity::WARNING),
                    ..Default::default()
                },
                Some(CodeActionData::RemoveEntry),
            ),
        }
    }
}
