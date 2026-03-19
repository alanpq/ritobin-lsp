use anyhow::Result;
use lsp_types::{
    DidChangeTextDocumentParams, DidOpenTextDocumentParams,
    notification::{DidChangeTextDocument, DidOpenTextDocument},
};
use lsp_types::request::Request as _;
use lsp_types::notification::Notification as _;

use crate::{document::Document, server::Server};

pub fn notification(server: &Server, note: &lsp_server::Notification) -> Result<()> {
    // tracing::debug!(?note, "handle_notification");
    match note.method.as_str() {
        DidOpenTextDocument::METHOD => {
            let p: DidOpenTextDocumentParams = serde_json::from_value(note.params.clone())?;
            let uri = p.text_document.uri;
            let doc = Document::new(uri.clone(), p.text_document.text);
            doc.publish_parse_errors(&server.conn)?;
            let mut docs = server.docs.write().unwrap();
            docs.insert(uri.clone(), doc);
        }
        DidChangeTextDocument::METHOD => {
            let p: DidChangeTextDocumentParams = serde_json::from_value(note.params.clone())?;
            if let Some(change) = p.content_changes.into_iter().next() {
                let uri = p.text_document.uri;
                let doc = Document::new(uri.clone(), change.text);
                doc.publish_parse_errors(&server.conn)?;
                let mut docs = server.docs.write().unwrap();
                docs.insert(uri.clone(), doc);
            }
        }
        _ => {}
    }
    Ok(())
}
