/**
 * Registry locations for the Windows Explorer integration.
 *
 * All keys live under HKCU (no admin required). The SystemFileAssociations
 * verb adds a context-menu entry for .bin files without touching the user's
 * default .bin handler.
 */

/** ProgId that `.rito`/`.ritobin` files are associated with. */
export const EXPLORER_PROG_ID = "RitobinLSP.rito";

/** Context-menu verb shown for `.bin` files ("Open as Ritobin"). */
export const EXPLORER_VERB_KEY =
  "HKCU\\Software\\Classes\\SystemFileAssociations\\.bin\\shell\\RitobinLSP.OpenAsRitobin";

export const EXPLORER_PROG_ID_KEY = `HKCU\\Software\\Classes\\${EXPLORER_PROG_ID}`;

/** Extension keys whose default value points at [`EXPLORER_PROG_ID`]. */
export const EXPLORER_EXTENSION_KEYS = [
  "HKCU\\Software\\Classes\\.rito",
  "HKCU\\Software\\Classes\\.ritobin",
];

/**
 * Bump whenever the set or shape of the keys written by `writeExplorerKeys`
 * changes. Installs stamped with an older version are reconciled on activation.
 */
export const EXPLORER_LAYOUT_VERSION = 1;

/**
 * Keys written by superseded layouts, swept on both reconcile and uninstall.
 *
 * Empty today. When a key is renamed or dropped from the layout above, add its
 * old path here and bump [`EXPLORER_LAYOUT_VERSION`] — that is what lets an
 * existing install shed keys it should no longer own.
 */
export const EXPLORER_LEGACY_KEYS: string[] = [];
