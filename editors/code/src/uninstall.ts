/**
 * `vscode:uninstall` hook — removes the Windows Explorer registry keys when the
 * extension is uninstalled.
 *
 * VS Code runs this in **plain Node**, not in the extension host: there is no
 * `vscode` module, and any transitive import of one throws at load time, which
 * VS Code swallows. Keep this file's import graph vscode-free — after building,
 * `out/uninstall.js` should require nothing but `child_process`.
 *
 * Note this hook does not fire on update, on manual deletion of the extension
 * folder, or when VS Code itself is uninstalled.
 */

import { setRegistryLogger } from "./registry";
import { removeExplorerKeys } from "./registry/explorer_keys";

if (process.platform === "win32") {
  setRegistryLogger((message) => console.warn(message));

  // A failed cleanup must never fail the uninstall itself.
  removeExplorerKeys().catch((err) => {
    console.warn("ritobin-lsp: Explorer registry cleanup failed:", err);
  });
}
