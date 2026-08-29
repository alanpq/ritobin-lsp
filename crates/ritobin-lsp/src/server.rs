use std::{
    borrow::Cow,
    collections::HashMap,
    sync::{Arc, Mutex},
};

use arc_swap::ArcSwap;
use lsp_server::{Connection, Message, RequestId, Response};
use lsp_types::{
    Diagnostic, PublishDiagnosticsParams, Url,
    notification::{Notification as _, PublishDiagnostics},
};
use ltk_hash::BinHash;
use ltk_hashdb::HashDb;
use ltk_mimir_cache::{HashStore, Table, UpdateOptions, UpdateOutcome};
use ltk_ritobin::HashProvider;
use rustc_hash::FxHashMap;
use tokio::sync::RwLock;

use crate::{
    config::Config, lsp::ext::ServerStatusNotification, status::ServerStatus, worker::WorkerHandle,
};
use meta_wiki::{docs_cache::WikiDocs, service::MetaService};

#[derive(Clone)]
pub struct Hashes {
    store: Arc<HashStore>,
    tables: Arc<ArcSwap<HashMap<Table, Arc<HashDb>>>>,
}

/// See [`Hashes::snapshot`].
#[derive(Clone)]
pub struct HashesSnapshot(Arc<HashMap<Table, Arc<HashDb>>>);

impl HashesSnapshot {
    pub fn lookup(&self, table: Table, hash: u64) -> Option<Cow<'_, str>> {
        self.0.get(&table)?.get(hash)
    }
}

#[derive(Clone, Default)]
pub struct BinHashProvider {
    entries: Option<Arc<HashDb>>,
    fields: Option<Arc<HashDb>>,
    hashes: Option<Arc<HashDb>>,
    types: Option<Arc<HashDb>>,
    wad: Option<Arc<HashDb>>,
}

impl HashProvider for BinHashProvider {
    fn lookup_entry(&self, hash: BinHash) -> Option<std::borrow::Cow<'_, str>> {
        self.entries.as_ref()?.get((*hash).into())
    }

    fn lookup_field(&self, hash: BinHash) -> Option<std::borrow::Cow<'_, str>> {
        self.fields.as_ref()?.get((*hash).into())
    }

    fn lookup_hash(&self, hash: BinHash) -> Option<std::borrow::Cow<'_, str>> {
        self.hashes.as_ref()?.get((*hash).into())
    }

    fn lookup_type(&self, hash: BinHash) -> Option<std::borrow::Cow<'_, str>> {
        self.types.as_ref()?.get((*hash).into())
    }

    fn lookup_wad(&self, hash: ltk_hash::WadHash) -> Option<std::borrow::Cow<'_, str>> {
        self.wad.as_ref()?.get(*hash)
    }
}

impl Hashes {
    pub fn bin_provider(&self) -> BinHashProvider {
        BinHashProvider {
            entries: self.table(Table::BinEntries),
            fields: self.table(Table::BinFields),
            hashes: self.table(Table::BinHashes),
            types: self.table(Table::BinTypes),
            wad: self.table(Table::Game),
        }
    }
    pub async fn update(
        &self,
    ) -> Result<UpdateOutcome, ltk_mimir_cache::UpdateError<reqwest::Error>> {
        let store = self.store.clone();

        let fetch = |filename: &str| {
            let url = format!(
                "https://github.com/LeagueToolkit/mimir/releases/latest/download/{filename}"
            );
            async move {
                Ok(reqwest::get(&url)
                    .await?
                    .error_for_status()?
                    .bytes()
                    .await?
                    .to_vec())
            }
        };

        let outcome = store.update_async(&fetch, UpdateOptions::default()).await;

        if let Ok(UpdateOutcome::Completed(_)) = &outcome {
            self.load();
        }

        outcome
    }

    pub fn table(&self, table: Table) -> Option<Arc<HashDb>> {
        self.tables.load().get(&table).cloned()
    }

    /// A consistent, owned view of every table as of now. Tables are only ever swapped out as a
    /// whole (see [`Hashes::load`]), and only on startup or an explicit user-triggered update, so
    /// a snapshot held for e.g. the lifetime of one lint pass won't tear or go stale in any way
    /// that matters - and unlike per-call lookups through [`Hashes::table`], it lets lookups hand
    /// back data borrowed for as long as the snapshot itself is kept alive.
    pub fn snapshot(&self) -> HashesSnapshot {
        HashesSnapshot(self.tables.load_full())
    }

