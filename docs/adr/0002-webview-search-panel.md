# ADR-0002: Webview search panel

- **Status:** Accepted
- **Date:** 2026-10-23
- **Crates:** `ritobin-lsp`, the VS Code extension
- **Related:** the spec sections that state the rules:
  `docs/design/game-search.md` [section 7.4](../design/game-search.md#s7.4) and
  [section 8.2](../design/game-search.md#s8.2)

## Context and problem statement

A search over the whole game returns thousands of objects for an ordinary query: a class name
matches 23,260 objects, a champion name 6,438. The surface has to take a query as it is typed,
filter by term kind, show the class and the matched terms in distinguishable colours, preview an
object without opening a 900 KB chunk, and page without losing place.

VS Code offers five surfaces. A `TreeView` has no input of its own, colours only the icon or the
whole label, and renders every row it is given. A `QuickPick` is modal and flat. The built-in
Search view with `TextSearchProvider` is a proposed API and closed to Marketplace extensions. A
`WebviewView` lives in a sidebar container and renders any HTML. A `WebviewPanel` is the same
HTML in an editor tab.

The LSP `workspace/symbol` request is a sixth surface for one use: jumping to an object by name
through the editor's own picker, in every editor that speaks LSP.

## Decision drivers

- Results arrive as the user types; the server answers in under 70 ms.
- Class names, strings, hashes and links read apart at a glance.
- The surface is reachable without a ritobin file open.
- A jump by name costs nothing beyond the server.

## Considered options

1. **Tree view in the Explorer** - native rows and context menus.
2. **Webview view in an activity bar container**, plus `workspace/symbol` for jumps.
3. **Webview panel in an editor tab** as the primary surface.
4. **Quick pick only.**

## Decision

**Search is a webview view in the `ritobin-game` activity bar container, and `workspace/symbol`
with resolve puts the same index behind the editor's symbol picker.** The rules are in the spec,
[section 7.4](../design/game-search.md#s7.4) and [section 8.2](../design/game-search.md#s8.2).

The page draws with the theme's own colour variables and the editor's list and input styles. An
editor-tab panel is the same page in a `WebviewPanel` and is not part of this decision.

## Consequences

- **Positive:** an input with live results, kind chips with counts, previews inline, colours per
  kind, keyboard navigation, and paging by scroll. Ctrl+T reaches any object by name with no
  extension code and works in Zed and Neovim.
- **Negative:** about 700 lines of HTML, CSS and JavaScript the extension owns, outside the
  TypeScript bundle and type checker; icons are hand-drawn SVG, not codicons; accessibility is
  the page's own work; a `retainContextWhenHidden` webview holds its DOM while hidden.
- **Revisit when:** `TextSearchProvider` becomes a stable API, which would put the game behind
  Ctrl+Shift+F with no page at all.

## Pros and cons of the options

### Tree view in the Explorer

- Good: native look, keyboard, collapse, context menus, 150 lines of TypeScript.
- Bad: no inline input, so every search is a modal box; one colour per row at most; no previews;
  the Explorer is the wrong container for a game-wide tool.

### Webview view plus `workspace/symbol`

- Good: everything the drivers ask for; the symbol picker comes free.
- Bad: the extension owns a page; theme fidelity is by CSS variable, not by construction.

### Webview panel in an editor tab

- Good: full width for long paths, side by side with a chunk.
- Bad: not glanceable while editing; a tab per search or one shared tab to manage; the same
  page cost.

### Quick pick only

- Good: fastest to build; the editor's fuzzy matcher for free.
- Bad: modal, flat, no grouping or filter, results vanish on pick.
