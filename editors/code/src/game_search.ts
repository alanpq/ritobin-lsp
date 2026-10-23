import * as vscode from "vscode";

import type { Cmd, Ctx, CtxInit } from "./ctx";
import { guard, toast } from "./ide_utils";
import * as ra from "./lsp_ext";
import { log } from "./util";
import { RitobinWadFs, wadChunkUri } from "./wad_fs";

export const GAME_SEARCH_VIEW_ID = "ritobin-lsp.gameSearch";

/** Hits fetched per page. */
const PAGE = 500;

/** Messages the panel sends. */
type FromPanel =
  | { type: "ready" }
  | {
      type: "search";
      seq: number;
      query: string;
      kinds: ra.TermKind[];
      offset: number;
    }
  | { type: "open"; chunk: string; display: string | null; object: string }
  | { type: "preview"; key: string; chunk: string; object: string }
  | { type: "browse"; key: string; params: ra.GameBrowseParams }
  | { type: "copy"; text: string }
  | { type: "persist"; query: string; kinds: ra.TermKind[] }
  | { type: "rebuild" }
  | { type: "openSettings" };

/** Messages the panel receives. */
type ToPanel =
  | {
      type: "init";
      state: ra.GameIndexState;
      lastQuery?: { query: string; kinds: string[] };
    }
  | { type: "indexState"; state: ra.GameIndexState }
  | {
      type: "results";
      seq: number;
      result: ra.GameSearchResult;
      append: boolean;
    }
  | { type: "error"; seq: number; message: string }
  | { type: "preview"; key: string; preview: ra.ObjectPreview | null }
  | {
      type: "listing";
      key: string;
      listing: ra.GameListing | null;
      error?: string;
    }
  | { type: "focus"; query?: string };

/**
 * The Game Search panel: a webview view in the "Game" activity bar container. The panel owns
 * the input, the kind filter and the result list; this provider relays its requests to the
 * server and opens chunks.
 */
export class GameSearchPanel implements vscode.WebviewViewProvider {
  private view: vscode.WebviewView | undefined;

  constructor(
    private readonly ctx: Ctx,
    private readonly wadFs: RitobinWadFs,
  ) {
    ctx.pushExtCleanup(
      ctx.onDidChangeGameIndexState((state) =>
        this.post({ type: "indexState", state }),
      ),
    );
  }

  resolveWebviewView(view: vscode.WebviewView): void {
    this.view = view;
    const media = vscode.Uri.joinPath(this.ctx.extCtx.extensionUri, "media");
    view.webview.options = {
      enableScripts: true,
      localResourceRoots: [media],
    };
    view.webview.html = this.html(view.webview, media);
    view.webview.onDidReceiveMessage(
      (msg: FromPanel) => void this.onMessage(msg),
      undefined,
      this.ctx.subscriptions,
    );
    view.onDidDispose(() => {
      if (this.view === view) {
        this.view = undefined;
      }
    });
  }

  /** Brings the panel up, focuses its input, and runs `query` when given. */
  async focus(query?: string): Promise<void> {
    await vscode.commands.executeCommand(`${GAME_SEARCH_VIEW_ID}.focus`);
    // The view may still be resolving; the panel asks for `init` once ready and the focus
    // lands on the next tick.
    setTimeout(() => this.post({ type: "focus", query }), 50);
  }

  private post(msg: ToPanel): void {
    void this.view?.webview.postMessage(msg);
  }

  private async onMessage(msg: FromPanel): Promise<void> {
    switch (msg.type) {
      case "ready":
        this.post({
          type: "init",
          state: this.ctx.gameIndexState,
          lastQuery: this.ctx.persistentState.lastGameSearch,
        });
        return;
      case "search":
        await this.search(msg);
        return;
      case "open":
        await openChunk(this.wadFs, msg.chunk, msg.display, msg.object);
        return;
      case "preview":
        await this.preview(msg);
        return;
      case "browse":
        await this.browse(msg);
        return;
      case "copy":
        await vscode.env.clipboard.writeText(msg.text);
        return;
      case "persist":
        await this.ctx.persistentState.updateLastGameSearch({
          query: msg.query,
          kinds: msg.kinds,
        });
        return;
      case "rebuild":
        await this.ctx.client?.sendRequest(ra.rebuildGameIndex);
        toast.info("Rebuilding the game index.");
        return;
      case "openSettings":
        await vscode.commands.executeCommand(
          "workbench.action.openSettings",
          "ritobin-lsp.gameDirectory",
        );
        return;
    }
  }

  private async search(
    msg: Extract<FromPanel, { type: "search" }>,
  ): Promise<void> {
    try {
      const client = await this.ctx.ensureClientReady();
      const result = await client.sendRequest(ra.gameSearch, {
        query: msg.query,
        kinds: msg.kinds,
        limit: PAGE,
        offset: msg.offset,
      });
      this.post({
        type: "results",
        seq: msg.seq,
        result,
        append: msg.offset > 0,
      });
    } catch (e) {
      log.error("game search failed", e);
      this.post({ type: "error", seq: msg.seq, message: errorText(e) });
    }
  }

