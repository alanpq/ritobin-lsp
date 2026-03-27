use anyhow::{Result, anyhow};
use itertools::Itertools;
use lsp_server::Request as ServerRequest;
use lsp_types::request::Request;
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionResponse, DocumentFormattingParams, Hover,
    HoverContents, MarkedString, Position, Range, SemanticTokensParams, SemanticTokensRangeParams,
    TextEdit,
    request::{
        Completion, Formatting, GotoDefinition, HoverRequest, SemanticTokensFullRequest,
        SemanticTokensRangeRequest,
    },
};
use lsp_types::{CompletionParams, notification::Notification as _};
use ltk_ritobin::{
    cst::{
        Cst, TreeKind, Visitor,
        visitor::{Visit, VisitorExt},
    },
    parse::{Span, Token, TokenKind},
    print::PrintConfig,
};
use ritobin_lsp::{cst_ext::CstExt, line_ends::LineNumbers};

use crate::{
    lsp::{
        ext::{HoverParams, Unhash, UnhashParams},
        semantic_tokens::{
            self,
            builder::{SemanticTokensBuilder, type_index},
        },
    },
    server::Server,
    worker::{self, CompletionRequest},
};

pub async fn request(server: &Server, req: ServerRequest) -> Result<()> {
    // tracing::debug!(?req, "handle_request");
    let id = req.id.clone();
    let (uri, msg) = {
        match req.method.as_str() {
            // GotoDefinition::METHOD => {
            //     server.send_ok(
            //         req.id.clone(),
            //         &lsp_types::GotoDefinitionResponse::Array(Vec::new()),
            //     )?;
            // }
            Unhash::METHOD => {
                let p: UnhashParams = serde_json::from_value(req.params)?;
                (
                    p.text_document.uri,
                    worker::Message::UnhashRequest { id, range: p.range },
                )
            }
            Completion::METHOD => {
                let p: CompletionParams = serde_json::from_value(req.params)?;
                (
                    p.text_document_position.text_document.uri,
                    worker::Message::CompletionRequest(CompletionRequest {
                        id,
                        context: p.context,
                        position: p.text_document_position.position,
                        work_done_progress_params: p.work_done_progress_params,
                        partial_result_params: p.partial_result_params,
                    }),
                )
            }
            HoverRequest::METHOD => {
                let p: HoverParams = serde_json::from_value(req.params.clone())?;

                (
                    p.text_document.uri,
                    worker::Message::HoverRequest {
                        id,
                        position: p.position,
                        work_done_progress_params: p.work_done_progress_params,
                    },
                )
            }
            Formatting::METHOD => {
                let p: DocumentFormattingParams = serde_json::from_value(req.params.clone())?;
                (
                    p.text_document.uri.clone(),
                    worker::Message::FormatRequest {
                        id,
                        options: p.options,
                        work_done_progress_params: p.work_done_progress_params,
                    },
                )
            }
            SemanticTokensRangeRequest::METHOD => {
                let p: SemanticTokensRangeParams = serde_json::from_value(req.params.clone())?;
                (
                    p.text_document.uri.clone(),
                    worker::Message::SemanticTokens {
                        id,
                        work_done_progress_params: p.work_done_progress_params,
                        partial_result_params: p.partial_result_params,
                        range: Some(p.range),
                    },
                )
            }
            SemanticTokensFullRequest::METHOD => {
                let p: SemanticTokensParams = serde_json::from_value(req.params.clone())?;
                (
                    p.text_document.uri.clone(),
                    worker::Message::SemanticTokens {
                        id,
                        work_done_progress_params: p.work_done_progress_params,
                        partial_result_params: p.partial_result_params,
                        range: None,
                    },
                )
            }
            _ => {
                server.send_err(
                    req.id.clone(),
                    lsp_server::ErrorCode::MethodNotFound,
                    "unhandled method",
                )?;
                return Ok(());
            }
        }
    };

    let workers = server.workers.read().await;
    match workers.get(&uri) {
        Some(worker) => {
            let _ = worker.tx.send(msg).await;
        }
        None => {
            server.send_err(
                req.id,
                lsp_server::ErrorCode::InvalidRequest,
                "cannot execute on document without worker!",
            )?;
        }
    }
    Ok(())
}
