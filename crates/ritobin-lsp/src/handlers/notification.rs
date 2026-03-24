use std::sync::Arc;

use anyhow::Result;
use lsp_types::notification::Notification as _;
use lsp_types::request::Request as _;
use lsp_types::{
    DidChangeTextDocumentParams, DidOpenTextDocumentParams,
    notification::{DidChangeTextDocument, DidOpenTextDocument},
};

use crate::{server::Server, worker::Worker};

pub async fn notification(server: &Arc<Server>, note: &lsp_server::Notification) -> Result<()> {
    // tracing::debug!(?note, "handle_notification");
    match note.method.as_str() {
        DidOpenTextDocument::METHOD => {
            let p: DidOpenTextDocumentParams = serde_json::from_value(note.params.clone())?;
            let uri = p.text_document.uri;
            server.workers.write().await.insert(
                uri.clone(),
                Worker::spawn(
                    server.clone(),
                    uri,
                    p.text_document.version,
                    p.text_document.text,
                ),
            );
            // let doc = Document::new(uri.clone(), p.text_document.text);
            // doc.publish_parse_errors(&server.conn)?;
            // let mut docs = server.docs.write().unwrap();
            // docs.insert(uri.clone(), doc);
        }
        DidChangeTextDocument::METHOD => {
            let p: DidChangeTextDocumentParams = serde_json::from_value(note.params.clone())?;
            let workers = server.workers.read().await;
            let Some(worker) = workers.get(&p.text_document.uri) else {
                tracing::error!("change text for document without worker!");
                return Ok(());
            };

            let _ = worker
                .tx
                .send(crate::worker::Message::DocumentChange {
                    version: p.text_document.version,
                    changes: p.content_changes,
                })
                .await;

            // if let Some(change) = p.content_changes.into_iter().next() {
            //     let uri = p.text_document.uri;
            //     let doc = Document::new(uri.clone(), change.text);
            //     doc.publish_parse_errors(&server.conn)?;
            //     let mut docs = server.docs.write().unwrap();
            //     docs.insert(uri.clone(), doc);
            // }
        }
        _ => {}
    }
    Ok(())
}
