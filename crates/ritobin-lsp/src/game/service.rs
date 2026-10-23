//! The service that owns the index lifecycle: configuration, build, state, and the ready set.

use std::{
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use arc_swap::ArcSwapOption;
use camino::{Utf8Path, Utf8PathBuf};
use ltk_game_index::{
    BuildOptions, Dir, GameIndex, GameSearch, ListOptions, NameSheets, Query, ResolveWadPath,
    TermIndex, TermKind, TermKinds, WadHash,
};
use ltk_mimir_cache::Table;
use serde::{Deserialize, Serialize};

use crate::server::{BinHashProvider, Hashes};

use super::{
    chunk::{self, ChunkDocument, ChunkError},
    names::{ChunkPaths, MimirNames},
    results::{IndexStats, Listing, Namer, SearchResult, UNNAMED_DIR},
};

/// The cache files of the game search, under the service's cache directory.
const CHUNKS_CACHE: &str = "chunks.bin";
const NAMES_CACHE: &str = "names.bin";
const TERMS_CACHE: &str = "terms.bin";

/// Cache files of earlier layouts, removed on every build.
const LEGACY_CACHE: &[&str] = &["objects.bin", "strings.txt", "strings.val"];

/// Which step a build is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    Chunks,
    Names,
    Walk,
    Save,
}

/// Where the index is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum IndexState {
    Unconfigured,
    Building { stage: Stage, done: u32, total: u32 },
    Ready(IndexStats),
    Failed { message: String },
}

/// The chunk index and the search over it, built together and swapped in together.
pub struct Ready {
    pub game: GameIndex,
    pub search: GameSearch,
}

impl Ready {
    fn namer(&self) -> Namer<'_> {
        Namer {
            game: &self.game,
            search: &self.search,
        }
    }
}

/// What a build needs to know.
#[derive(Debug, Clone)]
pub struct GameConfig {
    pub game_dir: Option<Utf8PathBuf>,
    pub enabled: bool,
    pub workers: Option<NonZeroUsize>,
}

/// A listener for state changes.
type StateListener = Box<dyn Fn(&IndexState) + Send + Sync>;

/// Owns the current index, its state, and the one build running at a time.
pub struct GameService {
    cache_dir: PathBuf,
    config: GameConfig,
    state: Mutex<IndexState>,
    ready: ArcSwapOption<Ready>,
    /// The build generation; a build whose generation is behind this is called off.
    generation: AtomicU64,
    /// Listeners for state changes.
    on_change: Mutex<Vec<StateListener>>,
    /// Whether a build is running.
    building: AtomicBool,
    /// The chunk document rendered last. A symbol resolve, its open and its preview hit the
    /// same chunk in a row.
    rendered: Mutex<Option<(WadHash, Arc<ChunkDocument>)>>,
}

impl GameService {
    pub fn new(cache_dir: impl Into<PathBuf>, config: GameConfig) -> Self {
        let cache_dir = cache_dir.into().join("game-search");
        let state = match (config.enabled, &config.game_dir) {
            (true, Some(_)) => IndexState::Building {
                stage: Stage::Chunks,
                done: 0,
                total: 0,
            },
            _ => IndexState::Unconfigured,
        };
        Self {
            cache_dir,
            config,
            state: Mutex::new(state),
            ready: ArcSwapOption::empty(),
            generation: AtomicU64::new(0),
            on_change: Mutex::new(Vec::new()),
            building: AtomicBool::new(false),
            rendered: Mutex::new(None),
        }
    }

    /// Renders chunk `hash` as ritobin text, reusing the last render of the same chunk.
    pub fn render(
        &self,
        hash: WadHash,
        hashes: BinHashProvider,
    ) -> Result<Arc<ChunkDocument>, String> {
        if let Some((cached, doc)) = self.rendered.lock().unwrap().as_ref()
            && *cached == hash
        {
            return Ok(doc.clone());
        }
        let ready = self.ready().ok_or_else(|| self.not_ready_message())?;
        let doc = Arc::new(
            chunk::render(&ready.game, hash, hashes).map_err(|e: ChunkError| e.to_string())?,
        );
        *self.rendered.lock().unwrap() = Some((hash, doc.clone()));
        Ok(doc)
    }

