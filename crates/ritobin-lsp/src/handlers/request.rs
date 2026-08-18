mod bin;

use std::{fmt::Write as _, str::FromStr, sync::Arc};

use anyhow::Result;
use lsp_server::Request as ServerRequest;
use lsp_types::{CompletionItem, CompletionParams, request::ResolveCompletionItem};
use lsp_types::{
    DocumentFormattingParams, SemanticTokensDeltaParams, SemanticTokensParams,
    SemanticTokensRangeParams,
    request::{
        Completion, Formatting, HoverRequest, SemanticTokensFullDeltaRequest,
        SemanticTokensFullRequest, SemanticTokensRangeRequest,
    },
};
use lsp_types::{MarkupContent, request::Request};
use meta_wiki::{client::types::GetDocsNameOrHash, schema::U32Hash};

use crate::{
    lsp::ext::{DeserializeBin, HoverParams, SerializeBin, Unhash, UnhashParams},
    server::Server,
    wiki,
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
            ResolveCompletionItem::METHOD => {
                let mut item: CompletionItem = serde_json::from_value(req.params)?;
                let server = server.clone();
                tokio::spawn(async move {
                    tracing::info!(?item);
                    match item
                        .data
                        .as_ref()
                        .and_then(|value| value.as_str())
                        .and_then(|v| GetDocsNameOrHash::try_from(v).ok())
                    {
                        Some(class_hash) => {
                            let docs = match wiki::fetch_class_docs(&server.wiki, &class_hash).await
                            {
                                Ok(docs) => {
                                    let mut str = format!(
                                        "## [{}](https://meta-wiki.leaguetoolkit.dev/classes/{})\n",
                                        docs.name, docs.name
                                    );
                                    writeln!(
                                        str,
                                        "{}",
                                        wiki::describe(docs.properties.get(&item.label))
                                    )
                                    .unwrap();
                                    str
                                }
                                Err(msg) => msg,
                            };

                            item.documentation
                                .replace(lsp_types::Documentation::MarkupContent(MarkupContent {
                                    kind: lsp_types::MarkupKind::Markdown,
                                    value: docs,
                                }));

                            let _ = server.send_ok(id, &item);
                        }
                        None => {
                            let _ = server.send_err(
                                id.clone(),
                                lsp_server::ErrorCode::RequestFailed,
                                "An internal error occured trying to resolve completion information",
                            );
                        }
                    }
                });

                return Ok(());
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
