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

use crate::{
    document::Document,
    lsp::{
        ext::HoverParams,
        semantic_tokens::{
            self,
            builder::{SemanticTokensBuilder, type_index},
        },
    },
    server::Server,
};

pub fn request(server: &Server, req: &ServerRequest) -> Result<()> {
    tracing::debug!(?req, "handle_request");
    match req.method.as_str() {
        GotoDefinition::METHOD => {
            server.send_ok(
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
            server.send_ok(req.id.clone(), &CompletionResponse::Array(vec![item]))?;
        }
        HoverRequest::METHOD => {
            let p: HoverParams = serde_json::from_value(req.params.clone())?;
            let pos = p.position.start();

            let docs = server.docs.read().unwrap();
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
            server.send_ok(req.id.clone(), &hover)?;
        }
        Formatting::METHOD => {
            let p: DocumentFormattingParams = serde_json::from_value(req.params.clone())?;
            let uri = p.text_document.uri;
            let docs = server.docs.read().unwrap();
            let doc = docs
                .get(&uri)
                .ok_or_else(|| anyhow!("document not in cache – did you send DidOpen?"))?;
            // let formatted = run_rustfmt(text)?;
            let formatted = doc.text.clone();
            let edit = TextEdit {
                // range: full_range(&doc.text),
                range: todo!(),
                new_text: formatted,
            };
            server.send_ok(req.id.clone(), &vec![edit])?;
        }
        SemanticTokensFullRequest::METHOD => {
            let p: SemanticTokensParams = serde_json::from_value(req.params.clone())?;
            let builder = SemanticTokensBuilder::new(p.text_document.uri.to_string());
            let docs = server.docs.read().unwrap();
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
                fn enter_tree(&mut self, tree: &Cst) -> Visit {
                    if matches!(tree.kind, TreeKind::ErrorTree) {
                        return Visit::Continue;
                    }
                    self.stack.push(tree.kind);
                    Visit::Continue
                }

                fn exit_tree(&mut self, tree: &Cst) -> Visit {
                    if matches!(tree.kind, TreeKind::ErrorTree) {
                        return Visit::Continue;
                    }
                    self.stack.pop();
                    Visit::Continue
                }
                fn visit_token(&mut self, token: &Token, _context: &Cst) -> Visit {
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
                        (_, TokenKind::Number) | (_, TokenKind::HexLit) => {
                            semantic_tokens::types::NUMBER
                        }
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
            server.send_ok(req.id.clone(), &tokens)?;
        }
        _ => server.send_err(
            req.id.clone(),
            lsp_server::ErrorCode::MethodNotFound,
            "unhandled method",
        )?,
    }
    Ok(())
}
