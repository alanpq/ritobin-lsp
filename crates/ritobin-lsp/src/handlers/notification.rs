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

use crate::{document::Document, server::Server};

pub fn notification(server: &Server, note: &lsp_server::Notification) -> Result<()> {
    tracing::debug!(?note, "handle_notification");
    match note.method.as_str() {
        DidOpenTextDocument::METHOD => {
            let p: DidOpenTextDocumentParams = serde_json::from_value(note.params.clone())?;
            let uri = p.text_document.uri;
            let doc = Document::new(uri.clone(), p.text_document.text);
            doc.publish_parse_errors(&server.conn)?;
            let mut docs = server.docs.write().unwrap();
            docs.insert(uri.clone(), doc);
        }
        DidChangeTextDocument::METHOD => {
            let p: DidChangeTextDocumentParams = serde_json::from_value(note.params.clone())?;
            if let Some(change) = p.content_changes.into_iter().next() {
                let uri = p.text_document.uri;
                let doc = Document::new(uri.clone(), change.text);
                doc.publish_parse_errors(&server.conn)?;
                let mut docs = server.docs.write().unwrap();
                docs.insert(uri.clone(), doc);
            }
        }
        _ => {}
    }
    Ok(())
}
