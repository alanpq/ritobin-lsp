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
    pub async fn update(&self) -> Result<UpdateOutcome, ltk_mimir_cache::Error> {
        let store = self.store.clone();
        let outcome = tokio::task::spawn_blocking(move || {
            let fetch =
                |filename: &str| -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
                    let url = format!(
                        "https://github.com/LeagueToolkit/mimir/releases/latest/download/{filename}"
                    );
                    Ok(reqwest::blocking::get(&url)?
                        .error_for_status()?
                        .bytes()?
                        .to_vec())
                };

            store.update(&fetch, UpdateOptions::default())
        })
        .await
        .unwrap();

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

    pub fn new() -> Result<Self, ltk_mimir_cache::Error> {
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
}
