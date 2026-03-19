use anyhow::Result;
use itertools::Itertools;
use lsp_server::{Connection, Message};
use lsp_types::{
    Diagnostic,
    DiagnosticRelatedInformation, DiagnosticSeverity, Location,
    PublishDiagnosticsParams, Url,
    notification::PublishDiagnostics,
};
use lsp_types::request::Request as _;
use lsp_types::notification::Notification as _;
use ltk_ritobin::{
    cst::{Cst, FlatErrors},
    parse::{self, ErrorKind},
    typecheck::visitor::TypeChecker,
};
use ritobin_lsp::line_ends::LineNumbers;

pub struct Document {
    pub uri: Url,
    pub text: String,
    pub cst: Cst,
    pub parse_errors: Vec<parse::Error>,
    pub line_numbers: LineNumbers,
}

macro_rules! match_token {
    ($expr:expr, $kind:path) => {{
        match $expr {
            Child::Token(token @ Token { kind: $kind, .. }) => Some(token),
            _ => None,
        }
    }};
}
macro_rules! match_tree {
    ($expr:expr, $kind:path) => {{
        match $expr {
            Child::Tree(tree @ Cst { kind: $kind, .. }) => Some(tree),
            _ => None,
        }
    }};
}
impl Document {
    pub fn new(uri: Url, text: String) -> Self {
        let cst = parse::parse(&text);
        let parse_errors = FlatErrors::walk(&cst);
        Self {
            uri,
            cst,
            parse_errors,
            line_numbers: LineNumbers::new(&text),
            text,
        }
    }

    pub fn publish_parse_errors(&self, conn: &Connection) -> Result<()> {
        let mut visitor = TypeChecker::new(&self.text);
        self.cst.walk(&mut visitor);

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
                        range: self.line_numbers.from_span(span),
                        severity: Some(DiagnosticSeverity::ERROR),
                        related_information: expected_span.map(|span| {
                            vec![DiagnosticRelatedInformation {
                                location: Location {
                                    uri: self.uri.clone(),
                                    range: self.line_numbers.from_span(span),
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
                        range: self.line_numbers.from_span(d.span),
                        severity: Some(DiagnosticSeverity::WARNING),

                        related_information: Some(vec![DiagnosticRelatedInformation {
                            location: Location {
                                uri: self.uri.clone(),
                                range: self.line_numbers.from_span(shadowee),
                            },
                            message: "Shadowed here".into(),
                        }]),

                        message: format!(
                            "Entry '{}' shadows previous entry",
                            &self.text.as_str()[shadower]
                        ),
                        ..Default::default()
                    },
                    ltk_ritobin::typecheck::visitor::Diagnostic::RootNonEntry => Diagnostic {
                        range: self.line_numbers.from_span(d.span),
                        severity: Some(DiagnosticSeverity::ERROR),
                        message: "Top-level bin entries must be of form 'name: type = ..'".into(),
                        ..Default::default()
                    },
                    ltk_ritobin::typecheck::visitor::Diagnostic::UnexpectedSubtypes {
                        base_type,
                        ..
                    } => Diagnostic {
                        range: self.line_numbers.from_span(d.span),
                        severity: Some(DiagnosticSeverity::ERROR),
                        message: format!(
                            "{} does not accept type parameters",
                            &self.text.as_str()[base_type]
                        ),
                        ..Default::default()
                    },
                    inner => Diagnostic {
                        range: self.line_numbers.from_span(d.span),
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

        for err in &self.parse_errors {
            diagnostics.push(Diagnostic {
                range: self.line_numbers.from_span(err.span),
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
            uri: self.uri.clone(),
            diagnostics,
            version: None,
        };
        conn.sender
            .send(Message::Notification(lsp_server::Notification::new(
                PublishDiagnostics::METHOD.to_owned(),
                params,
            )))?;
        Ok(())
    }
}
