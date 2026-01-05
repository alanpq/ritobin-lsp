//! ### Minimal manual session (all nine packets)
//! ```no_run
//! # 1. initialize - server replies with capabilities
//! Content-Length: 85

//! {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}
//!
//! # 2. initialized - no response expected
//! Content-Length: 59

//! {"jsonrpc":"2.0","method":"initialized","params":{}}
//!
//! # 3. didOpen - provide initial buffer text
//! Content-Length: 173

//! {"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///tmp/foo.rs","languageId":"rust","version":1,"text":"fn  main( ){println!(\"hi\") }"}}}
//!
//! # 4. completion - expect HelloFromLSP
//! Content-Length: 139

//! {"jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{"textDocument":{"uri":"file:///tmp/foo.rs"},"position":{"line":0,"character":0}}}
//!
//! # 5. hover - expect markdown greeting
//! Content-Length: 135

//! {"jsonrpc":"2.0","id":3,"method":"textDocument/hover","params":{"textDocument":{"uri":"file:///tmp/foo.rs"},"position":{"line":0,"character":0}}}
//!
//! # 6. goto-definition - dummy empty array
//! Content-Length: 139

//! {"jsonrpc":"2.0","id":4,"method":"textDocument/definition","params":{"textDocument":{"uri":"file:///tmp/foo.rs"},"position":{"line":0,"character":0}}}
//!
//! # 7. formatting - rustfmt full document
//! Content-Length: 157

//! {"jsonrpc":"2.0","id":5,"method":"textDocument/formatting","params":{"textDocument":{"uri":"file:///tmp/foo.rs"},"options":{"tabSize":4,"insertSpaces":true}}}
//!
//! # 8. shutdown request - server acks and prepares to exit
//! Content-Length: 67

//! {"jsonrpc":"2.0","id":6,"method":"shutdown","params":null}
//!
//! # 9. exit notification - terminates the server
//! Content-Length: 54

//! {"jsonrpc":"2.0","method":"exit","params":null}
//! ```
//!

use std::{error::Error, io::Write};

use rustc_hash::FxHashMap; // fast hash map
use std::process::Stdio;

#[allow(
    clippy::print_stderr,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]
use anyhow::{Context, Result, anyhow, bail};
use lsp_server::{Connection, Message, Request as ServerRequest, RequestId, Response};
use lsp_types::notification::Notification as _; // for METHOD consts
use lsp_types::request::Request as _;
use lsp_types::{
    CompletionItem,
    CompletionItemKind,
    // capability helpers
    CompletionOptions,
    CompletionResponse,
    Diagnostic,
    DiagnosticSeverity,
    DidChangeTextDocumentParams,
    DidOpenTextDocumentParams,
    DocumentFormattingParams,
    Hover,
    HoverContents,
    HoverProviderCapability,
    // core
    InitializeParams,
    MarkedString,
    OneOf,
    Position,
    PublishDiagnosticsParams,
    Range,
    ServerCapabilities,
    TextDocumentSyncCapability,
    TextDocumentSyncKind,
    TextEdit,
    Uri,
    // notifications
    notification::{DidChangeTextDocument, DidOpenTextDocument, PublishDiagnostics},
    // requests
    request::{Completion, Formatting, GotoDefinition, HoverRequest},
}; // for METHOD consts

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    LspServer {},
}

impl Default for Commands {
    fn default() -> Self {
        Self::LspServer {}
    }
}

#[allow(clippy::print_stderr)]
fn main() -> std::result::Result<(), Box<dyn Error + Sync + Send>> {
    env_logger::init();
    log::error!("starting minimal_lsp");

    let mut cli = Cli::parse();
    let subcommand = cli.command.take().unwrap_or_default();

    // transport
    let (connection, io_thread) = Connection::stdio();

    // advertised capabilities
    let caps = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        completion_provider: Some(CompletionOptions::default()),
        definition_provider: Some(OneOf::Left(true)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        document_formatting_provider: Some(OneOf::Left(true)),
        ..Default::default()
    };
    let init_value = serde_json::json!({
        "capabilities": caps,
        "offsetEncoding": ["utf-8"],
    });

    let init_params = connection.initialize(init_value)?;
    main_loop(connection, init_params)?;
    io_thread.join()?;
    log::error!("shutting down server");
    Ok(())
}

