//! Chunk documents: a chunk of the game served as read-only ritobin text.

use std::{
    fs::File,
    io::{BufReader, Cursor},
};

use ltk_file::LeagueFileKind;
use ltk_game_index::{GameIndex, WadHash};
use ltk_meta::{Bin, BinOverride};
use ltk_ritobin::{
    cst::Cst,
    print::{Print, PrintConfig},
};
use ltk_wad::{Wad, WadError};
use serde::{Deserialize, Serialize};

use ritobin_lsp::line_ends::LineNumbers;

use crate::server::BinHashProvider;

/// One root entry of a chunk document and the line it starts on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryLine {
    /// `0x`-prefixed hex of the entry's path hash.
    pub object: String,
    /// 0-based.
    pub line: u32,
}

/// A chunk rendered as ritobin text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChunkDocument {
    pub text: String,
    pub is_override: bool,
    pub entries: Vec<EntryLine>,
}

/// Why a chunk did not render.
#[derive(Debug, thiserror::Error)]
pub enum ChunkError {
    #[error(transparent)]
    Read(#[from] ReadChunkError),
    #[error("chunk {0} is not a bin")]
    NotABin(WadHash),
    #[error("chunk {hash} failed to decode: {source}")]
    Decode {
        hash: WadHash,
        #[source]
        source: ltk_meta::Error,
    },
    #[error("chunk {hash} failed to print: {source}")]
    Print {
        hash: WadHash,
        #[source]
        source: ltk_ritobin::print::PrintError,
    },
}

/// A few lines of one object's ritobin text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    /// 0-based line of the object's entry in the chunk document.
    pub line: u32,
    pub text: String,
    /// Whether the object runs past `text`.
    pub truncated: bool,
}

impl ChunkDocument {
    /// The first `max_lines` lines of `object`'s entry, or `None` when the document has no
    /// entry at that hash.
    pub fn preview(&self, object: &str, max_lines: usize) -> Option<Preview> {
        let wanted = object.to_ascii_lowercase();
        let at = self
            .entries
            .iter()
            .position(|e| e.object.eq_ignore_ascii_case(&wanted))?;
        let start = self.entries[at].line as usize;
        // An entry runs to the line before the next entry, or to the closing brace of the
        // entries map at the end of the document.
        let end = self
            .entries
            .get(at + 1)
            .map(|e| e.line as usize)
            .unwrap_or_else(|| self.text.lines().count().saturating_sub(1));
        let lines: Vec<&str> = self.text.lines().skip(start).take(end - start).collect();
        let truncated = lines.len() > max_lines;
        let text = lines[..lines.len().min(max_lines)].join("\n");
        Some(Preview {
            line: start as u32,
            text,
            truncated,
        })
    }
}

/// Reads `hash` from its first holder and prints it with `hashes`.
pub fn render(
    game: &GameIndex,
    hash: WadHash,
    hashes: BinHashProvider,
) -> Result<ChunkDocument, ChunkError> {
    let bytes = read_chunk(game, hash)?;
    let config = PrintConfig::default().with_hashes(hashes);
    let decode = |source| ChunkError::Decode { hash, source };
    let print = |source| ChunkError::Print { hash, source };
    let (text, is_override) = match LeagueFileKind::identify_from_bytes(&bytes) {
        LeagueFileKind::PropertyBin => {
            let bin = Bin::from_reader(&mut Cursor::new(&bytes)).map_err(decode)?;
            (bin.print_with_config(config).map_err(print)?, false)
        }
        LeagueFileKind::PropertyBinOverride => {
            let patch = BinOverride::from_reader(&mut Cursor::new(&bytes)).map_err(decode)?;
            (patch.print_with_config(config).map_err(print)?, true)
        }
        _ => return Err(ChunkError::NotABin(hash)),
    };
    let entries = entry_lines(&text);
    Ok(ChunkDocument {
        text,
        is_override,
        entries,
    })
}

/// The line of every root entry of `text`.
fn entry_lines(text: &str) -> Vec<EntryLine> {
    let cst = Cst::parse(text);
    let ast = cst.build_ast(text);
    let lines = LineNumbers::new(text);
    ast.root_entries()
        .map(|entry| EntryLine {
            object: format!("0x{:08x}", entry.path_hash.value.0),
            line: lines.line_number(entry.span().start),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_preview_runs_to_the_next_entry() {
        let text = "#PROP_text\ntype: string = \"PROP\"\nversion: u32 = 3\nlinked: list[string] = {}\nentries: map[hash,embed] = {\n    0x00000001 = Foo {\n        a: u32 = 1\n    }\n    \"bar\" = Baz {}\n}\n";
        let doc = ChunkDocument {
            text: text.to_owned(),
            is_override: false,
            entries: entry_lines(text),
        };
        let first = doc.preview("0x00000001", 10).unwrap();
        assert_eq!(first.line, 5);
        assert_eq!(
            first.text,
            "    0x00000001 = Foo {\n        a: u32 = 1\n    }"
        );
        assert!(!first.truncated);
        let cut = doc.preview("0x00000001", 1).unwrap();
        assert_eq!(cut.text, "    0x00000001 = Foo {");
        assert!(cut.truncated);
        let last = doc.preview(&doc.entries[1].object, 10).unwrap();
        assert_eq!(last.text, "    \"bar\" = Baz {}");
        assert!(doc.preview("0x0000ffff", 10).is_none());
    }

    #[test]
    fn every_root_entry_reports_its_line() {
        let text = "#PROP_text\ntype: string = \"PROP\"\nversion: u32 = 3\nlinked: list[string] = {}\nentries: map[hash,embed] = {\n    0x00000001 = Foo {\n        a: u32 = 1\n    }\n    \"bar\" = Baz {}\n}\n";
        let lines = entry_lines(text);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].object, "0x00000001");
        assert_eq!(lines[0].line, 5);
        assert_eq!(lines[1].line, 8);
    }
}

/// Reads one chunk of `game` in full from its first holder.
pub fn read_chunk(game: &GameIndex, hash: WadHash) -> Result<Vec<u8>, ReadChunkError> {
    let row = game.row(hash).ok_or(ReadChunkError::Unknown(hash))?;
    let path = &game.archive(row.first_holder()).path;
    let file = File::open(path).map_err(|source| ReadChunkError::Open {
        path: path.to_string(),
        source,
    })?;
    let mut wad = Wad::mount(BufReader::new(file)).map_err(|source| ReadChunkError::Wad {
        path: path.to_string(),
        source,
    })?;
    let chunk = *wad
        .chunks()
        .get(hash)
        .ok_or(ReadChunkError::Unknown(hash))?;
    wad.load_chunk_decompressed(&chunk)
        .map(Vec::from)
        .map_err(|source| ReadChunkError::Wad {
            path: path.to_string(),
            source,
        })
}

/// Why a chunk did not read.
#[derive(Debug, thiserror::Error)]
pub enum ReadChunkError {
    #[error("chunk {0} is not in the game index")]
    Unknown(WadHash),
    #[error("failed to open '{path}': {source}")]
    Open {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to read '{path}': {source}")]
    Wad {
        path: String,
        #[source]
        source: WadError,
    },
}
