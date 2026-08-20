use std::num::IntErrorKind;

use itertools::Itertools as _;
use lsp_types::{
    Diagnostic as LspDiag, DiagnosticRelatedInformation, DiagnosticSeverity, Location, Range,
};
use ltk_ritobin::{
    Cst, RitoType,
    parse::ErrorKind,
    typecheck::diagnostics::{Diagnostic, DiagnosticWithSpan},
};

use crate::{config::LintsConfig, linter::Linter, worker::Worker};

impl Worker {
    fn convert_diagnostic(&self, d: DiagnosticWithSpan) -> LspDiag {
        match d.diagnostic {
            Diagnostic::CustomSpan(msg, span) => LspDiag {
                range: self.document.line_numbers.from_span(span),

                severity: Some(DiagnosticSeverity::ERROR),
                message: msg.to_string(),
                ..Default::default()
            },
            Diagnostic::NotEnoughItems {
                span,
                got,
                expected,
            } => LspDiag {
                range: self.document.line_numbers.from_span(span),
                severity: Some(DiagnosticSeverity::ERROR),
                message: format!(
                    "{expected} needs {} items - got {got}.",
                    expected.needed_children()
                ),

                ..Default::default()
            },
            Diagnostic::TypeMismatch {
                span,
                expected,
                expected_span,
                got,
            } => LspDiag {
                range: self.document.line_numbers.from_span(span),
                severity: Some(DiagnosticSeverity::ERROR),
                related_information: expected_span.map(|span| {
                    vec![DiagnosticRelatedInformation {
                        location: Location {
                            uri: self.document.uri.clone(),
                            range: self.document.line_numbers.from_span(span),
                        },
                        message: "due to this type expression".into(),
                    }]
                }),
                message: format!("Type mismatch - expected {expected}, got {got}"),
                ..Default::default()
            },
            Diagnostic::ParseNumericError {
                expected,
                error,
                span,
            } => {
                let expected = RitoType::simple(expected);
                let error = match error {
                    Some(IntErrorKind::Empty) => "cannot parse integer from empty literal",
                    Some(IntErrorKind::InvalidDigit) => "invalid digit found in literal",
                    Some(IntErrorKind::PosOverflow) => "number too large to fit in target type",
                    Some(IntErrorKind::NegOverflow) => "number too small to fit in target type",
                    Some(IntErrorKind::Zero) => "number would be zero",
                    _ => "invalid literal",
                };
                LspDiag {
                    range: self.document.line_numbers.from_span(span),
                    severity: Some(DiagnosticSeverity::ERROR),
                    message: format!("Could not parse {expected} - {error}"),
                    ..Default::default()
                }
            }
            Diagnostic::ShadowedEntry { shadowee, shadower } => LspDiag {
                range: self.document.line_numbers.from_span(d.span),
                severity: Some(DiagnosticSeverity::WARNING),

                related_information: Some(vec![DiagnosticRelatedInformation {
                    location: Location {
                        uri: self.document.uri.clone(),
                        range: self.document.line_numbers.from_span(shadowee),
                    },
                    message: "Shadowed here".into(),
                }]),

                message: format!(
                    "Entry '{}' shadows previous entry",
                    &self.document.text.as_str()[shadower]
                ),
                ..Default::default()
            },
            Diagnostic::RootNonEntry => LspDiag {
                range: self.document.line_numbers.from_span(d.span),
                severity: Some(DiagnosticSeverity::ERROR),
                message: "Top-level bin entries must be of form 'name: type = ..'".into(),
                ..Default::default()
            },
            Diagnostic::UnexpectedSubtypes { base_type, .. } => LspDiag {
                range: self.document.line_numbers.from_span(d.span),
                severity: Some(DiagnosticSeverity::ERROR),
                message: format!(
                    "{} does not accept type parameters",
                    &self.document.text.as_str()[base_type]
                ),
                ..Default::default()
            },
            Diagnostic::UnexpectedContainerItem {
                span,
                expected,
                expected_span: _,
            } => {
                let mut expected = expected.to_string();
                make_ascii_titlecase(&mut expected);
                LspDiag {
                    range: self.document.line_numbers.from_span(span),
                    severity: Some(DiagnosticSeverity::ERROR),
                    message: format!(
                        "{expected} type does not accept container items / blocks!\nRemove any curly braces surrounding the value."
                    ),
                    ..Default::default()
                }
            }
            inner => LspDiag {
                range: self.document.line_numbers.from_span(d.span),

                severity: Some(DiagnosticSeverity::ERROR),
                message: format!("{inner:?}"),
                ..Default::default()
            },
        }
    }

    /// `typecheck` is `None` when the typechecker panicked on this tree.
    pub fn publish_parse_errors(
        &mut self,
        // cst: &Cst,
        typecheck: Option<Vec<DiagnosticWithSpan>>,
    ) -> anyhow::Result<()> {
        let Some(cst) = self.cst.as_ref() else {
            return Ok(());
        };

        let mut diagnostics = cst
            .errors
            .iter()
            .map(|err| LspDiag {
                range: self.document.line_numbers.from_span(err.span),
                severity: Some(DiagnosticSeverity::ERROR),
                code: None,
                code_description: None,
                source: Some("ritobin-lsp".into()),
                message: match err.kind {
                    ErrorKind::Expected { expected, got } => {
                        format!("Missing {expected} for {} - got {got}", err.tree)
                    }
                    ErrorKind::Unexpected { token } => {
                        format!("Unexpected {token}, expected {}", err.tree)
                    }
                    kind => format!("{kind:#?}"),
                },
                related_information: None,
                tags: None,
                data: None,
            })
            .collect_vec();

        // let mut parse_errors = FlatErrors::new();
        // cst.walk(&mut parse_errors);

        match typecheck {
            Some(errors) => diagnostics.extend(
                errors
                    .into_iter()
                    .map(|d| self.convert_diagnostic(d))
                    .update(|d| {
                        d.source.replace("ritobin-lsp".into());
                    }),
            ),
            // Say so rather than silently dropping to syntax-only diagnostics.
            None => diagnostics.push(LspDiag {
                range: Range::default(),
                severity: Some(DiagnosticSeverity::WARNING),
                source: Some("ritobin-lsp".into()),
                message: "Type checking crashed - only syntax errors are \
                          reported until the file parses cleanly."
                    .into(),
                ..Default::default()
            }),
        }

        let classes = self.server.meta.classes.clone();
        let classes = classes.read();
        let mut linter = Linter::new(&self.document, &classes);
        cst.walk(&mut linter);

        let default_lints_config = LintsConfig::default();
        let lints_config = self
            .server
            .config
            .initialization_options
            .as_ref()
            .and_then(|opt| opt.lints.as_ref())
            .unwrap_or(&default_lints_config);

        for (mut diag, action) in linter
            .lints
            .into_iter()
            .filter_map(|l| l.into_lsp_diagnostic(&self.document, lints_config))
            .collect::<Vec<_>>()
        {
            if let Some(action) = action {
                diag.data.replace(self.register_code_action(action).into());
            }
            diagnostics.push(diag);
        }

        diagnostics.truncate(
            self.server
                .config
                .initialization_options
                .as_ref()
                .and_then(|opt| opt.diagnostic_limit)
                .unwrap_or(1000),
        );

        self.server
            .publish_diagnostics(self.document.uri.clone(), diagnostics)
    }
}

fn make_ascii_titlecase(s: &mut str) {
    if let Some(r) = s.get_mut(0..1) {
        r.make_ascii_uppercase();
    }
}
