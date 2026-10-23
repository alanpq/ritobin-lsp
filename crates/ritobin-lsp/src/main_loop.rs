use lsp_server::{Connection, Message};
use ltk_mimir_cache::UpdateOutcome;
use paths::Utf8PathBuf;
use std::{num::NonZeroUsize, path::PathBuf, sync::Arc};

use crate::{
    config::Config,
    game::{GameConfig, GameService, IndexState, Stage, service::default_game_dir},
    handlers,
    lsp::ext::GameIndexStateNotification,
    server::{Hashes, Server},
    status::TaskStatus,
};

/// The game-search configuration: `RB_GAME_DIR`, then the setting, then the platform default.
fn game_config(config: &Config) -> GameConfig {
    let opts = config.initialization_options.as_ref();
    let env_dir = std::env::var_os("RB_GAME_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from);
    let game_dir = env_dir
        .or_else(|| opts.and_then(|o| o.game_directory.clone()))
        .and_then(|p| Utf8PathBuf::from_path_buf(p).ok())
        .or_else(default_game_dir);
    let search = opts.map(|o| o.game_search).unwrap_or_default();
    GameConfig {
        game_dir,
        enabled: search.enabled.unwrap_or(true),
        workers: search.workers.and_then(NonZeroUsize::new),
    }
}

/// The status-bar task a game index state maps to.
fn game_task(state: &IndexState) -> TaskStatus {
    match state {
        IndexState::Unconfigured | IndexState::Ready(_) => TaskStatus::Ready,
        IndexState::Building { stage, done, total } => TaskStatus::Loading(match stage {
            Stage::Walk => format!("Indexing game ({done}/{total})"),
            _ => "Indexing game".to_owned(),
        }),
        IndexState::Failed { message } => TaskStatus::Failed(format!("Game index: {message}")),
    }
}

pub async fn main_loop(config: Config, connection: Connection) -> anyhow::Result<()> {
    let files = directories_next::ProjectDirs::from("com", "alanpq", "ritobin-lsp")
        .expect("invalid app id for dirs");

    let hashes = Hashes::new()
        .inspect_err(|e| {
            tracing::error!("Failed to resolve hashtable directory: {e}");
            tracing::warn!("No hashes will be loaded.");
        })
        .ok();

    let game = Arc::new(GameService::new(files.cache_dir(), game_config(&config)));
    tracing::info!("game search: {:?}", game.config());
    let server = Arc::new(Server::new(
        connection,
        config.clone(),
        hashes.clone(),
        game.clone(),
    ));

    server.update_status(|status| {
        status.hashes = match hashes.is_some() {
            true => TaskStatus::Loading("Updating hashtables".to_owned()),
            false => TaskStatus::Failed("No hashtable directory".to_owned()),
        };
        status.meta = TaskStatus::Loading("Loading meta dump".to_owned());
        status.game_index = game_task(&game.state());
    });

    game.on_change({
        let server = server.clone();
        move |state| {
            if let Err(e) = server.send_notification::<GameIndexStateNotification>(state.clone()) {
                tracing::error!("failed to send game index state: {e}");
            }
            let task = game_task(state);
            server.update_status(|status| status.game_index = task);
        }
    });

    match &hashes {
        // The build waits for the tables: the name layer is cut from them.
        Some(_) => {}
        None => {
            let game = game.clone();
            tokio::task::spawn_blocking(move || game.build(None));
        }
    }

    if let Some(hashes) = hashes {
        tokio::spawn({
            let server = server.clone();
            let game = game.clone();
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
                tokio::task::spawn_blocking(move || game.build(Some(&hashes)));
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
                    if let Err(e) = server.meta.load_path(dir).await {
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

            let outcome = match server.meta.loaded() {
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
