//! What the game search requests answer with, mapped from `ltk_game_index`'s results.
//!
//! The crate answers in hashes and ids. The extension reads names, `0x` hex and archive file
//! names, in camelCase.

use ltk_game_index::{
    ChunkKind, Declaration, GameIndex, GameSearch, KindCount, MatchedTerm, NameTable, ObjectNode,
    TermIndex, TermKind, TreeEntry as CrateEntry,
};
use serde::{Deserialize, Serialize};

/// The name of the unnamed directory, as the extension lists and requests it.
pub const UNNAMED_DIR: &str = "<unnamed>";

/// A hash with the name the tables give it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Named {
    /// `0x`-prefixed lowercase hex.
    pub hash: String,
    pub name: Option<String>,
}

/// One term a declaration matched on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Match {
    pub kind: TermKind,
    /// The term's name or string, or its hex for a hash no table names.
    pub text: String,
}

/// One declaration a query matched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub declaration: u32,
    pub object: Named,
    pub class: Named,
    pub chunk: Named,
    pub archive: String,
    pub is_override: bool,
    pub matches: Vec<Match>,
}

/// One page of a search.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    /// Declarations matching through each kind, whatever kinds the search asked for.
    pub kinds: Vec<KindCount>,
    /// Matching declarations before paging, through the requested kinds.
    pub total: u32,
    pub offset: u32,
    pub truncated: bool,
    pub hits: Vec<Hit>,
}

/// Where one object is declared first, and what it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectRef {
    pub object: Named,
    pub class: Named,
    pub chunk: Named,
    pub archive: String,
    pub is_override: bool,
}

/// One row of a directory listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum TreeEntry {
    #[serde(rename_all = "camelCase")]
    Dir {
        name: String,
        /// Objects anywhere beneath.
        objects: u32,
        /// The object at this path itself, when there is one.
        object: Option<ObjectRef>,
    },
    #[serde(rename_all = "camelCase")]
    Object {
        name: String,
        #[serde(flatten)]
        object: ObjectRef,
    },
}

/// One answer of `ritobin-lsp/gameBrowse`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    pub entries: Vec<TreeEntry>,
    pub total: u32,
    pub truncated: bool,
}

/// Counts of the build behind the ready set.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStats {
    pub archives: u32,
    pub chunks: u32,
    pub bins: u32,
    pub declarations: u32,
    pub hash_terms: u32,
    pub string_terms: u32,
    pub failed_chunks: u32,
    /// Milliseconds the walk took.
    pub elapsed: u64,
}

impl IndexStats {
    pub fn of(game: &GameIndex, terms: &TermIndex) -> Self {
        let objects = terms.objects().stats();
        Self {
            archives: objects.archives,
            chunks: game.len() as u32,
            bins: objects.bins,
            declarations: objects.declarations,
            hash_terms: terms.stats().hash_terms,
            string_terms: terms.stats().string_terms,
            failed_chunks: terms.stats().failed_chunks + objects.skipped_chunks,
            elapsed: objects.elapsed.as_millis() as u64,
        }
    }
}

/// `0x`-prefixed hex, 16 digits for a chunk hash and 8 for a bin hash.
pub fn format_hash(table: NameTable, hash: u64) -> String {
    match table {
        NameTable::Chunks => format!("0x{hash:016x}"),
        _ => format!("0x{hash:08x}"),
    }
}

/// Names the hashes of hits and nodes for the extension.
pub struct Namer<'a> {
    pub game: &'a GameIndex,
    pub search: &'a GameSearch,
}

impl Namer<'_> {
    pub fn named(&self, table: NameTable, hash: u64) -> Named {
        Named {
            hash: format_hash(table, hash),
            name: self.search.name(table, hash).map(|name| name.into_owned()),
        }
    }

    pub fn object_ref(&self, declaration: &Declaration) -> ObjectRef {
        ObjectRef {
            object: self.named(NameTable::Entries, u64::from(declaration.object.0)),
            class: self.named(NameTable::Types, u64::from(declaration.class.0)),
            chunk: self.named(NameTable::Chunks, declaration.chunk.0),
            archive: self.game.archive(declaration.archive).name.clone(),
            is_override: declaration.kind == ChunkKind::Patch,
        }
    }

    pub fn search_result(&self, result: ltk_game_index::SearchResult) -> SearchResult {
        let hits = result
            .hits
            .into_iter()
            .map(|hit| {
                let place = self.object_ref(&hit.declaration);
                Hit {
                    declaration: hit.id.into(),
                    object: place.object,
                    class: place.class,
                    chunk: place.chunk,
                    archive: place.archive,
                    is_override: place.is_override,
                    matches: hit
                        .matches
                        .into_iter()
                        .map(|m| Match {
                            text: match m.term {
                                MatchedTerm::Hash { hash, name } => name.unwrap_or_else(|| {
                                    format_hash(m.kind.table().unwrap_or(NameTable::Hashes), hash)
                                }),
                                MatchedTerm::String(text) => text,
                                _ => String::new(),
                            },
                            kind: m.kind,
                        })
                        .collect(),
                }
            })
            .collect();
        SearchResult {
            kinds: result.counts,
            total: result.total,
            offset: result.offset,
            truncated: result.truncated,
            hits,
        }
    }

    pub fn listing(&self, listing: ltk_game_index::Listing) -> Listing {
        let entries = listing
            .entries
            .into_iter()
            .filter_map(|entry| match entry {
                CrateEntry::Dir {
                    name, objects, own, ..
                } => Some(TreeEntry::Dir {
                    name,
                    objects,
                    object: own.and_then(|node| self.node_ref(&node)),
                }),
                CrateEntry::Object(node) => Some(TreeEntry::Object {
                    object: self.node_ref(&node)?,
                    name: node.name,
                }),
                CrateEntry::Unnamed { objects } => Some(TreeEntry::Dir {
                    name: UNNAMED_DIR.to_owned(),
                    objects,
                    object: None,
                }),
                _ => None,
            })
            .collect();
        Listing {
            entries,
            total: listing.total,
            truncated: listing.truncated,
        }
    }

    /// The node's first declaration, in storage order.
    fn node_ref(&self, node: &ObjectNode) -> Option<ObjectRef> {
        let first = node.declarations.first()?;
        let declaration = self.search.terms().objects().row(*first)?;
        Some(self.object_ref(declaration))
    }
}
