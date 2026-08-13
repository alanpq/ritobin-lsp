mod bin;

use std::sync::Arc;

use anyhow::Result;
use lsp_server::Request as ServerRequest;
use lsp_types::CompletionParams;
use lsp_types::request::Request;
use lsp_types::{
    DocumentFormattingParams, SemanticTokensDeltaParams, SemanticTokensParams,
    SemanticTokensRangeParams,
    request::{
        Completion, Formatting, HoverRequest, SemanticTokensFullDeltaRequest,
        SemanticTokensFullRequest, SemanticTokensRangeRequest,
    },
};

use crate::{
    lsp::ext::{DeserializeBin, HoverParams, SerializeBin, Unhash, UnhashParams},
    server::Server,
    worker::{self, CompletionRequest},
};

pub async fn request(server: &Arc<Server>, req: ServerRequest) -> Result<()> {
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
            DeserializeBin::METHOD => {
                bin::handle_deserialize_bin(server, id, serde_json::from_value(req.params)?);
                return Ok(());
            }
            SerializeBin::METHOD => {
                bin::handle_serialize_bin(server, id, serde_json::from_value(req.params)?);
                return Ok(());
            }
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
                        previous_result_id: None,
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
                        previous_result_id: None,
                    },
                )
            }
            SemanticTokensFullDeltaRequest::METHOD => {
                let p: SemanticTokensDeltaParams = serde_json::from_value(req.params.clone())?;
                (
                    p.text_document.uri.clone(),
                    worker::Message::SemanticTokens {
                        id,
                        work_done_progress_params: p.work_done_progress_params,
                        partial_result_params: p.partial_result_params,
                        range: None,
                        previous_result_id: Some(p.previous_result_id),
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
            if worker.tx.send(msg).await.is_err() {
                server.send_err(
                    req.id,
                    lsp_server::ErrorCode::InternalError,
                    "document worker is no longer running",
                )?;
            }
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
