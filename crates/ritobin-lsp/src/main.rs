use std::{env, error::Error, fmt::format, fs, io::Write, path::PathBuf, sync::Arc};

use crossbeam_channel::Sender;
use itertools::Itertools;
use ltk_ritobin::{
    Span,
    parser::{
        real::{Child, ErrorKind, FlatErrors, ParseError, Tree, TreeKind, Visit, Visitor, parse},
        tokenizer::{Token, TokenKind},
    },
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
}; // fast hash map

#[allow(
    clippy::print_stderr,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]
use anyhow::{Context, Result, anyhow, bail};
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
use lsp_types::{WorkDoneProgressOptions, notification::Notification as _}; // for METHOD consts // for METHOD consts

use clap::{Parser, Subcommand};

use crate::{
    config::{Config, ConfigChange, ConfigErrors},
    lsp::{
        capabilities::server_capabilities,
        ext::{HoverParams, ServerStatusNotification, ServerStatusParams},
        semantic_tokens::{
            self,
            builder::{SemanticTokensBuilder, type_index},
        },
    },
};

pub mod config;
pub mod lsp;
pub mod main_loop;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[arg(long)]
    pub log_file: Option<PathBuf>,

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

fn setup_logging(log_file_flag: Option<PathBuf>) -> anyhow::Result<()> {
    if cfg!(windows) {
        // This is required so that windows finds our pdb that is placed right beside the exe.
        // By default it doesn't look at the folder the exe resides in, only in the current working
        // directory which we set to the project workspace.
        // https://docs.microsoft.com/en-us/windows-hardware/drivers/debugger/general-environment-variables
        // https://docs.microsoft.com/en-us/windows/win32/api/dbghelp/nf-dbghelp-syminitialize
        if let Ok(path) = env::current_exe()
            && let Some(path) = path.parent()
        {
            // SAFETY: This is safe because this is single-threaded.
            unsafe {
                env::set_var("_NT_SYMBOL_PATH", path);
            }
        }
    }

    if env::var("RUST_BACKTRACE").is_err() {
        // SAFETY: This is safe because this is single-threaded.
        unsafe {
            env::set_var("RUST_BACKTRACE", "short");
        }
    }

    let log_file = env::var("RB_LOG_FILE")
        .ok()
        .map(PathBuf::from)
        .or(log_file_flag);
    let log_file = match log_file {
        Some(path) => {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            Some(
                fs::File::create(&path)
                    .with_context(|| format!("can't create log file at {}", path.display()))?,
            )
        }
        None => None,
    };

    let writer = match log_file {
        Some(file) => BoxMakeWriter::new(Arc::new(file)),
        None => BoxMakeWriter::new(std::io::stderr),
    };

    // Deliberately enable all `warn` logs if the user has not set RB_LOG, as there is usually
    // useful information in there for debugging.
    let targets_filter = env::var("RB_LOG").ok().unwrap_or_else(|| "warn".to_owned());
    let targets_filter: Targets = targets_filter
        .parse()
        .with_context(|| format!("invalid log filter: `{}`", targets_filter))?;

    let rb_fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_ansi(false)
        .with_writer(writer);

    let rb_fmt_layer = match time::OffsetTime::local_rfc_3339() {
        Ok(timer) => {
            // If we can get the time offset, format logs with the timezone.
            rb_fmt_layer.with_timer(timer).boxed()
        }
        Err(_) => {
            // Use system time if we can't get the time offset. This should
            // never happen on Linux, but can happen on e.g. OpenBSD.
            rb_fmt_layer.boxed()
        }
    }
    .with_filter(targets_filter);

    let subscriber = Registry::default().with(rb_fmt_layer);

    tracing::subscriber::set_global_default(subscriber)?;

    // crate::tracing::Config {
    //     writer,
    //     chalk_filter: env::var("CHALK_DEBUG").ok(),
    //     profile_filter: env::var("RB_PROFILE").ok(),
    //     json_profile_filter: std::env::var("RB_PROFILE_JSON").ok(),
    // }
    // .init()?;

    Ok(())
}

