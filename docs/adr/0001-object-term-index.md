# ADR-0001: Object term index

- **Status:** Superseded by [ADR-0003](0003-search-in-ltk-game-index.md)
- **Date:** 2026-10-23
- **Crates:** `ritobin-lsp`
- **Related:** the spec sections that state the rules:
  `docs/design/game-search.md` [section 1](../design/game-search.md#s1),
  [section 5](../design/game-search.md#s5) and [section 6](../design/game-search.md#s6)

## Context and problem statement

The extension needs "find in files" over the whole game: a substring typed in VS Code, and every
bin object that mentions it, whether as a name, a string value, a hash or a link. The corpus is
466,863 objects in 50,550 bin chunks across 394 archives, 2.4 GB decoded. Rendered as ritobin text
with no names resolved it is 8.3 GB (spec appendix A).

`ltk_game_index` catalogs chunks by hash and, with its `objects` feature, records which chunk
declares which object. It stores no names and decodes no value. Any term beyond a declaration
comes from reading the bins.

Names live in the Mimir hash tables the server already loads and updates. A rendered name depends
on the table version; a hash does not.

This decision adds `ltk_game_index`, `ltk_wad`, `rayon`, `rmp-serde` and `memchr` to the server's
dependencies.

## Decision drivers

- Substring semantics: `aatrox` matches `Characters/Aatrox/Skins/Skin0` and `ASSETS/.../aatrox.skn`.
- Query latency under a second on a laptop; a build under two minutes.
- A hash table update must not force a full rebuild.
- Disk and memory in the hundreds of megabytes at most.
- Builds on musl Linux, MSVC Windows and macOS, as the rest of the server does.

## Considered options

1. **Object term index** - walk every object once, record `(kind, hash)` and string terms per
   declaration as postings, resolve names through the Mimir tables at query time.
2. **Stored text corpus** - keep every object's rendered ritobin text zstd-compressed and scan it
   with a regex per query.
3. **tantivy** - a tokenized inverted index over the rendered text.
4. **On-demand decode** - use the crate's `objects` feature for declarations and decode chunks at
   query time.

## Decision

**The server builds an object term index keyed by hashes and resolves names through sheets built
from the Mimir tables.** The rules are in the spec, [section 5](../design/game-search.md#s5) and
[section 6](../design/game-search.md#s6).

A declaration posts under every hash term and string term it holds. A query scans the name sheets
and the strings sheet for the substring, takes the postings of each match, and unions them.

## Consequences

- **Positive:** the heavy walk runs once per installation state. A table update rebuilds sheets of
  tens of megabytes from `HashDb::iter()` in seconds. The index holds no text, so it stays near
  100 MB. Substring is exact and case-insensitive by construction.
- **Negative:** numbers, booleans and vectors are not searchable. A query cannot express structure
  ("objects whose `mSkinName` is X"). A `Field` term posts to most of the corpus and needs the
  result cap. Five crates join the dependency tree.
- **Revisit when:** a consumer needs structured value queries; the strings sheet or postings
  outgrow a few hundred megabytes on a real installation.

## Pros and cons of the options

### Object term index

- Good: index independent of table versions; small; query is a memory scan plus binary searches.
- Bad: no numeric or regex search; the walk is the server's own code to maintain.

### Stored text corpus

- Good: true full text and regex; nothing to design beyond a blob store.
- Bad: the text embeds resolved names, so a table update is a 90 s rebuild or stale names; 471 MB
  compressed on disk; a query decompresses and scans 8.3 GB, one to four seconds on a desktop and
  longer on a laptop.

### tantivy

- Good: millisecond queries; phrase, fuzzy and boolean queries for free.
- Bad: token semantics, not substring, unless an n-gram field triples the index; the text carries
  resolved names with the same staleness problem; an index of several gigabytes; a dependency tree
  of dozens of crates to build on musl.

### On-demand decode

- Good: nothing beyond the crate's own object index on disk.
- Bad: a value query has to decode every chunk, the 82 s pass, on each search; only name queries
  over entries would be fast.
