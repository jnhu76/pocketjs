// tools/fs-url.ts — filesystem path conversion for tool scripts.
//
// Never use `new URL(...).pathname` as a filesystem path: on Windows the
// pathname of a file URL is "/C:/…" — a leading slash plus the drive
// letter — which every fs API rejects (ENOENT, ENOTDIR). This is the
// canonical conversion (node:url fileURLToPath); a regression test pins
// the Windows shape in tests/path-portability.test.ts.

import { fileURLToPath } from "node:url";

/** The filesystem path of `relative` resolved against `metaUrl`
 *  (pass `import.meta.url`). POSIX result is identical to `.pathname`;
 *  on Windows the drive letter is handled correctly. */
export function fsPath(relative: string, metaUrl: string): string {
  return fileURLToPath(new URL(relative, metaUrl));
}
