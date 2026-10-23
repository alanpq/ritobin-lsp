import type * as vscode from "vscode";
import { log } from "./util";

export class PersistentState {
  constructor(private readonly globalState: vscode.Memento) {
    const { serverVersion } = this;
    log.info("PersistentState:", { serverVersion });
  }

  /**
   * Version of the extension that installed the server.
   * Used to check if we need to run patchelf again on NixOS.
   */
  get serverVersion(): string | undefined {
    return this.globalState.get("serverVersion");
  }

  async updateServerVersion(value: string | undefined) {
    await this.globalState.update("serverVersion", value);
  }

  /**
   * Outcome of the one-time Windows Explorer integration prompt.
   * `undefined` means the user hasn't answered yet (or answered "No",
   * which asks again next session).
   */
  get explorerIntegrationPrompt(): "dismissed" | "installed" | undefined {
    return this.globalState.get("explorerIntegrationPrompt");
  }

  async updateExplorerIntegrationPrompt(
    value: "dismissed" | "installed" | undefined,
  ) {
    await this.globalState.update("explorerIntegrationPrompt", value);
  }

  /**
   * Whether the user dismissed the "opened .bin read-only" nudge with
   * "Don't show again". `undefined` means keep showing it on each bin open.
   */
  get binReadonlyPrompt(): "dismissed" | undefined {
    return this.globalState.get("binReadonlyPrompt");
  }

  async updateBinReadonlyPrompt(value: "dismissed" | undefined) {
    await this.globalState.update("binReadonlyPrompt", value);
  }

  /** The last game search, so the input box can offer it again. */
  get lastGameSearch(): { query: string; kinds: string[] } | undefined {
    return this.globalState.get("lastGameSearch");
  }

  async updateLastGameSearch(value: { query: string; kinds: string[] } | undefined) {
    await this.globalState.update("lastGameSearch", value);
  }
}
