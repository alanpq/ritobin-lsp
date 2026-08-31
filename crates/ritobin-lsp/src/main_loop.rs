use lsp_server::{Connection, Message};
use ltk_mimir_cache::UpdateOutcome;
use std::sync::{Arc, atomic::Ordering};

use crate::{
    config::Config,
    handlers,
    server::{Hashes, Server},
    status::TaskStatus,
};

pub async fn main_loop(config: Config, connection: Connection) -> anyhow::Result<()> {
    let files = directories_next::ProjectDirs::from("com", "alanpq", "ritobin-lsp")
        .expect("invalid app id for dirs");

    let hashes = Hashes::new()
        .inspect_err(|e| {
            tracing::error!("Failed to resolve hashtable directory: {e}");
            tracing::warn!("No hashes will be loaded.");
        })
        .ok();

    let server = Arc::new(Server::new(connection, config.clone(), hashes.clone()));

    server.update_status(|status| {
        status.hashes = match hashes.is_some() {
            true => TaskStatus::Loading("Updating hashtables".to_owned()),
            false => TaskStatus::Failed("No hashtable directory".to_owned()),
        };
        status.meta = TaskStatus::Loading("Loading meta dump".to_owned());
    });

    if let Some(hashes) = hashes {
        tokio::spawn({
            let server = server.clone();
            async move {
                tracing::info!("Checking for new hashes...");
                let outcome = match hashes.update().await {
                    Ok(UpdateOutcome::Completed(report)) => {
                        tracing::info!("Updated {} tables.", report.installed.len());
                        TaskStatus::Ready
                    }
                    Ok(UpdateOutcome::Locked) => {
                        tracing::info!("Another application is updating hashes. Doing nothing.");
                        TaskStatus::Ready
                    }
                    Err(e) => {
                        tracing::error!("Failed to update hashtables: {e}");
                        TaskStatus::Failed("Could not update hashtables".to_owned())
                    }
                };
                hashes.load();
                server.update_status(|status| status.hashes = outcome);
            }
        });
    }

    tokio::spawn({
        let server = server.clone();
        let meta_override = std::env::var("RB_META_DUMP_PATH")
            .ok()
            .and_then(|v| v.parse().ok())
            .or_else(|| {
                config
                    .initialization_options
                    .as_ref()
                    .and_then(|o| o.meta_dump_path.clone())
            });
        async move {
            match meta_override {
                Some(meta_override) => {
                    if let Err(e) = server.meta.load_file(meta_override).await {
                        tracing::error!("Failed to load overridden meta dump - {e:?}");
                    }
                    tracing::info!(
                        "Skipping latest meta dump fetching - dump file path has been explicitly specified."
                    );
                }
                None => {
                    let dir = files.cache_dir();
                    if let Err(e) = server.meta.load(dir).await {
                        tracing::error!("Failed to load existing meta - {e:?}");
                    }

                    server.update_status(|status| {
                        status.meta = TaskStatus::Loading("Updating meta dump".to_owned())
                    });

                    match server.meta.fetch_latest(dir).await {
                        Err(e) => {
                            tracing::error!("Failed to fetch latest meta dump - {e:?}");
                        }
                        Ok(Some(path)) => {
                            if let Err(e) = server.meta.load_file(path).await {
                                tracing::error!("Failed to load fetched meta dump - {e:?}");
                            }
                        }
                        Ok(None) => {}
                    }
                }
            }

            let outcome = match server.meta.loaded.load(Ordering::Relaxed) {
                true => TaskStatus::Ready,
                false => TaskStatus::Failed("No meta dump available".to_owned()),
            };
            server.update_status(|status| status.meta = outcome);
        }
    });

    for msg in &server.conn.receiver {
        match msg {
            Message::Request(req) => {
                if server.conn.handle_shutdown(&req)? {
                    break;
                }
                let method = req.method.clone();
                if let Err(err) = handlers::request(&server, req).await {
                    tracing::error!("[lsp] request {} failed: {err}", method);
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
