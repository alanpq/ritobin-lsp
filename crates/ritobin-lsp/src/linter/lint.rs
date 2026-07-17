use lsp_types::{Diagnostic as LspDiag, DiagnosticSeverity, Url};
use ltk_ritobin::parse::Span;

use crate::{document::Document, lol_meta::schema::U32Hash};

pub enum Lint {
    /// Field doesn't exist in known meta class
    UnknownField { span: Span, class: Span },
}

impl Lint {
    pub fn into_lsp_diagnostic(self, document: &Document) -> LspDiag {
        match self {
            Lint::UnknownField { span, class } => LspDiag {
                range: document.line_numbers.from_span(span),
                message: format!("Unknown field '{}'", &document.text[span]),
                severity: Some(DiagnosticSeverity::WARNING),
                ..Default::default()
            },
        }
    }
}
