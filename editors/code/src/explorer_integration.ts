import * as fs from "node:fs";
import * as path from "node:path";
import * as vscode from "vscode";

import type { Ctx } from "./ctx";
import { askUser, toast } from "./ide_utils";
import { EXPLORER_LAYOUT_VERSION } from "./registry/constants";
import {
  expectedCommand,
  readInstalledCommand,
  removeExplorerKeys,
  sweepLegacyExplorerKeys,
  writeExplorerKeys,
} from "./registry/explorer_keys";
import { log } from "./util";

export function explorerIntegrationSupported(): boolean {
  return process.platform === "win32" && vscode.env.remoteName === undefined;
}

function resolveCodeExe(): string | undefined {
  // The desktop extension host is a fork of the VS Code executable itself
  // (also covers Insiders/VSCodium installs).
  const execPath = process.execPath;
  if (
    path.extname(execPath).toLowerCase() === ".exe" &&
    fs.existsSync(execPath)
  ) {
    return execPath;
  }

  // Fallback: appRoot is <install dir>\resources\app.
  const installDir = path.join(vscode.env.appRoot, "..", "..");
  for (const name of ["Code.exe", "Code - Insiders.exe", "VSCodium.exe"]) {
    const candidate = path.join(installDir, name);
    if (fs.existsSync(candidate)) {
      return candidate;
    }
  }

  return undefined;
}

export async function installExplorerIntegration(ctx: Ctx): Promise<boolean> {
  if (!explorerIntegrationSupported()) {
    toast.warn(
      "Explorer integration is only available on local Windows installs.",
    );

    return false;
  }

  const codeExe = resolveCodeExe();
  if (!codeExe) {
    toast.error(
      "Could not locate the VS Code executable to register with Explorer.",
    );

    return false;
  }

  const failures = await writeExplorerKeys(codeExe);
  if (failures > 0) {
    toast.error(
      `Explorer integration install failed (${failures} registry operation(s) failed). ` +
        "See the ritobin-lsp Extension output for details.",
    );

    return false;
  }

  await ctx.persistentState.updateExplorerIntegrationPrompt("installed");
  await ctx.persistentState.updateExplorerLayoutVersion(
    EXPLORER_LAYOUT_VERSION,
  );

  log.info("Explorer integration installed", { codeExe });
  toast.info(
    'Explorer integration installed: right-click a .bin file and choose "Open as Ritobin"; ' +
      ".rito/.ritobin files now open with VS Code.",
  );

  return true;
}

export async function uninstallExplorerIntegration(ctx: Ctx): Promise<boolean> {
  if (!explorerIntegrationSupported()) {
    toast.warn(
      "Explorer integration is only available on local Windows installs.",
    );

    return false;
  }

  await removeExplorerKeys();

  // "dismissed" both records the answer to the one-time prompt and stops
  // `reconcileExplorerIntegration` from putting the keys straight back.
  await ctx.persistentState.updateExplorerIntegrationPrompt("dismissed");
  await ctx.persistentState.updateExplorerLayoutVersion(undefined);

  toast.info("Explorer integration uninstalled.");
  return true;
}

/**
 * Re-assert the registry layout for users who already opted in, if it has
 * drifted from what this version of the extension writes.
 *
 * Drift happens two ways: the extension changes its key layout (caught by the
 * stored layout version), or the VS Code executable moves — a Stable → Insiders
 * switch, or a portable install being relocated (caught by comparing the
 * registered command against the current one).
 *
 * Deliberately silent. The user consented to these keys already; re-writing
 * them is not a new consent event, so there is no toast on either path.
 */
export async function reconcileExplorerIntegration(ctx: Ctx): Promise<void> {
  if (!explorerIntegrationSupported()) {
    return;
  }

  // Only ever touch installs the user opted into. `undefined` (not asked yet)
  // and "dismissed" (declined, or explicitly uninstalled) are both hands-off.
  if (ctx.persistentState.explorerIntegrationPrompt !== "installed") {
    return;
  }

  try {
    const codeExe = resolveCodeExe();
    if (!codeExe) {
      log.warn(
        "Skipping Explorer integration reconcile: VS Code executable not found.",
      );

      return;
    }

    const staleLayout =
      ctx.persistentState.explorerLayoutVersion !== EXPLORER_LAYOUT_VERSION;

    // Skip the registry read when the layout version has already decided it.
    // Otherwise this is the one `reg query` an unchanged session costs. A
    // missing key reads as `undefined`, which counts as drift and restores it.
    const staleCommand =
      !staleLayout &&
      (await readInstalledCommand()) !== expectedCommand(codeExe);

    if (!staleLayout && !staleCommand) {
      return;
    }

    await sweepLegacyExplorerKeys();

    const failures = await writeExplorerKeys(codeExe);
    if (failures > 0) {
      log.warn(
        `Explorer integration reconcile: ${failures} registry operation(s) failed.`,
      );

      return;
    }

    await ctx.persistentState.updateExplorerLayoutVersion(
      EXPLORER_LAYOUT_VERSION,
    );

    log.info("Explorer integration reconciled", {
      codeExe,
      reason: staleLayout ? "layout version" : "command drift",
    });
  } catch (err) {
    // Never let a background repair break activation.
    log.warn("Explorer integration reconcile failed:", err);
  }
}

export async function maybePromptExplorerIntegration(ctx: Ctx): Promise<void> {
  if (!explorerIntegrationSupported()) {
    return;
  }
  
  const state = ctx.persistentState.explorerIntegrationPrompt;
  if (state === "installed" || state === "dismissed") {
    return;
  }

  const choice = await askUser(
    'Add "Open as Ritobin" to the Windows Explorer context menu for .bin files, ' +
      "and open .rito/.ritobin files with VS Code by default?",
    "Yes",
    "No",
    "Don't ask again",
  );
  if (choice === "Yes") {
    await installExplorerIntegration(ctx);
  } else if (choice === "Don't ask again") {
    await ctx.persistentState.updateExplorerIntegrationPrompt("dismissed");
  }
  // "No" (or dismissing the toast) → ask again next session.
}
