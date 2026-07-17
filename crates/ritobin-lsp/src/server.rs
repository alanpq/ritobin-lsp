use std::{collections::HashMap, sync::Arc};

use arc_swap::ArcSwap;
use lsp_server::{Connection, Message, RequestId, Response};
use lsp_types::Url;
use ltk_hashdb::HashDb;
use ltk_mimir_cache::{HashStore, Table, UpdateOptions, UpdateOutcome};
use rustc_hash::FxHashMap;
use tokio::sync::RwLock;

use crate::{config::Config, lol_meta::service::MetaService, worker::WorkerHandle};

#[derive(Clone)]
pub struct Hashes {
    store: Arc<HashStore>,
    tables: Arc<ArcSwap<HashMap<Table, Arc<HashDb>>>>,
}

impl Hashes {
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
    pub hashes: Option<Hashes>,
}

impl Server {
    pub fn new(conn: Connection, config: Config, hashes: Option<Hashes>) -> Self {
        Self {
            conn,
            config,
            workers: Default::default(),
            meta: MetaService::default(),
            hashes,
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
