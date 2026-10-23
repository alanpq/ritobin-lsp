# Game search in `ritobin-lsp`

The API spec for whole-game search in the server: how it builds and holds `ltk_game_index`'s
search over a League of Legends installation, the requests that query it, and the VS Code surface
that shows the results. The index itself, its terms, its query resolution and its object tree are
specified in league-mod's [`game-search.md`](https://github.com/LeagueToolkit/league-mod/blob/main/docs/design/game-search.md).

**This document states what is true.** Where the code and a section here disagree, the section
is the bug and gets edited.

The server side lives in `crates/ritobin-lsp/src/game/`; the extension side in
`editors/code/src/game_search.ts` and `editors/code/src/wad_fs.ts`.

## <a id="s1"></a>1. Summary

The server indexes every bin object of one installation with the `search` feature of
`ltk_game_index` and answers a substring query with the objects that mention it: as their own
name, their class, a class nested in them, a field, a string value, a hash value, an object link
or a wad chunk link. A hit opens the chunk that declares the object as read-only ritobin text,
positioned at the object.

The term index stores hashes only. Names come from the Mimir hash tables the server keeps, cut
into the crate's name sheets. A query resolves to hashes through the sheets, then to declarations
through the term index (D1, ADR-0003). A hash table update recuts the sheets without touching the
term index.

```text
query "aatrox/skins/skin0"   -> entries sheet: Characters/Aatrox/Skins/Skin0  (0x1a2b3c4d)
                             -> objects whose path, link or hash value is 0x1a2b3c4d
query "assets/characters/aatrox/skins/base/aatrox.skn"
                             -> strings sheet: term #88213
                             -> objects holding that string value
query "4f2e7d11"             -> every kind at that hash, and the text scan as well
```

## <a id="s2"></a>2. Vocabulary

The vocabulary of league-mod's [`game-search.md` section 2](https://github.com/LeagueToolkit/league-mod/blob/main/docs/design/game-search.md#s2) applies: term, term kind,
declaration, declaration id, posting, term index, sheet, name sheets, matches, hit, object tree.
In addition:

- **game directory** - the `Game` directory of an installation. It holds `DATA/FINAL`.
  Setting `ritobin-lsp.gameDirectory`.
- **ready set** - the chunk index and the `GameSearch` over it, built together and swapped in
  together. Type `Ready`.
- **chunk document** - a chunk's ritobin text served under the `ritobin-wad` scheme.

The term kinds the requests carry are the crate's `TermKind`, in lowercase: `object`, `class`,
`embedded`, `field`, `hash`, `link`, `wad`, `string`.

## <a id="s3"></a>3. Evidence

The corpus is one installation, measured 2026-10-23 (appendix A): 394 archives, 921,530 chunks,
50,550 bin chunks, 466,863 objects, 2.4 GB of decoded bin, 8.3 GB of rendered ritobin text. The
term index over it is league-mod's [`game-search.md` section 3](https://github.com/LeagueToolkit/league-mod/blob/main/docs/design/game-search.md#s3).

## <a id="s4"></a>4. Configuration

| Setting | Type | Default | Meaning |
| --- | --- | --- | --- |
| `ritobin-lsp.gameDirectory` | string | `""` | The game directory. Empty probes the platform default install path. |
| `ritobin-lsp.gameSearch.enabled` | boolean | `true` | Whether the server builds the index at all. |
| `ritobin-lsp.gameSearch.workers` | number | `0` | Walk threads. `0` is the number of CPUs. |

The settings reach the server in `initializationOptions`, as every other setting does. A change to
any of the three prompts for a server restart in the extension (D2). `InitOptions` carries them
with `#[serde(default)]`. The environment variable `RB_GAME_DIR` takes precedence over the setting
(D10).

The platform default install path is `C:\Riot Games\League of Legends\Game` on Windows,
`/Applications/League of Legends.app/Contents/LoL/Game` on macOS, and nothing on Linux. A missing
directory leaves the index `Unconfigured` and is not an error.

## <a id="s5"></a>5. Index

### <a id="s5.1"></a>5.1 Structures

