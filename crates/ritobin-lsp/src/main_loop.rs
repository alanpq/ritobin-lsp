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
use std::{process::Stdio, sync::Arc};
use tracing_subscriber::{
    Layer as _, Registry,
    filter::Targets,
    fmt::{time, writer::BoxMakeWriter},
    layer::SubscriberExt as _,
};

use crate::{
    config::Config,
    document::Document,
    handlers,
    lsp::{
        self,
        ext::{ServerStatusNotification, ServerStatusParams},
    },
    server::Server,
};

pub fn main_loop(config: Config, connection: Connection) -> anyhow::Result<()> {
    let server = Arc::new(Server::new(connection, config));

    let not = lsp_server::Notification::new(
        ServerStatusNotification::METHOD.to_owned(),
        ServerStatusParams {
            health: lsp::ext::Health::Ok,
            quiescent: true,
            message: None,
        },
    );
    server
        .conn
        .sender
        .send(lsp_server::Message::Notification(not))?;

    for msg in &server.conn.receiver {
        tracing::info!("MSG");
        match msg {
            Message::Request(req) => {
                if server.conn.handle_shutdown(&req)? {
                    break;
                }
                let server = server.clone();
                std::thread::spawn(move || {
                    if let Err(err) = handlers::request(&server, &req) {
                        tracing::error!("[lsp] request {} failed: {err}", &req.method);
                    }
                });
            }
            Message::Notification(note) => {
                let server = server.clone();
                std::thread::spawn(move || {
                    if let Err(err) = handlers::notification(&server, &note) {
                        tracing::error!("[lsp] notification {} failed: {err}", note.method);
                    }
                });
            }
            Message::Response(resp) => tracing::error!("[lsp] response: {resp:?}"),
        }
    }
    Ok(())
}
