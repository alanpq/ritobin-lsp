use itertools::Itertools as _;
use lsp_types::{
    Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, Location,
    PublishDiagnosticsParams,
    notification::{Notification as _, PublishDiagnostics},
};
use ltk_ritobin::{
    Cst,
    cst::FlatErrors,
    parse::ErrorKind,
    typecheck::visitor::{DiagnosticWithSpan, TypeChecker},
};

use crate::worker::Worker;

impl Worker {
    fn convert_diagnostic(&self, d: DiagnosticWithSpan) -> Diagnostic {
        match d.diagnostic {
            ltk_ritobin::typecheck::visitor::Diagnostic::TypeMismatch {
                span,
                expected,
                expected_span,
                got,
            } => Diagnostic {
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
            ltk_ritobin::typecheck::visitor::Diagnostic::ShadowedEntry { shadowee, shadower } => {
                Diagnostic {
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
                }
            }
            ltk_ritobin::typecheck::visitor::Diagnostic::RootNonEntry => Diagnostic {
                range: self.document.line_numbers.from_span(d.span),
                severity: Some(DiagnosticSeverity::ERROR),
                message: "Top-level bin entries must be of form 'name: type = ..'".into(),
                ..Default::default()
            },
            ltk_ritobin::typecheck::visitor::Diagnostic::UnexpectedSubtypes {
                base_type, ..
            } => Diagnostic {
                range: self.document.line_numbers.from_span(d.span),
                severity: Some(DiagnosticSeverity::ERROR),
                message: format!(
                    "{} does not accept type parameters",
                    &self.document.text.as_str()[base_type]
                ),
                ..Default::default()
            },
            ltk_ritobin::typecheck::visitor::Diagnostic::UnexpectedContainerItem {
                span,
                expected,
                expected_span: _,
            } => {
                let mut expected = expected.to_string();
                make_ascii_titlecase(&mut expected);
                Diagnostic {
                    range: self.document.line_numbers.from_span(span),
                    severity: Some(DiagnosticSeverity::ERROR),
                    message: format!(
                        "{expected} type does not accept container items / blocks!\nRemove any curly braces surrounding the value."
                    ),
                    ..Default::default()
                }
            }
            inner => Diagnostic {
                range: self.document.line_numbers.from_span(d.span),

                severity: Some(DiagnosticSeverity::ERROR),
                message: format!("{inner:?}"),
                ..Default::default()
            },
        }
    }

    pub fn publish_parse_errors(
        &self,
        cst: &Cst,
        bin_errors: impl IntoIterator<Item = DiagnosticWithSpan>,
    ) -> anyhow::Result<()> {
        let mut diagnostics = bin_errors
            .into_iter()
            .map(|d| self.convert_diagnostic(d))
            .update(|d| {
                d.source.replace("ritobin-lsp".into());
            })
            .collect_vec();

        let mut parse_errors = FlatErrors::new();
        cst.walk(&mut parse_errors);
        let parse_errors = parse_errors.into_errors();

        diagnostics.extend(parse_errors.into_iter().map(|err| Diagnostic {
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
        }));

        diagnostics.truncate(20);
        let params = PublishDiagnosticsParams {
            uri: self.document.uri.clone(),
            diagnostics,
            version: None,
        };
        self.server
            .conn
            .sender
            .send(lsp_server::Message::Notification(
                lsp_server::Notification::new(PublishDiagnostics::METHOD.to_owned(), params),
            ))?;
        Ok(())
    }
}

fn make_ascii_titlecase(s: &mut str) {
    if let Some(r) = s.get_mut(0..1) {
        r.make_ascii_uppercase();
    }
}
