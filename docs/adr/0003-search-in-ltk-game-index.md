# ADR-0003: Search in `ltk_game_index`

- **Status:** Accepted
- **Date:** 2026-10-24
- **Crates:** `ritobin-lsp`
- **Supersedes:** [ADR-0001](0001-object-term-index.md)
- **Related:** league-mod
  [ADR-0038](https://github.com/LeagueToolkit/league-mod/blob/main/docs/adr/0038-search-in-the-index-crate.md),
  league-mod
  [`game-search.md`](https://github.com/LeagueToolkit/league-mod/blob/main/docs/design/game-search.md),
  and the spec sections that state the rules here: `docs/design/game-search.md`
  [section 5](../design/game-search.md#s5) and [section 6](../design/game-search.md#s6)

## Context and problem statement

ADR-0001 put an object term index in the server: a term walk over every bin, postings keyed by
hash, substring sheets over the Mimir tables, query resolution and an object tree, about 2,400
lines in `crates/ritobin-lsp/src/game/`. 150 of them copied `ltk_game_index`'s object build.

`ltk_game_index` 0.2.0 holds the same design behind its `search` feature (league-mod
ADR-0038): `TermIndex`, `NameSheets` with names through a `NameSource` trait, `GameSearch` with
query resolution, paging and the object tree. LTK Manager is its other consumer. The crate walks
objects over their streamed bytes without materializing them, splits nested classes into an
`Embedded` kind, and answers the same queries in 6 ms to 9 ms on the measured installation
(spec appendix A).

## Decision drivers

- One term walk, one cache format and one query resolution across the server and LTK Manager.
- The server keeps its own requests, extension surface and response shapes.
- A hash table update still recuts names without a walk.

## Considered options

1. **The crate's `search` feature**, with the Mimir tables as its `NameSource` and the server's
   requests mapped from the crate's results.
2. **The server's own index**, as ADR-0001 decided.

## Decision

**The server builds and queries `ltk_game_index`'s `search` feature.** The Mimir tables reach it
through `MimirNames` and `ChunkPaths`. The server maps hits and listings to the shapes its
requests answer in. The rules are in the spec, [section 5](../design/game-search.md#s5) to
[section 7](../design/game-search.md#s7).

## Consequences

- **Positive:** the index, sheets, terms, build, names, search and tree modules leave the server.
  `rayon`, `rmp-serde` and `memchr` leave its dependencies. A cold build reaches `Ready` in 19.5 s
  and a warm start in 3.9 s.
- **Negative:** a change to search behaviour is a release of `ltk_game_index`. The `embedded`
  kind is a new chip in the panel. The tree lists one entry per object, not one per declaration.
- **Revisit when:** the server needs a query the crate does not answer and the crate declines it.

## Pros and cons of the options

### The crate's `search` feature

- Good: no term walk or postings code in the server; the crate's tests and measurements cover it.
- Bad: the server's search moves at the crate's release cadence.

### The server's own index

- Good: the server changes its search without a crate release.
- Bad: two implementations of one design, in two repositories, with two sets of bugs.
