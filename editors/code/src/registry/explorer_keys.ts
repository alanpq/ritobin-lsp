import * as registry from ".";
import {
  EXPLORER_EXTENSION_KEYS,
  EXPLORER_LEGACY_KEYS,
  EXPLORER_PROG_ID,
  EXPLORER_PROG_ID_KEY,
  EXPLORER_VERB_KEY,
} from "./constants";

/**
 * The registry layout of the Windows Explorer integration, as pure key
 * operations.
 *
 * Kept separate from `../explorer_integration.ts` (which owns the UI, the
 * prompt and the VS Code executable lookup) so that `src/uninstall.ts` can
 * reuse the teardown from plain Node. Nothing here may import `vscode`.
 */

/** The `shell\open\command` / verb command string for a given VS Code exe. */
export function expectedCommand(codeExe: string): string {
  return `"${codeExe}" "%1"`;
}

/** The `DefaultIcon` / verb icon string for a given VS Code exe. */
export function expectedIcon(codeExe: string): string {
  return `"${codeExe}",0`;
}

/**
 * Write the full key layout. Every operation is a `reg add /f`, so this is
 * idempotent and doubles as the repair path. Returns the number of failed
 * operations (0 on success).
 */
export async function writeExplorerKeys(codeExe: string): Promise<number> {
  const command = expectedCommand(codeExe);
  const icon = expectedIcon(codeExe);

  const results = [
    await registry.setValue(EXPLORER_VERB_KEY, undefined, "Open as Ritobin"),
    await registry.setValue(EXPLORER_VERB_KEY, "Icon", icon),
    await registry.setValue(
      `${EXPLORER_VERB_KEY}\\command`,
      undefined,
      command,
    ),
    await registry.setValue(
      EXPLORER_PROG_ID_KEY,
      undefined,
      "Ritobin Text File",
    ),
    await registry.setValue(
      `${EXPLORER_PROG_ID_KEY}\\DefaultIcon`,
      undefined,
      icon,
    ),
    await registry.setValue(
      `${EXPLORER_PROG_ID_KEY}\\shell\\open\\command`,
      undefined,
      command,
    ),
  ];

  for (const key of EXPLORER_EXTENSION_KEYS) {
    results.push(await registry.setValue(key, undefined, EXPLORER_PROG_ID));
  }

  return results.filter((ok) => !ok).length;
}

/**
 * Remove every key this extension owns, including any left over from
 * superseded layouts.
 *
 * Extension associations are only cleared if they still point at our ProgId —
 * another handler may have claimed `.rito` in the meantime, and that claim is
 * not ours to revoke.
 */
export async function removeExplorerKeys(): Promise<void> {
  await registry.deleteKey(EXPLORER_VERB_KEY);
  await registry.deleteKey(EXPLORER_PROG_ID_KEY);
  await sweepLegacyExplorerKeys();

  for (const key of EXPLORER_EXTENSION_KEYS) {
    if ((await registry.getValue(key)) === EXPLORER_PROG_ID) {
      await registry.deleteValue(key);
    }
  }
}

/** Drop keys from superseded layouts without touching the current ones. */
export async function sweepLegacyExplorerKeys(): Promise<void> {
  for (const key of EXPLORER_LEGACY_KEYS) {
    await registry.deleteKey(key);
  }
}

/**
 * The command currently registered for the ProgId, or `undefined` if the key
 * is absent. Compare against [`expectedCommand`] to detect drift.
 */
export function readInstalledCommand(): Promise<string | undefined> {
  return registry.getValue(`${EXPLORER_PROG_ID_KEY}\\shell\\open\\command`);
}
