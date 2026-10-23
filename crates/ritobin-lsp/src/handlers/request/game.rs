//! Handlers for whole-game search: the custom `ritobin-lsp/gameSearch`,
//! `ritobin-lsp/deserializeWadChunk`, `ritobin-lsp/gameObjectPreview` and
//! `ritobin-lsp/rebuildGameIndex` requests, and the standard `workspace/symbol` pair that puts
//! the same index behind the editor's own "go to symbol in workspace".
//!
//! Each runs off the main loop through [`Server::respond_blocking`]; a search over the ready set
//! is a memory scan, a chunk document mounts one archive, and a rebuild walks the game.

use std::sync::Arc;

use lsp_server::RequestId;
use lsp_types::{
    Location, OneOf, Position, Range, SymbolKind, Url, WorkspaceLocation, WorkspaceSymbol,
    WorkspaceSymbolParams, WorkspaceSymbolResponse,
    request::{Request as _, WorkspaceSymbolRequest, WorkspaceSymbolResolve},
};
use ltk_game_index::WadHash;
use serde::{Deserialize, Serialize};

use crate::{
    game::{Hit, TermKind},
    lsp::ext::{
        DeserializeWadChunk, DeserializeWadChunkParams, GameBrowse, GameBrowseParams,
        GameObjectPreview, GameObjectPreviewParams, GameSearch, GameSearchParams,
    },
    server::Server,
};

/// Symbols a `workspace/symbol` answer holds at most. The editor filters and ranks on its side.
const WORKSPACE_SYMBOL_LIMIT: u32 = 200;

/// Lines of an object a preview holds by default.
const PREVIEW_LINES: u32 = 12;

pub fn handle_search(server: &Arc<Server>, id: RequestId, params: GameSearchParams) {
    let game = server.game.clone();
    server.respond_blocking(id, GameSearch::METHOD, move || {
        game.search(
            &params.query,
            &params.kinds,
            params.offset,
            params.limit.unwrap_or(ltk_game_index::DEFAULT_LIMIT),
        )
    });
}

pub fn handle_deserialize_wad_chunk(
    server: &Arc<Server>,
    id: RequestId,
    params: DeserializeWadChunkParams,
) {
    let game = server.game.clone();
    let hashes = server
        .hashes
        .as_ref()
        .map(|h| h.bin_provider())
        .unwrap_or_default();
    server.respond_blocking(id, DeserializeWadChunk::METHOD, move || {
        let hash = parse_wad_hash(&params.chunk)?;
        game.render(hash, hashes).map(|doc| (*doc).clone())
    });
}

pub fn handle_preview(server: &Arc<Server>, id: RequestId, params: GameObjectPreviewParams) {
    let game = server.game.clone();
    let hashes = server
        .hashes
        .as_ref()
        .map(|h| h.bin_provider())
        .unwrap_or_default();
    server.respond_blocking(id, GameObjectPreview::METHOD, move || {
        let hash = parse_wad_hash(&params.chunk)?;
        let doc = game.render(hash, hashes)?;
        Ok(doc.preview(
            &params.object,
            params.max_lines.unwrap_or(PREVIEW_LINES) as usize,
        ))
    });
}

pub fn handle_browse(server: &Arc<Server>, id: RequestId, params: GameBrowseParams) {
    let game = server.game.clone();
    server.respond_blocking(id, GameBrowse::METHOD, move || {
        game.browse(params.dir.as_deref().unwrap_or(""))
    });
}

pub fn handle_rebuild(server: &Arc<Server>, id: RequestId) {
    let game = server.game.clone();
    let hashes = server.hashes.clone();
    game.invalidate_objects();
    tokio::task::spawn_blocking(move || game.build(hashes.as_ref()));
    if let Err(e) = server.send_ok(id, &()) {
        tracing::error!("failed to send rebuildGameIndex response: {e:?}");
    }
}

/// What a workspace symbol carries to its resolve: the chunk and object it stands for.
#[derive(Debug, Serialize, Deserialize)]
struct SymbolData {
    chunk: String,
    object: String,
}

/// `workspace/symbol`: objects and classes of the game matching the query, located in their chunk
/// document without a range. The range comes from `workspaceSymbol/resolve` when one is picked.
pub fn handle_workspace_symbol(server: &Arc<Server>, id: RequestId, params: WorkspaceSymbolParams) {
    let game = server.game.clone();
    server.respond_blocking(id, WorkspaceSymbolRequest::METHOD, move || {
        if game.ready().is_none() || params.query.trim().is_empty() {
            return Ok(Some(WorkspaceSymbolResponse::Nested(Vec::new())));
        }
        let result = game.search(
            &params.query,
            &[TermKind::Object, TermKind::Class],
            0,
            WORKSPACE_SYMBOL_LIMIT,
        )?;
        let symbols = result
            .hits
            .iter()
            .filter_map(symbol_of)
            .collect::<Vec<WorkspaceSymbol>>();
        Ok(Some(WorkspaceSymbolResponse::Nested(symbols)))
    });
}

