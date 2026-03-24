use anyhow::{Result, anyhow};
use itertools::Itertools;
use lsp_server::Request as ServerRequest;
use lsp_types::notification::Notification as _;
use lsp_types::request::Request as _;
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionResponse, DocumentFormattingParams, Hover,
    HoverContents, MarkedString, Position, Range, SemanticTokensParams, SemanticTokensRangeParams,
    TextEdit,
    request::{
        Completion, Formatting, GotoDefinition, HoverRequest, SemanticTokensFullRequest,
        SemanticTokensRangeRequest,
    },
};
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
        ext::HoverParams,
        semantic_tokens::{
            self,
            builder::{SemanticTokensBuilder, type_index},
        },
    },
    server::Server,
    worker,
};

pub async fn request(server: &Server, req: &ServerRequest) -> Result<()> {
    // tracing::debug!(?req, "handle_request");
    let id = req.id.clone();
    let (uri, msg) = match req.method.as_str() {
        // GotoDefinition::METHOD => {
        //     server.send_ok(
        //         req.id.clone(),
        //         &lsp_types::GotoDefinitionResponse::Array(Vec::new()),
        //     )?;
        // }
        // Completion::METHOD => {
        // let item = CompletionItem {
        //     label: "HelloFromLSP".into(),
        //     kind: Some(CompletionItemKind::FUNCTION),
        //     detail: Some("dummy completion".into()),
        //     ..Default::default()
        // };
        // server.send_ok(req.id.clone(), &CompletionResponse::Array(vec![item]))?;
        // }
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
    };

    let workers = server.workers.read().await;
    match workers.get(&uri) {
        Some(worker) => {
            let _ = worker.tx.send(msg).await;
        }
        None => {
            server.send_err(
                req.id.clone(),
                lsp_server::ErrorCode::InvalidRequest,
                "cannot execute on document without worker!",
            )?;
        }
    }
    Ok(())
}
