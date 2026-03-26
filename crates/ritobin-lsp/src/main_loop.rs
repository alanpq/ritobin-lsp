use lsp_server::{Connection, Message};
use lsp_types::notification::Notification as _;
use lsp_types::request::Request as _;
use std::sync::Arc;

use crate::{
    config::Config,
    handlers,
    lsp::{
        self,
        ext::{ServerStatusNotification, ServerStatusParams},
    },
    server::Server,
};

pub async fn main_loop(config: Config, connection: Connection) -> anyhow::Result<()> {
    let files = directories_next::ProjectDirs::from("com", "alanpq", "ritobin-lsp")
        .expect("invalid app id for dirs");

    let server = Arc::new(Server::new(connection, config));

    server.meta.load_file(
        std::env::var("RB_META_DUMP_PATH")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| files.cache_dir().join("dump.json")),
    );

    let not = lsp_server::Notification::new(
        ServerStatusNotification::METHOD.to_owned(),
        ServerStatusParams {
            health: lsp::ext::Health::Ok,
            quiescent: true,
            message: None,
        },
    );
    server
        .conn
        .sender
        .send(lsp_server::Message::Notification(not))?;

    for msg in &server.conn.receiver {
        match msg {
            Message::Request(req) => {
                if server.conn.handle_shutdown(&req)? {
                    break;
                }
                if let Err(err) = handlers::request(&server, &req).await {
                    tracing::error!("[lsp] request {} failed: {err}", &req.method);
                }
            }
            Message::Notification(note) => {
                if let Err(err) = handlers::notification(&server, &note).await {
                    tracing::error!("[lsp] notification {} failed: {err}", note.method);
                }
            }
            Message::Response(resp) => tracing::error!("[lsp] response: {resp:?}"),
        }
    }
    Ok(())
}