The ready set holds a `GameIndex` and a `GameSearch`: a `TermIndex` and `NameSheets`. Their
shapes, building, caches and queries are league-mod's
[`game-search.md` section 6](https://github.com/LeagueToolkit/league-mod/blob/main/docs/design/game-search.md#s6) to [section 9](https://github.com/LeagueToolkit/league-mod/blob/main/docs/design/game-search.md#s9).

### <a id="s5.2"></a>5.2 Mimir adapters

`game::names` holds the two adapters between the Mimir tables and the crate:

- `MimirNames` is the `NameSource`. `entries`, `types`, `fields`, `hashes` and `chunks` read the
  Mimir tables `binentries`, `bintypes`, `binfields`, `binhashes` and `game`. A table's version
  is its sha256 in the Mimir manifest. Without a manifest every version is `None`, and every
  sheet is cut on every build.
- `ChunkPaths` is the `ResolveWadPath` over the `game` table, for the build's read-or-sniff
  policy.

### <a id="s5.3"></a>5.3 Files

Everything lives under `<cache dir>/game-search/`, the same cache dir that holds the meta dump:

| File | Content | Keyed by |
| --- | --- | --- |
| `chunks.bin` | the chunk index cache | the game fingerprint |
| `names.bin` | the name sheets | each table's manifest sha256; for `chunks`, the game fingerprint as well |
| `terms.bin` | the term index cache | the game fingerprint |

A build removes `objects.bin`, `strings.txt`, `strings.val` and every `names-*` file in the
directory. No code reads them.

## <a id="s6"></a>6. Build

### <a id="s6.1"></a>6.1 Lifecycle

The build is one background task started after the hash tables load, and restarted when they
update. Its state is `IndexState`:

```rust
pub enum IndexState {
    Unconfigured,          // no game directory
    Building { stage: Stage, done: u32, total: u32 },   // Chunks | Names | Walk | Save
    Ready(IndexStats),
    Failed { message: String },
}
```

`ServerStatus` carries a `game_index: TaskStatus` field so the status bar spinner covers the build.
The server also sends the notification `ritobin-lsp/gameIndexState` with the full `IndexState` on
every change; the extension's panel renders it.

A restart with a different game directory or a newer table set calls off a running build through
`BuildOptions::called_off`, bound to a build generation and polled once per archive. A build
called off leaves the previous ready set in place.

### <a id="s6.2"></a>6.2 Pipeline

1. **Chunk index.** `GameIndex::load_or_build(game_dir, chunks.bin)`. Skipped archives are logged
   and never fail the build.
2. **Name sheets.** `NameSheets::load_or_build(names.bin, MimirNames, game)`. Without hash tables
   the sheets are empty.
3. **Term index.** `TermIndex::load_for(terms.bin, game)`. On any cache error the walk runs:
   `TermIndex::build_with` with `ChunkPaths` as the resolver, `workers` threads, the build
   generation as `called_off`, and `on_archive` reporting `Walk` with the archives done. The
   result is saved to `terms.bin`, stage `Save`; a failed save is a `warn`.
4. **Search.** `GameSearch::new(terms, names)` and `prepare`, which cuts the lookups, the scan
   tables, the standing order and the sorted path list before the ready set is swapped in.

### <a id="s6.3"></a>6.3 Stats

```rust
pub struct IndexStats {
    pub archives: u32,       // ObjectStats::archives
    pub chunks: u32,         // GameIndex::len
    pub bins: u32,           // ObjectStats::bins
    pub declarations: u32,   // ObjectStats::declarations
    pub hash_terms: u32,     // TermStats::hash_terms
    pub string_terms: u32,   // TermStats::string_terms
    pub failed_chunks: u32,  // TermStats::failed_chunks + ObjectStats::skipped_chunks
    pub elapsed: u64,        // milliseconds of the walk
}
```

## <a id="s7"></a>7. Requests

All methods are server-wide and answered off the main loop through `respond_blocking`. A request
against an index that is not `Ready` fails with `RequestFailed` and the state's message.

### <a id="s7.1"></a>7.1 `ritobin-lsp/gameSearch`

```rust
pub struct GameSearchParams {
    pub query: String,
    #[serde(default)] pub kinds: Vec<TermKind>,   // empty is every kind
    #[serde(default)] pub limit: Option<u32>,     // None is 500
    #[serde(default)] pub offset: u32,            // first hit of the page
}
pub struct GameSearchResult {
    pub kinds: Vec<KindCount>, // declarations matching through each kind, every kind scanned
    pub total: u32,            // matching declarations through the requested kinds
    pub offset: u32,
    pub truncated: bool,       // hits exist past this page
    pub hits: Vec<Hit>,
}
pub struct KindCount { pub kind: TermKind, pub count: u32 }
pub struct Hit {
    pub declaration: u32,
    pub object: Named,     // hash + name from the entries sheet
    pub class: Named,      // hash + name from the types sheet
    pub chunk: Named,      // hash + path from the chunks sheet
    pub archive: String,   // DATA/FINAL-relative archive name
    pub is_override: bool,
    pub matches: Vec<Match>,
}
pub struct Named { pub hash: String, pub name: Option<String> }  // hash is 0x-prefixed hex
pub struct Match { pub kind: TermKind, pub text: String }
```

Resolution is `GameSearch::search` with the query's text, kinds (every kind when empty), offset
and limit: league-mod's [`game-search.md` section 8.2](https://github.com/LeagueToolkit/league-mod/blob/main/docs/design/game-search.md#s8.2) and
[section 8.3](https://github.com/LeagueToolkit/league-mod/blob/main/docs/design/game-search.md#s8.3). The server maps the crate's result:

1. `kinds` is the crate's `counts`, one per kind, every kind scanned whatever `kinds` asked for.
2. A hit's `object`, `class` and `chunk` are named through `GameSearch::name` over the
   `entries`, `types` and `chunks` sheets, as the tables write them; `archive` is the archive's
   `DATA/FINAL`-relative name; `isOverride` is a `PTCH` declaration.
3. A match's `text` is the term's name, its lowercased string, or the hash in hex for a hash no
   sheet names.

A `Field` match is a property name. The query `mname` matches a large share of the corpus through
`fields`; the limit and `kinds` are how a client keeps that useful (D4).

### <a id="s7.2"></a>7.2 `ritobin-lsp/deserializeWadChunk`

```rust
pub struct DeserializeWadChunkParams { pub chunk: String }   // 0x-prefixed hex WadHash
pub struct DeserializeWadChunkResult {
    pub text: String,
    pub is_override: bool,
    pub entries: Vec<EntryLine>,   // every root entry with its 0-based line
}
pub struct EntryLine { pub object: String, pub line: u32 }
```

The chunk is read from its first holder in the chunk index, decoded as `Bin` or `BinOverride` by
magic, and printed with the server's `BinHashProvider`, exactly as `deserializeBin` prints a file.
The entry lines come from the CST the printer builds. A chunk the index does not hold, or one that
is not a bin, is `RequestFailed`.

### <a id="s7.3"></a>7.3 `ritobin-lsp/gameObjectPreview`

```rust
pub struct GameObjectPreviewParams { pub chunk: String, pub object: String, pub max_lines: Option<u32> }
pub struct Preview { pub line: u32, pub text: String, pub truncated: bool }   // None: no such entry
```

The first `max_lines` lines (default 12) of the object's entry in the rendered chunk, from its
entry line to the line before the next entry. The chunk renders as in
[section 7.2](#s7.2); the server keeps the last rendered chunk, and a preview, a resolve and
an open of the same chunk render it once (D11).

### <a id="s7.4"></a>7.4 `workspace/symbol` and `workspaceSymbol/resolve`

The server advertises `workspaceSymbolProvider` with `resolveProvider`. A `workspace/symbol`
query is a search over the `Object` and `Class` kinds, capped at 200 symbols; the editor filters
and ranks them. Each symbol is the object's name (or hex) of kind `Class`, its container the
class name and chunk path joined by ` · `, and its location the chunk document's uri without a
range. `data` carries the chunk and object hashes. `workspaceSymbol/resolve` renders the chunk
and returns the symbol with its entry line as a zero-width range. The editor's "go to symbol in
workspace" (Ctrl+T) runs over the whole game through this pair, in any client that speaks LSP
3.17.

### <a id="s7.5"></a>7.5 `ritobin-lsp/gameBrowse`

```rust
pub struct GameBrowseParams { pub dir: Option<String> }      // an object path prefix; "" or absent is the root
pub struct Listing { pub entries: Vec<TreeEntry>, pub total: u32, pub truncated: bool }
pub enum TreeEntry {
    Dir { name: String, objects: u32, object: Option<ObjectRef> },
    Object { name: String, #[serde(flatten)] object: ObjectRef },
}
pub struct ObjectRef { pub object: Named, pub class: Named, pub chunk: Named, pub archive: String, pub is_override: bool }
```

One level of the installation's object tree, `GameSearch::list` without folding: league-mod's
[`game-search.md` section 9](https://github.com/LeagueToolkit/league-mod/blob/main/docs/design/game-search.md#s9). One object is one entry, whichever chunks declare it.
A `Dir` counts the objects anywhere beneath it; a path that is an object and the parent of others
is one `Dir` row carrying that object. An `ObjectRef` describes the object's first declaration in
storage order. The crate's `TreeEntry::Unnamed` is the `Dir` named `<unnamed>` at the root, and a
`dir` of `<unnamed>` lists it, by hex. A listing holds at most 2,000 entries and says how many
there are; search reaches the rest (D14, D17).

### <a id="s7.6"></a>7.6 `ritobin-lsp/rebuildGameIndex`

No params, `()` result. Deletes `terms.bin` and restarts the build. The chunk index and name
sheets are reused when their keys match.

### <a id="s7.7"></a>7.7 `ritobin-lsp/gameIndexState`

Server-to-client notification carrying `IndexState`, sent on every change and once in reply to the
client's `initialized` notification.

## <a id="s8"></a>8. Extension surface

### <a id="s8.1"></a>8.1 Chunk documents

Scheme `ritobin-wad`. A URI is `ritobin-wad://<hex>/<display>.rito`, where `<hex>` is the
0x-less chunk hash and `<display>` is the resolved chunk path with `/` kept, or the hex again for an
unnamed chunk. The authority is the key; the path is for the tab title only.

The provider is a read-only `FileSystemProvider`. `readFile` sends `deserializeWadChunk` for the
authority and caches the result per URI until the document closes. `writeFile` throws
`NoPermissions`. The document selector of the language client includes
`{ scheme: "ritobin-wad", language: "ritobin" }`, so diagnostics, hover and symbols work on a chunk
document as on any other.

### <a id="s8.2"></a>8.2 Game Search panel

A webview view with id `ritobin-lsp.gameSearch` in the `ritobin-game` activity bar container.
The page is `editors/code/media/game-search.{js,css}`; the provider in
`editors/code/src/game_search.ts` relays its messages to the server.

The panel, top to bottom:

- an input; typing searches after a 180 ms pause, Enter searches at once, Escape clears;
- one chip per term kind, `embedded` included, with the count from `kinds`, toggled to filter;
  no chip on is every kind;
- a status line: the index state with a progress bar while walking, the result count, or the
  link to the setting when unconfigured;
- the results, in one shape whatever kinds are selected: chunk, then the parent object path
  as a collapsible folder, then one row per object, then that object's matched terms;
- a trailing "Load N more" row that also loads when scrolled into view.

An object row is the kind icon of its first match, the object's last path segment, and the
class name. Matched terms other than the object's own path and class follow on a second,
dimmer line: two inline, the rest listed one per line under the object behind a chevron. A term
that only repeats the object's path is dropped. The query is highlighted in text colour alone,
wherever it appears.

A row opens its object on click or Enter, previews it on Space or the eye button (the first 12
lines of its entry, fetched through `gameObjectPreview`), and copies its name on the copy
button or Ctrl+C. Arrow keys move the selection; Escape returns to the input. The last query
and filter persist across reloads.

With an empty query the panel shows the installation's objects as one tree through
`gameBrowse`: folders of object paths with filled folder icons and their object counts, objects
with filled cube icons, their class in the editor's type colour and their chunk's file name
dimmed, each level fetched on first expansion. A folder that is itself an object shows its class
and opens on double click or its button; an object opens on click. A bin opens on its button or a double click; an object
opens on click. The tree empties whenever the index leaves `Ready`.

Colours: icons take the theme's symbol palette; text takes the editor's token palette, the
Dark+ and Light+ values ritobin files are highlighted with, so a class name is the editor's
type colour, a string value the string colour, a hash the number colour, a link the variable
colour. The query match takes the list highlight foreground native trees use. Hover and
selection take the list colours.

### <a id="s8.3"></a>8.3 Commands

| Command | Title | Behaviour |
| --- | --- | --- |
| `ritobin-lsp.gameSearch` | Ritobin: Search Game (Ctrl+Shift+Alt+F) | Focuses the panel; a single-line editor selection becomes the query. |
| `ritobin-lsp.rebuildGameIndex` | Ritobin: Rebuild Game Index | Sends `rebuildGameIndex`; also the panel's refresh button and the view title button. |
| `ritobin-lsp.openGameChunk` | internal | Opens a chunk document at an object. |

## <a id="s9"></a>9. Errors

| Condition | Where | Outcome |
| --- | --- | --- |
| No game directory, no default | build | `IndexState::Unconfigured`; requests fail with "game directory is not set" |
| `DATA/FINAL` missing | build | `Failed(BuildError::MissingDataFinal)` |
| An archive fails to mount | build | skipped, counted, logged at `warn` |
| A chunk fails to read | build | skipped, counted in `failed_chunks` |
| Cache file unreadable | build | rebuilt, logged at `warn` |
| Request while building | request | `RequestFailed`, "game index is building (<stage> <done>/<total>)" |
| Unknown chunk in `deserializeWadChunk` | request | `RequestFailed`, "chunk <hex> is not in the game index" |
| Chunk is not a bin | request | `RequestFailed`, "chunk <hex> is not a bin" |

## <a id="s10"></a>10. Testing

The term walk, the sheets, the term index, the name sheets, query resolution and the object tree
are tested in `ltk_game_index`. Here:

- **Chunk uris.** A chunk uri has the shape the extension builds, named and unnamed.
- **Chunk documents.** `deserializeWadChunk` on a PROP and a PTCH chunk returns the same text as
  `deserializeBin` on the extracted file, with one `EntryLine` per root entry.
- **Installation.** A stdio driver runs `initialize` against a real installation, waits for
  `Ready`, and sends every request of [section 7](#s7); appendix A holds its figures.

Tests sit inline as `#[cfg(test)] mod tests` with sentence names, as elsewhere in the crate.

## <a id="s11"></a>11. Rules

| ID | Rule | Instead of | Why | Spec |
| --- | --- | --- | --- | --- |
| D1 | The term index stores hashes; names resolve through the crate's sheets, cut from Mimir. | Resolved names baked into the index. | A table update recuts the sheets, not a walk. | [section 1](#s1), ADR-0003 |
| D2 | A game-search setting change prompts a server restart. | Live reconfiguration. | Settings reach the server once, in `initializationOptions`. | [section 4](#s4) |
| D3 | Substring over a sheet, not a tokenized index. | fst or tantivy. | The query shape is "find in files"; the crate scans sheets. | [section 7.1](#s7.1), ADR-0001 |
| D4 | Results are capped by `limit`, default 500, with `total` reported. | Unbounded results. | `Field` terms post to most of the corpus. | [section 7.1](#s7.1) |
| D5 | Numeric values are not terms. | Indexing every number. | The crate's term walk reads no number. | [section 6.2](#s6.2) |
| D6 | `ltk_game_index` is used with its `search` feature. | A term walk and index of the server's own. | One walk, one cache and one query resolution serve the server and LTK Manager. | [section 5](#s5), ADR-0003 |
| D7 | Chunk documents are read-only. | Write-back into the archive. | A WAD is the game's own file. | [section 8.1](#s8.1) |
| D8 | The chunk read is the first holder. | Every holder. | Holders carry one checksum per chunk; the first is as good as any. | [section 7.2](#s7.2) |
| D9 | The `chunks` sheet keeps only chunks the installation holds. | The whole table. | The crate cuts it that way; the table is 2.3 M paths against 0.9 M chunks. | [section 5.3](#s5.3) |
| D10 | `RB_GAME_DIR` overrides the setting. | Setting only. | A server started outside VS Code, or a test, points at an install without a client. | [section 4](#s4) |
| D11 | The server keeps the last rendered chunk. | Rendering per request. | A resolve, an open and a preview of one object arrive within a second of each other. | [section 7.3](#s7.3) |
| D12 | `workspace/symbol` searches `Object` and `Class` only. | Every kind. | The editor's symbol picker is a name picker; strings and hashes belong in the panel. | [section 7.4](#s7.4) |
| D13 | Search is a webview panel in its own container, not a tree view. | A `TreeView`. | A tree has no input, no colours beyond the icon, and no previews. | [section 8.2](#s8.2), ADR-0002 |
| D14 | The search is prepared once per ready set, before it is swapped in. | Cutting lookups and the path list on the first request. | The first search and the first listing answer as fast as the rest. | [section 6.2](#s6.2) |
| D16 | The tree is the object namespace, not the archives' file paths. | A file tree of chunks. | The user reads and edits objects; a chunk is where an object happens to be stored. | [section 7.5](#s7.5) |
| D17 | A listing holds at most 2,000 entries. | Unbounded listings. | The unnamed bucket holds 48 k objects; a tree row is for browsing, search is for finding. | [section 7.5](#s7.5) |
| D15 | Results have one shape in every mode; no grouping by kind. | Kind sections when kinds mix. | The chips filter by kind and the icon names it; a second grouping axis changed the depth from chunk to chunk. | [section 8.2](#s8.2) |

## <a id="appendix-a"></a>Appendix A. Measurements

Taken 2026-10-23 on one installation at `C:\Riot Games\League of Legends\Game`, read over a WSL
mount, 16 threads, with `ltk_wad` 0.5.10, `ltk_meta` 0.9.1 and `ltk_ritobin` 0.12.0. Mimir
manifest of 2026-10-19.

| Metric | Value |
| --- | --- |
| archives | 394 |
| chunks | 921,530 |
| bin chunks (PROP + PTCH) | 50,550 (235 PTCH) |
| objects | 466,863 |
| decoded bin bytes | 2.4 GB |
| rendered ritobin text, hashes unresolved | 8.3 GB, 186 M lines |
| read, decode and print, wall time | 82 s |
| `game` table entries | 2,324,172 |
| `binentries` / `binhashes` / `binfields` / `bintypes` entries | 444,169 / 418,634 / 10,198 / 3,871 |

The server's own term walk and index, before ADR-0003, the same installation and machine, first
build with an empty cache:

| Metric | Value |
| --- | --- |
| archives walked (first holders) / chunks after dedup by hash | 384 / 824,375 |
| bins / declarations | 44,521 / 447,791 |
| distinct hash terms / string terms | 1,459,617 / 1,566,481 |
| chunks failing to decode | 3 |
| walk, 16 threads | 25.8 s |
| chunk index plus name layer plus walk, from `initialize` to `Ready` | 43.5 s |
| `objects.bin` / strings sheet / `game` sheet before D9 | 202 MB / 100 MB / 211 MB |
| `aatrox`, every kind, 6,438 objects | 45 ms |
| `aatrox`, `Object` only, 2,062 objects | 4 ms |
| `mname`, `Field` only, 25,376 objects | 6 ms |
| one 900 KB chunk document, 44 entries | 285 ms |

The server on `ltk_game_index` 0.2.0, same installation and machine, 2026-10-24, release build,
16 threads, warm file cache, driven over stdio:

| Metric | Value |
| --- | --- |
| bins / declarations | 44,524 / 447,791 |
| distinct hash terms / string terms | 1,459,635 / 1,566,481 |
| chunks that do not read | 3 |
| from `initialize` to `Ready`, empty term cache | 19.5 s |
| from `initialize` to `Ready`, every cache present | 3.9 s |
| `names.bin` / `terms.bin` | 153 MB / 288 MB |
| `gameSearch`, the queries above and others | 6 ms to 9 ms |
| `gameBrowse` root / `Characters/Aatrox/Skins` | 0 ms / 1 ms |
| `workspace/symbol` `Aatrox/Skins/Skin0`, 168 symbols | 9 ms |
