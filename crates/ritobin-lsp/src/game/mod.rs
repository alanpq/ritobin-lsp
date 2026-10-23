//! Whole-game search: `ltk_game_index`'s term index and name sheets over a League installation.
//!
//! The crate builds the chunk index, the term index, the name sheets, query resolution and
//! the object tree. Here are the Mimir adapters (`names`), the shapes the requests answer in
//! (`results`), chunk documents (`chunk`) and the service that owns the lifecycle (`service`).
//!
//! Spec: `docs/design/game-search.md`.

pub mod chunk;
pub mod names;
pub mod results;
pub mod service;

pub use ltk_game_index::TermKind;
pub use results::{Hit, IndexStats, Match, Named};
pub use service::{GameConfig, GameService, IndexState, Stage};