    pub fn config(&self) -> &GameConfig {
        &self.config
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Registers a listener called with every state change, and once with the current state.
    pub fn on_change(&self, f: impl Fn(&IndexState) + Send + Sync + 'static) {
        f(&self.state());
        self.on_change.lock().unwrap().push(Box::new(f));
    }

    pub fn state(&self) -> IndexState {
        self.state.lock().unwrap().clone()
    }

    fn set_state(&self, state: IndexState) {
        *self.state.lock().unwrap() = state.clone();
        for f in self.on_change.lock().unwrap().iter() {
            f(&state);
        }
    }

    /// The ready set, when the index is `Ready`.
    pub fn ready(&self) -> Option<Arc<Ready>> {
        self.ready.load_full()
    }

    /// One page of hits for `query` through `kinds`, every kind when `kinds` is empty.
    pub fn search(
        &self,
        query: &str,
        kinds: &[TermKind],
        offset: u32,
        limit: u32,
    ) -> Result<SearchResult, String> {
        let ready = self.ready().ok_or_else(|| self.not_ready_message())?;
        let mut q = Query::new(query);
        if !kinds.is_empty() {
            q.kinds = TermKinds::of(kinds.iter().copied());
        }
        q.offset = offset;
        q.limit = limit;
        let result = ready.search.search(&q);
        Ok(ready.namer().search_result(result))
    }

    /// One level of the object tree: `""` for the root, [`UNNAMED_DIR`] for the objects no
    /// table names, a path otherwise. A path no object runs through lists nothing.
    pub fn browse(&self, dir: &str) -> Result<Listing, String> {
        let ready = self.ready().ok_or_else(|| self.not_ready_message())?;
        let dir = match dir {
            "" => Dir::Root,
            UNNAMED_DIR => Dir::Unnamed,
            path => Dir::Path(path),
        };
        let listing = ready
            .search
            .list(dir, &ListOptions::default())
            .map(|listing| ready.namer().listing(listing));
        Ok(listing.unwrap_or(Listing {
            entries: Vec::new(),
            total: 0,
            truncated: false,
        }))
    }

    /// The message a request gets while the index is not `Ready`.
    pub fn not_ready_message(&self) -> String {
        match self.state() {
            IndexState::Unconfigured => "game directory is not set".to_owned(),
            IndexState::Building { stage, done, total } => match stage {
                Stage::Chunks => "game index is building: reading archives".to_owned(),
                Stage::Names => "game index is building: cutting name sheets".to_owned(),
                Stage::Walk => format!("game index is building: walking archive {done} of {total}"),
                Stage::Save => "game index is building: saving".to_owned(),
            },
            IndexState::Failed { message } => format!("game index failed: {message}"),
            IndexState::Ready(_) => "game index is not loaded".to_owned(),
        }
    }

    /// Deletes the term index so the next build walks again.
    pub fn invalidate_objects(&self) {
        self.remove_cache(TERMS_CACHE);
    }

    fn remove_cache(&self, name: &str) {
        let path = self.cache_dir.join(name);
        if let Err(e) = std::fs::remove_file(&path)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!("game search: cannot remove {}: {e}", path.display());
        }
    }

