//! The Mimir tables as the game index reads them: a name source for the sheets and a chunk
//! resolver for the build's read-or-sniff policy.

use std::sync::Arc;

use ltk_game_index::{NameSource, NameTable, ResolveWadPath, WadHash};
use ltk_hashdb::HashDb;
use ltk_mimir_cache::{Manifest, Table};

use crate::server::Hashes;

/// The Mimir table behind each name table.
pub fn mimir_table(table: NameTable) -> Table {
    match table {
        NameTable::Entries => Table::BinEntries,
        NameTable::Types => Table::BinTypes,
        NameTable::Fields => Table::BinFields,
        NameTable::Hashes => Table::BinHashes,
        _ => Table::Game,
    }
}

/// The Mimir tables, versioned by the manifest's checksum of each.
pub struct MimirNames<'a> {
    pub hashes: &'a Hashes,
    /// `None` versions every table as `None`: each sheet is cut on every build.
    pub manifest: Option<Manifest>,
}

impl NameSource for MimirNames<'_> {
    fn version(&self, table: NameTable) -> Option<String> {
        let entry = self.manifest.as_ref()?.entry(mimir_table(table))?;
        Some(entry.sha256.clone())
    }

    fn for_each_name(&self, table: NameTable, visit: &mut dyn FnMut(u64, &str)) {
        let Some(db) = self.hashes.table(mimir_table(table)) else {
            return;
        };
        for (hash, name) in db.iter() {
            visit(hash, name.as_ref());
        }
    }
}

/// Chunk paths from the Mimir `game` table.
pub struct ChunkPaths(pub Arc<HashDb>);

impl ResolveWadPath for ChunkPaths {
    fn for_each_named(&self, hashes: &[WadHash], visit: &mut dyn FnMut(usize, &str)) {
        for (index, hash) in hashes.iter().enumerate() {
            if let Some(path) = self.0.get(hash.0) {
                visit(index, &path);
            }
        }
    }
}