  private async preview(
    msg: Extract<FromPanel, { type: "preview" }>,
  ): Promise<void> {
    try {
      const client = await this.ctx.ensureClientReady();
      const preview = await client.sendRequest(ra.gameObjectPreview, {
        chunk: msg.chunk,
        object: msg.object,
        maxLines: 12,
      });
      this.post({ type: "preview", key: msg.key, preview });
    } catch (e) {
      log.error("game object preview failed", e);
      this.post({ type: "preview", key: msg.key, preview: null });
    }
  }

  private async browse(
    msg: Extract<FromPanel, { type: "browse" }>,
  ): Promise<void> {
    try {
      const client = await this.ctx.ensureClientReady();
      const listing = await client.sendRequest(ra.gameBrowse, msg.params);
      this.post({ type: "listing", key: msg.key, listing });
    } catch (e) {
      log.error("game browse failed", e);
      this.post({
        type: "listing",
        key: msg.key,
        listing: null,
        error: errorText(e),
      });
    }
  }

  private html(webview: vscode.Webview, media: vscode.Uri): string {
    const css = webview.asWebviewUri(
      vscode.Uri.joinPath(media, "game-search.css"),
    );
    const js = webview.asWebviewUri(
      vscode.Uri.joinPath(media, "game-search.js"),
    );
    const nonce = Array.from({ length: 16 }, () =>
      Math.floor(Math.random() * 36).toString(36),
    ).join("");
    return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${webview.cspSource}; script-src 'nonce-${nonce}';">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <link rel="stylesheet" href="${css}">
  <title>Game Search</title>
</head>
<body>
  <div class="head">
    <div class="search">
      <input id="query" type="text" placeholder="Search the game: object, class, field, string, hash, link…" spellcheck="false" autocomplete="off">
      <span class="tools">
        <button id="clear" class="icon-button" title="Clear"></button>
        <button id="fold" class="icon-button" title="Fold or unfold every chunk"></button>
        <button id="rebuild" class="icon-button" title="Rebuild game index"></button>
      </span>
    </div>
    <div id="chips" class="chips"></div>
    <div id="status" class="status"></div>
  </div>
  <div id="results" class="results"></div>
  <script nonce="${nonce}" src="${js}"></script>
</body>
</html>`;
  }
}

function errorText(e: unknown): string {
  if (e instanceof Error) {
    return e.message;
  }
  if (typeof e === "object" && e && "message" in e) {
    return String((e as { message: unknown }).message);
  }
  return String(e);
}

async function openChunk(
  wadFs: RitobinWadFs,
  chunkHash: string,
  displayPath: string | null,
  objectHash: string | null,
): Promise<void> {
  const uri = wadChunkUri(chunkHash, displayPath);
  const doc = await vscode.workspace.openTextDocument(uri);
  const editor = await vscode.window.showTextDocument(doc, {
    preview: true,
    preserveFocus: false,
  });
  const line = objectHash ? wadFs.entryLine(uri, objectHash) : undefined;
  if (line !== undefined) {
    const pos = new vscode.Position(line, 0);
    editor.selection = new vscode.Selection(pos, pos);
    editor.revealRange(
      new vscode.Range(pos, pos),
      vscode.TextEditorRevealType.AtTop,
    );
  }
}

let panel: GameSearchPanel | undefined;
let wadFs: RitobinWadFs | undefined;

export function registerGameSearch(ctx: Ctx, fs: RitobinWadFs): void {
  wadFs = fs;
  panel = new GameSearchPanel(ctx, fs);
  ctx.pushExtCleanup(
    vscode.window.registerWebviewViewProvider(GAME_SEARCH_VIEW_ID, panel, {
      webviewOptions: { retainContextWhenHidden: true },
    }),
  );
}

/** Focuses the panel. With a selection in the active editor, searches for it. */
export function gameSearch(_ctx: Ctx): Cmd {
  return guard("Game search", async () => {
    const editor = vscode.window.activeTextEditor;
    const selection = editor?.document.getText(editor.selection).trim();
    const query =
      selection && selection.length < 200 && !selection.includes("\n")
        ? selection
        : undefined;
    await panel?.focus(query);
  });
}

export function rebuildGameIndex(ctx: CtxInit): Cmd {
  return guard("Rebuild game index", async () => {
    await ctx.client.sendRequest(ra.rebuildGameIndex);
    toast.info("Rebuilding the game index.");
  });
}

export function openGameChunk(_ctx: CtxInit): Cmd {
  return guard(
    "Open game chunk",
    async (
      chunkHash: string,
      displayPath: string | null,
      objectHash: string | null,
    ) => {
      if (wadFs) {
        await openChunk(wadFs, chunkHash, displayPath, objectHash);
      }
    },
  );
}
