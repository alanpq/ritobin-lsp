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
use ltk_ritobin::parse::{
    self, ErrorKind, Span, Token, TokenKind,
    cst::{Child, Cst, FlatErrors, TreeKind, Visitor, visitor::Visit},
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
        struct TypeChecker<'a> {
            text: &'a str,
            lines: &'a LineNumbers,
            diagnostics: Vec<Diagnostic>,
        }

        impl TypeChecker<'_> {
            fn report(&mut self, span: Span, message: impl Into<String>) {
                self.diagnostics.push(Diagnostic {
                    range: self.lines.from_span(span),
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: None,
                    code_description: None,
                    source: Some("ritobin-lsp".into()),
                    message: message.into(),
                    related_information: None,
                    tags: None,
                    data: None,
                });
            }

            fn eat_equals<'a>(
                &mut self,
                type_name: &str,
                children: &mut impl Iterator<Item = &'a Child>,
            ) -> Option<crate::Span> {
                let equals = children.next()?;
                if matches!(
                    equals,
                    Child::Token(Token {
                        kind: TokenKind::LBrack,
                        ..
                    }),
                ) {
                    self.report(
                        equals.span(),
                        format!("type {type_name} does not take type parameters"),
                    );
                }
                Some(equals.span())
            }

            fn expect_type_params<'a, 'b, T: Iterator<Item = &'b Child>>(
                &'a mut self,
                type_children: &mut T,
            ) -> Option<impl Iterator<Item = &'b Child> + use<'b, T>> {
                match_token!(type_children.next()?, TokenKind::LBrack)?;
                let next = type_children.next()?;
                let Some(arg_list) = match_tree!(next, TreeKind::TypeArgList) else {
                    self.report(next.span(), "list requires type parameter");
                    return None;
                };

                Some(arg_list.children.iter().filter(|c| {
                    !matches!(
                        c,
                        Child::Token(Token {
                            kind: TokenKind::Comma,
                            ..
                        })
                    )
                }))
            }
        }

        impl Visitor for TypeChecker<'_> {
            fn enter_tree(&mut self, tree: &Cst) -> Visit {
                let mut children = tree.children.iter();

                fn option_to_visit<F>(f: F) -> Visit
                where
                    F: FnOnce() -> Option<()>,
                {
                    if f().is_none() {
                        return Visit::Skip;
                    }
                    Visit::Continue
                }

                option_to_visit(|| {
                    #[allow(clippy::single_match)]
                    match tree.kind {
                        TreeKind::Entry => {
                            match_tree!(children.next()?, TreeKind::EntryKey)?;
                            match_token!(children.next()?, TokenKind::Colon)?;
                            let tree = match_tree!(children.next()?, TreeKind::TypeExpr)?;
                            let mut type_children = tree.children.iter();
                            let type_name = type_children.next()?;

                            match &self.text[type_name.span()] {
                                name @ ("u8" | "u16" | "u32" | "i8" | "i16" | "i32") => {
                                    self.eat_equals(name, &mut children)?;
                                }
                                name @ "string" => {
                                    self.eat_equals(name, &mut children)?;

                                    let value =
                                        match_tree!(children.next()?, TreeKind::EntryValue)?;

                                    let mut children = value.children.iter();
                                    let next = children.next()?;
                                    if match_token!(next, TokenKind::String).is_none() {
                                        self.report(
                                            next.span(),
                                            "type of bin value must be string",
                                        );
                                        return None;
                                    };
                                }
                                "map" => {}
                                "embed" => {}
                                "list" => {
                                    let Some(mut args) =
                                        self.expect_type_params(&mut type_children)
                                    else {
                                        self.report(type_name.span(), "missing type parameter");
                                        return None;
                                    };

                                    let kind = match args.next() {
                                        Some(Child::Tree(Cst {
                                            kind: TreeKind::TypeArg,
                                            span,
                                            ..
                                        })) => &self.text[span],
                                        Some(c) => {
                                            self.report(
                                                type_name.span(),
                                                format!("unexpected type parameter {c:?}"),
                                            );
                                            return None;
                                        }
                                        None => {
                                            self.report(type_name.span(), "missing type parameter");
                                            return None;
                                        }
                                    };

                                    if let Some(arg) = args.next() {
                                        self.report(
                                            Span::new(arg.span().start, tree.span.end - 1),
                                            "too many type parameters",
                                        );
                                        return None;
                                    }

                                    drop(args);
                                    match_token!(type_children.next()?, TokenKind::RBrack)?;
                                }
                                type_name => {
                                    self.report(tree.span, format!("unknown type '{type_name}'"));
                                }
                            }
                        }
                        _ => {}
                    }
                    Some(())
                });

                Visit::Continue
            }
        }

        let mut visitor = TypeChecker {
            diagnostics: Vec::new(),
            lines: &self.line_numbers,
            text: &self.text,
        };
        self.cst.walk(&mut visitor);

        for err in &self.parse_errors {
            visitor.report(
                err.span,
                match err.kind {
                    ErrorKind::Expected { expected, got } => {
                        format!("Missing {expected} for {} - got {got}", err.tree)
                    }
                    ErrorKind::Unexpected { token } => {
                        format!("Unexpected {token}, expected {}", err.tree)
                    }
                    kind => format!("{kind:#?}"),
                },
            );
        }

        let params = PublishDiagnosticsParams {
            uri: self.uri.clone(),
            diagnostics: visitor.diagnostics,
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
