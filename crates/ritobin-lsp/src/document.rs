use anyhow::{Context, Result, anyhow, bail};
use itertools::Itertools;
use lsp_server::{Connection, Message, Request as ServerRequest, RequestId, Response};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionOptions, CompletionResponse, Diagnostic,
    DiagnosticSeverity, DidChangeTextDocumentParams, DidOpenTextDocumentParams,
    DocumentFormattingParams, Hover, HoverContents, HoverProviderCapability, InitializeParams,
    MarkedString, OneOf, Position, PublishDiagnosticsParams, Range, SemanticTokens,
    SemanticTokensFullOptions, SemanticTokensParams, ServerCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit, Url,
    notification::{DidChangeTextDocument, DidOpenTextDocument, PublishDiagnostics},
    request::{Completion, Formatting, GotoDefinition, HoverRequest, SemanticTokensFullRequest},
};
use lsp_types::{SemanticToken, request::Request as _};
use lsp_types::{WorkDoneProgressOptions, notification::Notification as _};
use ltk_ritobin::{
    parse::{
        self, ErrorKind, Span, Token, TokenKind,
        cst::{Child, Cst, FlatErrors, TreeKind, Visitor, visitor::Visit},
    },
    typecheck::visitor::TypeChecker,
};
use paths::{AbsPathBuf, Utf8PathBuf};
use ritobin_lsp::{cst_ext::CstExt, from_json, line_ends::LineNumbers};
use rustc_hash::FxHashMap;
use std::process::Stdio;
use tracing_subscriber::{
    Layer as _, Registry,
    filter::Targets,
    fmt::{time, writer::BoxMakeWriter},
    layer::SubscriberExt as _,
};

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

        let mut diagnostics = visitor
            .into_diagnostics()
            .into_iter()
            .map(|d| Diagnostic {
                range: self.line_numbers.from_span(d.span),
                severity: Some(DiagnosticSeverity::ERROR),
                code: None,
                code_description: None,
                source: Some("ritobin-lsp".into()),
                message: match d.diagnostic {
                    ltk_ritobin::typecheck::visitor::Diagnostic::RootNonEntry => {
                        "Top-level bin entries must be of form 'name: type = ..'".into()
                    }
                    ltk_ritobin::typecheck::visitor::Diagnostic::UnexpectedSubtypes {
                        base_type,
                        ..
                    } => {
                        format!(
                            "{} does not accept type parameters",
                            &self.text.as_str()[base_type]
                        )
                    }
                    d => format!("{d:?}"),
                },
                related_information: None,
                tags: None,
                data: None,
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