fn symbol_of(hit: &Hit) -> Option<WorkspaceSymbol> {
    let uri = chunk_uri(&hit.chunk.hash, hit.chunk.name.as_deref())?;
    let class = hit
        .class
        .name
        .clone()
        .unwrap_or_else(|| hit.class.hash.clone());
    let chunk = hit
        .chunk
        .name
        .clone()
        .unwrap_or_else(|| hit.chunk.hash.clone());
    Some(WorkspaceSymbol {
        name: hit
            .object
            .name
            .clone()
            .unwrap_or_else(|| hit.object.hash.clone()),
        kind: SymbolKind::CLASS,
        tags: None,
        container_name: Some(format!("{class} · {chunk}")),
        location: OneOf::Right(WorkspaceLocation { uri }),
        data: serde_json::to_value(SymbolData {
            chunk: hit.chunk.hash.clone(),
            object: hit.object.hash.clone(),
        })
        .ok(),
    })
}

/// `workspaceSymbol/resolve`: the symbol with its range, from the rendered chunk.
pub fn handle_workspace_symbol_resolve(
    server: &Arc<Server>,
    id: RequestId,
    mut symbol: WorkspaceSymbol,
) {
    let game = server.game.clone();
    let hashes = server
        .hashes
        .as_ref()
        .map(|h| h.bin_provider())
        .unwrap_or_default();
    server.respond_blocking(id, WorkspaceSymbolResolve::METHOD, move || {
        let data: SymbolData = symbol
            .data
            .take()
            .and_then(|d| serde_json::from_value(d).ok())
            .ok_or_else(|| "workspace symbol carries no chunk and object".to_owned())?;
        let uri = match &symbol.location {
            OneOf::Left(location) => location.uri.clone(),
            OneOf::Right(location) => location.uri.clone(),
        };
        let hash = parse_wad_hash(&data.chunk)?;
        let doc = game.render(hash, hashes)?;
        let line = doc
            .entries
            .iter()
            .find(|e| e.object.eq_ignore_ascii_case(&data.object))
            .map_or(0, |e| e.line);
        let at = Position::new(line, 0);
        symbol.location = OneOf::Left(Location::new(uri, Range::new(at, at)));
        Ok(symbol)
    });
}

/// The document uri of a chunk: `ritobin-wad://<hex>/<display>.rito`, as the extension builds it.
///
/// The authority is the chunk hash without `0x`; the path is the chunk's name, or the hex again
/// for an unnamed chunk, and only names the tab.
fn chunk_uri(chunk_hash: &str, display: Option<&str>) -> Option<Url> {
    let hex = chunk_hash.trim_start_matches("0x").to_ascii_lowercase();
    let display = display
        .filter(|d| !d.is_empty())
        .map(|d| d.trim_start_matches('/').to_owned())
        .unwrap_or_else(|| hex.clone());
    let mut url = Url::parse(&format!("ritobin-wad://{hex}/")).ok()?;
    url.set_path(&format!("/{display}.rito"));
    Some(url)
}

/// A chunk hash: sixteen hex digits, with or without `0x`.
fn parse_wad_hash(text: &str) -> Result<WadHash, String> {
    let digits = text.strip_prefix("0x").unwrap_or(text);
    (digits.len() == 16)
        .then(|| u64::from_str_radix(digits, 16).ok())
        .flatten()
        .map(WadHash)
        .ok_or_else(|| format!("'{text}' is not a chunk hash"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_uris_match_the_extension_shape() {
        let named = chunk_uri(
            "0x00ABCDEF01234567",
            Some("data/characters/aatrox/aatrox.bin"),
        );
        assert_eq!(
            named.unwrap().as_str(),
            "ritobin-wad://00abcdef01234567/data/characters/aatrox/aatrox.bin.rito"
        );
        let unnamed = chunk_uri("0x00abcdef01234567", None);
        assert_eq!(
            unnamed.unwrap().as_str(),
            "ritobin-wad://00abcdef01234567/00abcdef01234567.rito"
        );
        let spaced = chunk_uri("0x1", Some("data/odd name.bin")).unwrap();
        assert_eq!(spaced.path(), "/data/odd%20name.bin.rito");
    }
}
