use lsp_server::{Connection, Message};
use lsp_types::request::Request as _;
use lsp_types::notification::Notification as _;
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

pub fn main_loop(config: Config, connection: Connection) -> anyhow::Result<()> {
    let server = Arc::new(Server::new(connection, config));

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
        tracing::info!("MSG");
        match msg {
            Message::Request(req) => {
                if server.conn.handle_shutdown(&req)? {
                    break;
                }
                let server = server.clone();
                std::thread::spawn(move || {
                    if let Err(err) = handlers::request(&server, &req) {
                        tracing::error!("[lsp] request {} failed: {err}", &req.method);
                    }
                });
            }
            Message::Notification(note) => {
                let server = server.clone();
                std::thread::spawn(move || {
                    if let Err(err) = handlers::notification(&server, &note) {
                        tracing::error!("[lsp] notification {} failed: {err}", note.method);
                    }
                });
            }
            Message::Response(resp) => tracing::error!("[lsp] response: {resp:?}"),
        }
    }
    Ok(())
}
