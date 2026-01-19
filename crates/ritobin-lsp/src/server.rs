use std::sync::RwLock;

use lsp_server::{Connection, Message, RequestId, Response};
use lsp_types::Url;
use rustc_hash::FxHashMap;

use crate::{config::Config, document::Document, lol_meta::service::MetaService};

pub struct Server {
    pub conn: Connection,
    pub config: Config,
    pub docs: RwLock<FxHashMap<Url, Document>>,
    pub meta: MetaService,
}

impl Server {
    pub fn new(conn: Connection, config: Config) -> Self {
        Self {
            conn,
            config,
            docs: Default::default(),
            meta: MetaService::default(),
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