    /// Removes the cache files of earlier layouts: the object index, the strings sheet and the
    /// per-table name sheets.
    fn remove_legacy_cache(&self) {
        for name in LEGACY_CACHE {
            self.remove_cache(name);
        }
        let Ok(entries) = std::fs::read_dir(&self.cache_dir) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with("names-") {
                self.remove_cache(&name.to_string_lossy());
            }
        }
    }

    /// Builds or loads the index, blocking the calling thread. Any build in flight is called
    /// off first; the newer build wins.
    pub fn build(self: &Arc<Self>, hashes: Option<&Hashes>) {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let (enabled, game_dir) = (self.config.enabled, self.config.game_dir.clone());
        let Some(game_dir) = game_dir.filter(|_| enabled) else {
            self.set_state(IndexState::Unconfigured);
            return;
        };
        self.building.store(true, Ordering::SeqCst);
        *self.rendered.lock().unwrap() = None;
        let outcome = self.build_inner(generation, &game_dir, hashes);
        if self.generation.load(Ordering::SeqCst) != generation {
            // A newer build took over; it owns the state.
            return;
        }
        self.building.store(false, Ordering::SeqCst);
        match outcome {
            Ok(ready) => {
                let stats = IndexStats::of(&ready.game, ready.search.terms());
                self.ready.store(Some(Arc::new(ready)));
                self.set_state(IndexState::Ready(stats));
            }
            Err(message) => {
                tracing::error!("game search: {message}");
                self.set_state(IndexState::Failed { message });
            }
        }
    }

    fn cache_path(&self, name: &str) -> Result<Utf8PathBuf, String> {
        Utf8PathBuf::from_path_buf(self.cache_dir.join(name))
            .map_err(|p| format!("cache path is not UTF-8: {}", p.display()))
    }

    fn build_inner(
        self: &Arc<Self>,
        generation: u64,
        game_dir: &Utf8Path,
        hashes: Option<&Hashes>,
    ) -> Result<Ready, String> {
        let progress = BuildProgress {
            service: self.clone(),
            generation,
        };
        if let Err(e) = std::fs::create_dir_all(&self.cache_dir) {
            return Err(format!(
                "cannot create cache dir {}: {e}",
                self.cache_dir.display()
            ));
        }
        self.remove_legacy_cache();

        progress.stage(Stage::Chunks, 0, 0);
        let game = GameIndex::load_or_build(game_dir, &self.cache_path(CHUNKS_CACHE)?)
            .map_err(|e| e.to_string())?;
        for skipped in game.skipped() {
            tracing::warn!(
                "game search: archive {} skipped: {}",
                game.archive(skipped.archive).name,
                skipped.error
            );
        }
        if progress.called_off() {
            return Err("build was called off".to_owned());
        }

        progress.stage(Stage::Names, 0, 0);
        let names = match hashes {
            Some(hashes) => {
                let source = MimirNames {
                    hashes,
                    manifest: hashes.manifest().ok(),
                };
                NameSheets::load_or_build(&self.cache_path(NAMES_CACHE)?, &source, &game)
            }
            None => NameSheets::default(),
        };

        let terms_path = self.cache_path(TERMS_CACHE)?;
        let terms = match TermIndex::load_for(&terms_path, &game) {
            Ok(terms) => {
                tracing::info!("game search: term index loaded from {terms_path}");
                terms
            }
            Err(e) => {
                if e.is_missing_file() {
                    tracing::debug!("game search: {e}");
                } else {
                    tracing::warn!("game search: rebuilding the term index: {e}");
                }
                let chunk_paths = hashes.and_then(|h| h.table(Table::Game)).map(ChunkPaths);
                let called_off = || progress.called_off();
                let on_archive = |done: u32, total: u32| progress.stage(Stage::Walk, done, total);
                let mut options = BuildOptions::default();
                options.resolver = chunk_paths.as_ref().map(|p| p as &dyn ResolveWadPath);
                options.workers = self.config.workers;
                options.called_off = Some(&called_off);
                options.on_archive = Some(&on_archive);
                progress.stage(Stage::Walk, 0, 0);
                let terms = TermIndex::build_with(&game, &options).map_err(|e| e.to_string())?;
                progress.stage(Stage::Save, 0, 0);
                if let Err(e) = terms.save(&terms_path) {
                    tracing::warn!("game search: term index failed to save: {e}");
                }
                terms
            }
        };

        let search = GameSearch::new(terms, names);
        search.prepare();
        Ok(Ready { game, search })
    }
}

struct BuildProgress {
    service: Arc<GameService>,
    generation: u64,
}

impl BuildProgress {
    fn called_off(&self) -> bool {
        self.service.generation.load(Ordering::SeqCst) != self.generation
    }

    /// Reports `stage`. Every archive of the walk reports; the client throttles its own
    /// rendering.
    fn stage(&self, stage: Stage, done: u32, total: u32) {
        if !self.called_off() {
            self.service
                .set_state(IndexState::Building { stage, done, total });
        }
    }
}

/// The platform's default install path, when it exists.
pub fn default_game_dir() -> Option<Utf8PathBuf> {
    let candidates: &[&str] = if cfg!(windows) {
        &[r"C:\Riot Games\League of Legends\Game"]
    } else if cfg!(target_os = "macos") {
        &["/Applications/League of Legends.app/Contents/LoL/Game"]
    } else {
        &[]
    };
    candidates
        .iter()
        .map(Utf8PathBuf::from)
        .find(|p| p.join("DATA").join("FINAL").is_dir())
}
