use std::sync::Arc;

use anyhow::Result;
use lsp_types::notification::Notification as _;
use lsp_types::request::Request as _;
use lsp_types::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams, Url,
    notification::{DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument},
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
        DidCloseTextDocument::METHOD => {
            let p: DidCloseTextDocumentParams = serde_json::from_value(note.params.clone())?;
            close_document(server, p.text_document.uri).await;
        }
        _ => {}
    }
    Ok(())
}

/// Drops the document's worker and clears the diagnostics it published.
async fn close_document(server: &Arc<Server>, uri: Url) {
    let Some(worker) = server.workers.write().await.remove(&uri) else {
        return;
    };

    worker.shutdown();

    if let Err(e) = server.publish_diagnostics(uri, Vec::new()) {
        tracing::error!("failed to clear diagnostics on close: {e:?}");
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use lsp_types::{
        ClientCapabilities, PublishDiagnosticsParams, TextDocumentIdentifier, TextDocumentItem,
        notification::PublishDiagnostics,
    };
    use paths::AbsPathBuf;

    use super::*;
    use crate::{config::Config, worker::Worker};

    const SRC: &str = "type: string = \"PROP\"\n";

    fn server() -> (Arc<Server>, lsp_server::Connection) {
        let (ours, theirs) = lsp_server::Connection::memory();
        let root = AbsPathBuf::assert_utf8(std::env::current_dir().unwrap());
        let config = Config::new(root, ClientCapabilities::default(), vec![], None, None);

        (Arc::new(Server::new(ours, config, None)), theirs)
    }

    fn note<N: lsp_types::notification::Notification>(
        params: N::Params,
    ) -> lsp_server::Notification {
        lsp_server::Notification::new(N::METHOD.to_owned(), params)
    }

    /// The diagnostic sets published to the client, in order.
    fn published(client: &lsp_server::Connection) -> Vec<PublishDiagnosticsParams> {
        std::iter::from_fn(|| client.receiver.try_recv().ok())
            .filter_map(|msg| match msg {
                lsp_server::Message::Notification(n) if n.method == PublishDiagnostics::METHOD => {
                    serde_json::from_value(n.params).ok()
                }
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn closing_a_document_drops_its_worker() {
        let (server, client) = server();
        let uri = Url::parse("file:///t.rito").unwrap();

        let open = note::<DidOpenTextDocument>(DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(uri.clone(), "rito".into(), 0, SRC.into()),
        });
        notification(&server, &open).await.unwrap();
        assert!(server.workers.read().await.contains_key(&uri));

        let close = note::<DidCloseTextDocument>(DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier::new(uri.clone()),
        });
        notification(&server, &close).await.unwrap();

        // the worker owns the text, the tree and the token cache - holding it holds all of them
        assert!(
            !server.workers.read().await.contains_key(&uri),
            "worker outlived its document"
        );
        assert_eq!(
            published(&client).last().map(|p| p.diagnostics.len()),
            Some(0),
            "a closed document keeps whatever we published last, forever"
        );
    }

    #[tokio::test]
    async fn closing_a_document_we_never_opened_is_a_no_op() {
        let (server, _client) = server();
        let close = note::<DidCloseTextDocument>(DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier::new(Url::parse("file:///gone.rito").unwrap()),
        });

        notification(&server, &close).await.unwrap();
    }

    #[tokio::test]
    async fn a_worker_ends_when_its_handle_is_dropped() {
        let (server, _client) = server();
        let uri = Url::parse("file:///t.rito").unwrap();

        // nothing joins the task in production, so it has to notice the closed channel itself
        let task = Worker::spawn(server, uri, 0, SRC.to_owned()).shutdown();

        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .expect("worker task outlived its handle")
            .unwrap();
    }
}