#[allow(clippy::print_stderr)]
fn main() -> std::result::Result<(), Box<dyn Error + Sync + Send>> {
    let mut cli = Cli::parse();
    if let Err(e) = setup_logging(cli.log_file.clone()) {
        eprintln!("Failed to setup logging: {e:#}");
    }
    tracing::error!("starting minimal_lsp");
    tracing::debug!("test");

    let subcommand = cli.command.take().unwrap_or_default();

    // transport
    let (connection, io_threads) = Connection::stdio();

    let (initialize_id, initialize_params) = match connection.initialize_start() {
        Ok(it) => it,
        Err(e) => {
            if e.channel_is_disconnected() {
                io_threads.join()?;
            }
            return Err(e.into());
        }
    };

    tracing::info!("InitializeParams: {}", initialize_params);
    let lsp_types::InitializeParams {
        root_uri,
        mut capabilities,
        workspace_folders,
        initialization_options,
        client_info,
        ..
    } = from_json::<lsp_types::InitializeParams>("InitializeParams", &initialize_params)?;

    // lsp-types has a typo in the `/capabilities/workspace/diagnostics` field, its typoed as `diagnostic`
    if let Some(val) = initialize_params.pointer("/capabilities/workspace/diagnostics")
        && let Ok(diag_caps) = from_json::<lsp_types::DiagnosticWorkspaceClientCapabilities>(
            "DiagnosticWorkspaceClientCapabilities",
            val,
        )
    {
        tracing::info!("Patching lsp-types workspace diagnostics capabilities: {diag_caps:#?}");
        capabilities
            .workspace
            .get_or_insert_default()
            .diagnostic
            .get_or_insert(diag_caps);
    }

    let root_path = match root_uri
        .and_then(|it| it.to_file_path().ok())
        .map(patch_path_prefix)
        .and_then(|it| Utf8PathBuf::from_path_buf(it).ok())
        .and_then(|it| AbsPathBuf::try_from(it).ok())
    {
        Some(it) => it,
        None => {
            let cwd = env::current_dir()?;
            AbsPathBuf::assert_utf8(cwd)
        }
    };

    if let Some(client_info) = &client_info {
        tracing::info!(
            "Client '{}' {}",
            client_info.name,
            client_info.version.as_deref().unwrap_or_default()
        );
    }

    let workspace_roots = workspace_folders
        .map(|workspaces| {
            workspaces
                .into_iter()
                .filter_map(|it| it.uri.to_file_path().ok())
                .map(patch_path_prefix)
                .filter_map(|it| Utf8PathBuf::from_path_buf(it).ok())
                .filter_map(|it| AbsPathBuf::try_from(it).ok())
                .collect::<Vec<_>>()
        })
        .filter(|workspaces| !workspaces.is_empty())
        .unwrap_or_else(|| vec![root_path.clone()]);

    let config = Config::new(root_path, capabilities, workspace_roots, client_info);

    // advertised capabilities
    let server_caps = server_capabilities(&config);

    let initialize_result = lsp_types::InitializeResult {
        capabilities: server_caps,
        server_info: Some(lsp_types::ServerInfo {
            name: String::from("ritobin-lsp"),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
        }),
        offset_encoding: None,
    };

    let initialize_result = serde_json::to_value(initialize_result).unwrap();

    if let Err(e) = connection.initialize_finish(initialize_id, initialize_result) {
        if e.channel_is_disconnected() {
            io_threads.join()?;
        }
        return Err(e.into());
    }
    main_loop(config, connection)?;
    io_threads.join()?;
    tracing::error!("shutting down server");
    Ok(())
}

