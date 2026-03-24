use itertools::Itertools as _;
use lsp_types::{
    Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, Location,
    PublishDiagnosticsParams,
    notification::{Notification as _, PublishDiagnostics},
};
use ltk_ritobin::{cst::FlatErrors, parse::ErrorKind, typecheck::visitor::TypeChecker};

use crate::worker::Worker;

impl Worker {
    pub fn publish_parse_errors(&self) -> anyhow::Result<()> {
        let mut visitor = TypeChecker::new(&self.document.text);
        let Some(cst) = self.cst.as_ref() else {
            return Ok(());
        };
        cst.walk(&mut visitor);

        let (_roots, diagnostics) = visitor.into_parts();

        let mut diagnostics = diagnostics
            .into_iter()
            .flat_map(|d| {
                [match d.diagnostic {
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
                    ltk_ritobin::typecheck::visitor::Diagnostic::ShadowedEntry {
                        shadowee,
                        shadower,
                    } => Diagnostic {
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
                    ltk_ritobin::typecheck::visitor::Diagnostic::RootNonEntry => Diagnostic {
                        range: self.document.line_numbers.from_span(d.span),
                        severity: Some(DiagnosticSeverity::ERROR),
                        message: "Top-level bin entries must be of form 'name: type = ..'".into(),
                        ..Default::default()
                    },
                    ltk_ritobin::typecheck::visitor::Diagnostic::UnexpectedSubtypes {
                        base_type,
                        ..
                    } => Diagnostic {
                        range: self.document.line_numbers.from_span(d.span),
                        severity: Some(DiagnosticSeverity::ERROR),
                        message: format!(
                            "{} does not accept type parameters",
                            &self.document.text.as_str()[base_type]
                        ),
                        ..Default::default()
                    },
                    inner => Diagnostic {
                        range: self.document.line_numbers.from_span(d.span),

                        severity: Some(DiagnosticSeverity::ERROR),
                        message: format!("{inner:?}"),
                        ..Default::default()
                    },
                }]
            })
            .update(|d| {
                d.source.replace("ritobin-lsp".into());
            })
            .collect_vec();

        let mut parse_errors = FlatErrors::new();
        cst.walk(&mut parse_errors);
        let parse_errors = parse_errors.into_errors();

        for err in parse_errors {
            diagnostics.push(Diagnostic {
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
            });
        }

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