    pub fn load(&self) {
        self.tables.store(Arc::new(
            [
                Table::BinFields,
                Table::BinEntries,
                Table::BinHashes,
                Table::BinTypes,
                Table::Game,
            ]
            .into_iter()
            .filter_map(|table| {
                Some((
                    table,
                    self.store
                        .open(table)
                        .inspect_err(|e| {
                            tracing::warn!("Failed to load {table:?} hashes: {e}");
                        })
                        .ok()
                        .map(Arc::new)?,
                ))
            })
            .collect(),
        ));
    }

    pub fn new() -> Result<Self, ltk_mimir_cache::NoCacheDirError> {
        let store = HashStore::discover()?;
        Ok(Self {
            tables: Default::default(),
            store: store.into(),
        })
    }
}

pub struct Server {
    pub conn: Connection,
    pub config: Config,
    pub workers: RwLock<FxHashMap<Url, WorkerHandle>>,
    pub meta: MetaService,
    pub wiki: WikiDocs,
    pub hashes: Option<Hashes>,
    status: Mutex<ServerStatus>,
}

impl Server {
    pub fn new(conn: Connection, config: Config, hashes: Option<Hashes>) -> Self {
        Self {
            conn,
            config,
            workers: Default::default(),
            meta: MetaService::default(),
            wiki: WikiDocs::new("https://meta-api.leaguetoolkit.dev"),
            hashes,
            status: Default::default(),
        }
    }

    pub fn send_notification<N>(&self, params: N::Params) -> anyhow::Result<()>
    where
        N: lsp_types::notification::Notification,
    {
        self.conn
            .sender
            .send(Message::Notification(lsp_server::Notification::new(
                N::METHOD.to_owned(),
                params,
            )))?;
        Ok(())
    }

    /// Mutate the readiness state and tell the client about it. The lock is released before the
    /// notification goes out, so a slow client can never stall a background task.
    pub fn update_status(&self, f: impl FnOnce(&mut ServerStatus)) {
        let params = {
            let mut status = self.status.lock().unwrap();
            f(&mut status);
            status.params()
        };

        if let Err(e) = self.send_notification::<ServerStatusNotification>(params) {
            tracing::error!("failed to send server status: {e}");
        }
    }

    pub fn send_ok<T: serde::Serialize>(&self, id: RequestId, result: &T) -> anyhow::Result<()> {
        let resp = Response {
            id,
            result: Some(serde_json::to_value(result)?),
            error: None,
        };
        self.conn.sender.send(Message::Response(resp))?;
        Ok(())
    }

    pub fn send_err(
        &self,
        id: RequestId,
        code: lsp_server::ErrorCode,
        msg: &str,
    ) -> anyhow::Result<()> {
        let resp = Response {
            id,
            result: None,
            error: Some(lsp_server::ResponseError {
                code: code as i32,
                message: msg.into(),
                data: None,
            }),
        };
        self.conn.sender.send(Message::Response(resp))?;
        Ok(())
    }

    /// Replaces the document's whole diagnostic set. An empty `diagnostics` clears it.
    pub fn publish_diagnostics(
        &self,
        uri: Url,
        diagnostics: Vec<Diagnostic>,
    ) -> anyhow::Result<()> {
        let params = PublishDiagnosticsParams {
            uri,
            diagnostics,
            version: None,
        };

        self.conn
            .sender
            .send(Message::Notification(lsp_server::Notification::new(
                PublishDiagnostics::METHOD.to_owned(),
                params,
            )))?;

        Ok(())
    }

    /// Runs `work` on the blocking thread pool and sends its outcome as the
    /// response to `id`; a `String` error becomes a `RequestFailed` response
    /// error. `method` labels the error log if sending the response fails.
    pub fn respond_blocking<T, F>(self: &Arc<Self>, id: RequestId, method: &'static str, work: F)
    where
        T: serde::Serialize,
        F: FnOnce() -> Result<T, String> + Send + 'static,
    {
        let server = self.clone();

        tokio::task::spawn_blocking(move || {
            let sent = match work() {
                Ok(res) => server.send_ok(id, &res),
                Err(msg) => server.send_err(id, lsp_server::ErrorCode::RequestFailed, &msg),
            };

            if let Err(e) = sent {
                tracing::error!("failed to send {method} response: {e:?}");
            }
        });
    }
}