fn patch_path_prefix(path: PathBuf) -> PathBuf {
    use std::path::{Component, Prefix};
    if cfg!(windows) {
        // VSCode might report paths with the file drive in lowercase, but this can mess
        // with env vars set by tools and build scripts executed by r-a such that it invalidates
        // cargo's compilations unnecessarily. https://github.com/rust-lang/rust-analyzer/issues/14683
        // So we just uppercase the drive letter here unconditionally.
        // (doing it conditionally is a pain because std::path::Prefix always reports uppercase letters on windows)
        let mut comps = path.components();
        match comps.next() {
            Some(Component::Prefix(prefix)) => {
                let prefix = match prefix.kind() {
                    Prefix::Disk(d) => {
                        format!("{}:", d.to_ascii_uppercase() as char)
                    }
                    Prefix::VerbatimDisk(d) => {
                        format!(r"\\?\{}:", d.to_ascii_uppercase() as char)
                    }
                    _ => return path,
                };
                let mut path = PathBuf::new();
                path.push(prefix);
                path.extend(comps);
                path
            }
            _ => path,
        }
    } else {
        path
    }
}

fn main_loop(config: Config, connection: Connection) -> anyhow::Result<()> {
    let mut docs: FxHashMap<Url, Document> = FxHashMap::default();

    let not = lsp_server::Notification::new(
        ServerStatusNotification::METHOD.to_owned(),
        ServerStatusParams {
            health: lsp::ext::Health::Ok,
            quiescent: true,
            message: None,
        },
    );
    connection
        .sender
        .send(lsp_server::Message::Notification(not))?;

    for msg in &connection.receiver {
        tracing::info!("MSG");
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    break;
                }
                if let Err(err) = handle_request(&connection, &req, &mut docs) {
                    tracing::error!("[lsp] request {} failed: {err}", &req.method);
                }
            }
            Message::Notification(note) => {
                if let Err(err) = handle_notification(&connection, &note, &mut docs) {
                    tracing::error!("[lsp] notification {} failed: {err}", note.method);
                }
            }
            Message::Response(resp) => tracing::error!("[lsp] response: {resp:?}"),
        }
    }
    Ok(())
}

fn handle_notification(
    conn: &Connection,
    note: &lsp_server::Notification,
    docs: &mut FxHashMap<Url, Document>,
) -> Result<()> {
    tracing::debug!(?note, "handle_notification");
    match note.method.as_str() {
        DidOpenTextDocument::METHOD => {
            let p: DidOpenTextDocumentParams = serde_json::from_value(note.params.clone())?;
            let uri = p.text_document.uri;
            let doc = Document::new(uri.clone(), p.text_document.text);
            doc.publish_parse_errors(conn)?;
            docs.insert(uri.clone(), doc);
        }
        DidChangeTextDocument::METHOD => {
            let p: DidChangeTextDocumentParams = serde_json::from_value(note.params.clone())?;
            if let Some(change) = p.content_changes.into_iter().next() {
                let uri = p.text_document.uri;
                let doc = Document::new(uri.clone(), change.text);
                doc.publish_parse_errors(conn)?;
                docs.insert(uri.clone(), doc);
            }
        }
        _ => {}
    }
    Ok(())
}