fn main_loop(
    connection: Connection,
    params: serde_json::Value,
) -> std::result::Result<(), Box<dyn Error + Sync + Send>> {
    let _init: InitializeParams = serde_json::from_value(params)?;
    let mut docs: FxHashMap<Uri, String> = FxHashMap::default();

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    break;
                }
                if let Err(err) = handle_request(&connection, &req, &mut docs) {
                    log::error!("[lsp] request {} failed: {err}", &req.method);
                }
            }
            Message::Notification(note) => {
                if let Err(err) = handle_notification(&connection, &note, &mut docs) {
                    log::error!("[lsp] notification {} failed: {err}", note.method);
                }
            }
            Message::Response(resp) => log::error!("[lsp] response: {resp:?}"),
        }
    }
    Ok(())
}

fn handle_notification(
    conn: &Connection,
    note: &lsp_server::Notification,
    docs: &mut FxHashMap<Uri, String>,
) -> Result<()> {
    match note.method.as_str() {
        DidOpenTextDocument::METHOD => {
            let p: DidOpenTextDocumentParams = serde_json::from_value(note.params.clone())?;
            let uri = p.text_document.uri;
            docs.insert(uri.clone(), p.text_document.text);
            publish_dummy_diag(conn, &uri)?;
        }
        DidChangeTextDocument::METHOD => {
            let p: DidChangeTextDocumentParams = serde_json::from_value(note.params.clone())?;
            if let Some(change) = p.content_changes.into_iter().next() {
                let uri = p.text_document.uri;
                docs.insert(uri.clone(), change.text);
                publish_dummy_diag(conn, &uri)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn handle_request(
    conn: &Connection,
    req: &ServerRequest,
    docs: &mut FxHashMap<Uri, String>,
) -> Result<()> {
    match req.method.as_str() {
        GotoDefinition::METHOD => {
            send_ok(
                conn,
                req.id.clone(),
                &lsp_types::GotoDefinitionResponse::Array(Vec::new()),
            )?;
        }
        Completion::METHOD => {
            let item = CompletionItem {
                label: "HelloFromLSP".into(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some("dummy completion".into()),
                ..Default::default()
            };
            send_ok(conn, req.id.clone(), &CompletionResponse::Array(vec![item]))?;
        }
        HoverRequest::METHOD => {
            let hover = Hover {
                contents: HoverContents::Scalar(MarkedString::String(
                    "Hello from *minimal_lsp*".into(),
                )),
                range: None,
            };
            send_ok(conn, req.id.clone(), &hover)?;
        }
        Formatting::METHOD => {
            let p: DocumentFormattingParams = serde_json::from_value(req.params.clone())?;
            let uri = p.text_document.uri;
            let text = docs
                .get(&uri)
                .ok_or_else(|| anyhow!("document not in cache – did you send DidOpen?"))?;
            // let formatted = run_rustfmt(text)?;
            let formatted = text.clone();
            let edit = TextEdit {
                range: full_range(text),
                new_text: formatted,
            };
            send_ok(conn, req.id.clone(), &vec![edit])?;
        }
        _ => send_err(
            conn,
            req.id.clone(),
            lsp_server::ErrorCode::MethodNotFound,
            "unhandled method",
        )?,
    }
    Ok(())
}

fn publish_dummy_diag(conn: &Connection, uri: &Uri) -> Result<()> {
    let diag = Diagnostic {
        range: Range::new(Position::new(0, 0), Position::new(0, 1)),
        severity: Some(DiagnosticSeverity::INFORMATION),
        code: None,
        code_description: None,
        source: Some("minimal_lsp".into()),
        message: "dummy diagnostic".into(),
        related_information: None,
        tags: None,
        data: None,
    };
    let params = PublishDiagnosticsParams {
        uri: uri.clone(),
        diagnostics: vec![diag],
        version: None,
    };
    conn.sender
        .send(Message::Notification(lsp_server::Notification::new(
            PublishDiagnostics::METHOD.to_owned(),
            params,
        )))?;
    Ok(())
}

fn full_range(text: &str) -> Range {
    let last_line_idx = text.lines().count().saturating_sub(1) as u32;
    let last_col = text.lines().last().map_or(0, |l| l.chars().count()) as u32;
    Range::new(Position::new(0, 0), Position::new(last_line_idx, last_col))
}

fn send_ok<T: serde::Serialize>(conn: &Connection, id: RequestId, result: &T) -> Result<()> {
    let resp = Response {
        id,
        result: Some(serde_json::to_value(result)?),
        error: None,
    };
    conn.sender.send(Message::Response(resp))?;
    Ok(())
}

fn send_err(
    conn: &Connection,
    id: RequestId,
    code: lsp_server::ErrorCode,
    msg: &str,
) -> Result<()> {
    let resp = Response {
        id,
        result: None,
        error: Some(lsp_server::ResponseError {
            code: code as i32,
            message: msg.into(),
            data: None,
        }),
    };
    conn.sender.send(Message::Response(resp))?;
    Ok(())
}
