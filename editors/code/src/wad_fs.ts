import * as vscode from "vscode";

import type { Ctx } from "./ctx";
import { reportError } from "./ide_utils";
import * as ra from "./lsp_ext";

export const WAD_SCHEME = "ritobin-wad";

const VIRTUAL_SUFFIX = ".rito";

/**
 * The document uri of one chunk of the game: `ritobin-wad://<hex>/<display>.rito`.
 *
 * The authority is the key - the chunk hash without its `0x` - and the path only names the tab.
 * An unnamed chunk shows its hex again.
 */
export function wadChunkUri(
  chunkHash: string,
  displayPath?: string | null,
): vscode.Uri {
  const hex = chunkHash.replace(/^0x/i, "").toLowerCase();
  const display = (
    displayPath && displayPath.length > 0 ? displayPath : hex
  ).replace(/^\/+/, "");
  return vscode.Uri.from({
    scheme: WAD_SCHEME,
    authority: hex,
    path: "/" + display + VIRTUAL_SUFFIX,
  });
}

/** The `0x`-prefixed chunk hash a wad document uri names. */
export function chunkHashOf(uri: vscode.Uri): string {
  return "0x" + uri.authority.toLowerCase();
}

interface CacheEntry {
  doc: ra.ChunkDocument;
  mtime: number;
  size: number;
}

/**
 * Read-only FileSystemProvider serving chunks of the game as ritobin text, through
 * `ritobin-lsp/deserializeWadChunk`. Nothing writes back into an archive.
 */
export class RitobinWadFs implements vscode.FileSystemProvider {
  private readonly cache = new Map<string, CacheEntry>();

  private readonly emitter = new vscode.EventEmitter<
    vscode.FileChangeEvent[]
  >();
  readonly onDidChangeFile = this.emitter.event;

  constructor(private readonly ctx: Ctx) {}

  handleDidCloseTextDocument(doc: vscode.TextDocument) {
    if (doc.uri.scheme === WAD_SCHEME) {
      this.cache.delete(doc.uri.toString());
    }
  }

  /** The 0-based line of root entry `objectHash` in the open document `uri`, if known. */
  entryLine(uri: vscode.Uri, objectHash: string): number | undefined {
    const entry = this.cache.get(uri.toString());
    const wanted = objectHash.toLowerCase();
    return entry?.doc.entries.find((e) => e.object.toLowerCase() === wanted)
      ?.line;
  }

  private async ensureLoaded(uri: vscode.Uri): Promise<CacheEntry> {
    const key = uri.toString();
    const cached = this.cache.get(key);
    if (cached) {
      return cached;
    }
    const chunk = chunkHashOf(uri);
    try {
      const client = await this.ctx.ensureClientReady();
      await this.waitForIndex();
      const doc = await client.sendRequest(ra.deserializeWadChunk, { chunk });
      const entry: CacheEntry = {
        doc,
        mtime: Date.now(),
        size: Buffer.byteLength(doc.text, "utf8"),
      };
      this.cache.set(key, entry);
      return entry;
    } catch (e) {
      const message = reportError(
        `Failed to open chunk ${chunk} as ritobin`,
        e,
      );
      throw vscode.FileSystemError.Unavailable(message);
    }
  }

  /**
   * Resolves once the game index is ready. A server that just restarted rebuilds its index
   * for a few seconds (longer on a cold cache), and a chunk asked for in that window would
   * fail; waiting here keeps a click on a stale result working. Gives up after two minutes.
   */
  private async waitForIndex(): Promise<void> {
    if (this.ctx.gameIndexState.state !== "building") {
      return;
    }
    await vscode.window.withProgress(
      {
        location: vscode.ProgressLocation.Notification,
        title: "Waiting for the game index",
      },
      (progress) =>
        new Promise<void>((resolve) => {
          const done = () => {
            listener.dispose();
            clearTimeout(timer);
            resolve();
          };
          const listener = this.ctx.onDidChangeGameIndexState((state) => {
            if (state.state === "building") {
              progress.report({
                message:
                  state.stage === "walk"
                    ? `archive ${state.done} of ${state.total}`
                    : state.stage,
              });
            } else {
              done();
            }
          });
          const timer = setTimeout(done, 120_000);
        }),
    );
  }

  async stat(uri: vscode.Uri): Promise<vscode.FileStat> {
    const entry = await this.ensureLoaded(uri);
    return {
      type: vscode.FileType.File,
      ctime: entry.mtime,
      mtime: entry.mtime,
      size: entry.size,
      permissions: vscode.FilePermission.Readonly,
    };
  }

  async readFile(uri: vscode.Uri): Promise<Uint8Array> {
    const entry = await this.ensureLoaded(uri);
    return Buffer.from(entry.doc.text, "utf8");
  }

  writeFile(uri: vscode.Uri): void {
    throw vscode.FileSystemError.NoPermissions(uri);
  }

  watch(): vscode.Disposable {
    return new vscode.Disposable(() => {});
  }

  readDirectory(uri: vscode.Uri): [string, vscode.FileType][] {
    throw vscode.FileSystemError.NoPermissions(uri);
  }

  createDirectory(uri: vscode.Uri): void {
    throw vscode.FileSystemError.NoPermissions(uri);
  }

  delete(uri: vscode.Uri): void {
    throw vscode.FileSystemError.NoPermissions(uri);
  }

  rename(oldUri: vscode.Uri): void {
    throw vscode.FileSystemError.NoPermissions(oldUri);
  }
}

/** Marks chunk documents in the UI so they read as part of the game, not a file on disk. */
export class RitobinWadDecorationProvider
  implements vscode.FileDecorationProvider
{
  provideFileDecoration(
    uri: vscode.Uri,
  ): vscode.ProviderResult<vscode.FileDecoration> {
    if (uri.scheme !== WAD_SCHEME) {
      return undefined;
    }
    return new vscode.FileDecoration(
      "⚙",
      "Read-only chunk of the League install",
      new vscode.ThemeColor("charts.purple"),
    );
  }
}
