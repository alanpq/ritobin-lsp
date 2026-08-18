use std::sync::Arc;

use meta_wiki::{
    client::types::{ClassDocs, DocEntry, GetDocsNameOrHash},
    docs_cache::WikiDocs,
};
use reqwest::StatusCode;

pub async fn fetch_class_docs(
    wiki: &WikiDocs,
    key: &GetDocsNameOrHash,
) -> Result<Arc<ClassDocs>, String> {
    wiki.get_docs(key).await.map_err(|e| match e.status() {
        Some(StatusCode::NOT_FOUND) => "*No documentation available.*".into(),
        _ => format!("*Failed to fetch documentation - `{e}`*"),
    })
}

pub fn describe(entry: Option<&DocEntry>) -> &str {
    entry
        .and_then(|e| e.description.as_deref())
        .unwrap_or("*No documentation available.*")
}