pub struct Document {
    pub uri: Url,
    pub text: String,
    pub cst: Tree,
    pub parse_errors: Vec<ParseError>,
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
            Child::Tree(tree @ Tree { kind: $kind, .. }) => Some(tree),
            _ => None,
        }
    }};
}
impl Document {
    pub fn new(uri: Url, text: String) -> Self {
        let cst = parse(&text);
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
            fn enter_tree(&mut self, tree: &Tree) -> Visit {
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
                                        Some(Child::Tree(Tree {
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

fn handle_request(
    conn: &Connection,
    req: &ServerRequest,
    docs: &mut FxHashMap<Url, Document>,
) -> Result<()> {
    tracing::debug!(?req, "handle_request");
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
            let p: HoverParams = serde_json::from_value(req.params.clone())?;
            let pos = p.position.start();

            let doc = docs
                .get(&p.text_document.uri)
                .ok_or_else(|| anyhow!("document not in cache – did you send DidOpen?"))?;

            let txt = match doc
                .cst
                .find_node(doc.line_numbers.byte_index(pos.line, pos.character + 1))
            {
                Some((node, tok)) => {
                    let txt = &doc.text[tok.span.start as _..tok.span.end as _];
                    format!("{txt:?} | {node:?} | {:?}", tok.kind)
                }
                None => "".into(),
            };

            let hover = Hover {
                contents: HoverContents::Scalar(MarkedString::String(txt)),
                range: None,
            };
            send_ok(conn, req.id.clone(), &hover)?;
        }
        Formatting::METHOD => {
            let p: DocumentFormattingParams = serde_json::from_value(req.params.clone())?;
            let uri = p.text_document.uri;
            let doc = docs
                .get(&uri)
                .ok_or_else(|| anyhow!("document not in cache – did you send DidOpen?"))?;
            // let formatted = run_rustfmt(text)?;
            let formatted = doc.text.clone();
            let edit = TextEdit {
                range: full_range(&doc.text),
                new_text: formatted,
            };
            send_ok(conn, req.id.clone(), &vec![edit])?;
        }
        SemanticTokensFullRequest::METHOD => {
            let p: SemanticTokensParams = serde_json::from_value(req.params.clone())?;
            let builder = SemanticTokensBuilder::new(p.text_document.uri.to_string());
            let doc = docs
                .get(&p.text_document.uri)
                .ok_or_else(|| anyhow!("document not in cache – did you send DidOpen?"))?;

            struct SemanticVisitor<'a> {
                text: &'a str,
                line_nums: &'a LineNumbers,
                builder: SemanticTokensBuilder,
                stack: Vec<TreeKind>,
            }

            impl Visitor for SemanticVisitor<'_> {
                fn enter_tree(&mut self, tree: &Tree) -> Visit {
                    if matches!(tree.kind, TreeKind::ErrorTree) {
                        return Visit::Continue;
                    }
                    self.stack.push(tree.kind);
                    Visit::Continue
                }

                fn exit_tree(&mut self, tree: &Tree) -> Visit {
                    if matches!(tree.kind, TreeKind::ErrorTree) {
                        return Visit::Continue;
                    }
                    self.stack.pop();
                    Visit::Continue
                }
                fn visit_token(&mut self, token: &Token, _context: &Tree) -> Visit {
                    let last_tree = self.stack.last().unwrap();
                    tracing::debug!(
                        "{:?} ({:?}) | last tree: {last_tree:?}",
                        token.kind,
                        &self.text[token.span.start as usize..token.span.end as usize],
                    );

                    let token_kind = match (last_tree, token.kind) {
                        (_, TokenKind::RCurly)
                        | (_, TokenKind::LCurly)
                        | (_, TokenKind::RBrack)
                        | (_, TokenKind::LBrack)
                        | (_, TokenKind::Colon) => semantic_tokens::types::PUNCTUATION,

                        (TreeKind::TypeExpr, _) => semantic_tokens::types::TYPE,
                        (TreeKind::TypeArg, _) | (TreeKind::TypeArgList, _) => {
                            semantic_tokens::types::TYPE_PARAMETER
                        }
                        (_, TokenKind::Name) => semantic_tokens::types::KEYWORD,
                        (_, TokenKind::Quote)
                        | (_, TokenKind::String)
                        | (_, TokenKind::UnterminatedString) => semantic_tokens::types::STRING,
                        (_, TokenKind::Int) => semantic_tokens::types::NUMBER,
                        _ => {
                            return Visit::Continue;
                        }
                    };
                    for (line, range) in self.line_nums.iter_span_lines(token.span) {
                        tracing::debug!(?line, ?range);
                        self.builder.push(
                            Range::new(
                                Position::new((line) as _, *range.start()),
                                Position::new((line) as _, *range.end()),
                            ),
                            type_index(&token_kind),
                            semantic_tokens::modifier_set::ModifierSet::default().0,
                        );
                    }
                    Visit::Continue
                }
            }

            let mut visitor = SemanticVisitor {
                text: &doc.text,
                line_nums: &doc.line_numbers,
                stack: Vec::new(),
                builder,
            };
            doc.cst.walk(&mut visitor);

            let tokens = visitor.builder.build();
            send_ok(conn, req.id.clone(), &tokens)?;
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
