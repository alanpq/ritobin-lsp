use std::{fs::File, io::BufReader, path::Path, sync::Arc};

use anyhow::Context;
use lsp_server::{Connection, Message, RequestId, Response};
use lsp_types::Url;
use poro_hash::{BinHash, Hashtable};
use rustc_hash::FxHashMap;
use tokio::sync::RwLock;

use crate::{
    config::Config, document::Document, lol_meta::service::MetaService, worker::WorkerHandle,
};

#[derive(Default)]
pub struct Hashes {
    pub entries: Option<Hashtable<BinHash>>,
    pub fields: Option<Hashtable<BinHash>>,
    pub hashes: Option<Hashtable<BinHash>>,
    pub types: Option<Hashtable<BinHash>>,
}

impl Hashes {
    fn load_table_fst(p: impl AsRef<Path>) -> anyhow::Result<Hashtable<BinHash>> {
        let set =
            fst::Set::new(std::fs::read(&p).with_context(|| format!("reading {:?}", p.as_ref()))?)?;
        Ok(Hashtable::from_fst(set)?)
    }
    fn load_table_txt(p: impl AsRef<Path>) -> anyhow::Result<Hashtable<BinHash>> {
        let mut f = BufReader::new(std::fs::File::open(&p)?);
        Ok(Hashtable::read_hashtable_file(&mut f)?)
    }

    pub fn load_from_directory<P: AsRef<Path>>(&mut self, dir: P) -> anyhow::Result<()> {
        let dir = dir.as_ref();
        self.entries
            .replace(Self::load_table_txt(dir.join("hashes.binentries.txt"))?);
        self.fields
            .replace(Self::load_table_txt(dir.join("hashes.binfields.txt"))?);
        self.hashes
            .replace(Self::load_table_txt(dir.join("hashes.binhashes.txt"))?);
        self.types
            .replace(Self::load_table_txt(dir.join("hashes.bintypes.txt"))?);
        Ok(())
    }
}

pub struct Server {
    pub conn: Connection,
    pub config: Config,
    pub workers: RwLock<FxHashMap<Url, WorkerHandle>>,
    pub meta: MetaService,
    pub hashes: Hashes,
}

impl Server {
    pub fn new(conn: Connection, config: Config) -> Self {
        Self {
            conn,
            config,
            workers: Default::default(),
            meta: MetaService::default(),
            hashes: Hashes::default(),
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
