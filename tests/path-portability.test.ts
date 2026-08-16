// tests/path-portability.test.ts — Windows-safe filesystem URL conversion.
//
// Regression for the pattern in Issue #1: `new URL(...).pathname` on a
// Windows file URL yields "/C:/…" (leading slash + drive letter), which
// every fs API rejects. The fix is node:url fileURLToPath, exposed to tool
// scripts as tools/fs-url.ts fsPath. These tests pin the Windows URL shape
// and the conversion rule so nobody reverts to `.pathname`.

import { fileURLToPath } from "node:url";
import { describe, expect, test } from "bun:test";
import { fsPath } from "../tools/fs-url.ts";

// A Windows-style absolute file URL, as import.meta.url would be on a
// Windows checkout (C:\ repo).
const WINDOWS_URL = "file:///C:/PocketJS/pocketjs/tools/site-build.ts";

describe("Windows filesystem URL conversion", () => {
  test(".pathname keeps the drive-letter shape that breaks fs APIs", () => {
    // The directory of a Windows file URL, URL-encoded: the pathname starts
    // with "/C:/" — a leading slash before the drive letter. Passing this
    // string to fs/Bun.file on Windows fails (ENOENT/ENOTDIR).
    const pathname = new URL("../", WINDOWS_URL).pathname;
    expect(pathname.startsWith("/C:/")).toBe(true);
    expect(pathname.endsWith("PocketJS/pocketjs/")).toBe(true);
  });

  test("fsPath defers to fileURLToPath — the canonical conversion", () => {
    const viaFsPath = fsPath("..", WINDOWS_URL);
    const viaNode = fileURLToPath(new URL("..", WINDOWS_URL));
    expect(viaFsPath).toBe(viaNode);
    // The parent directory, whatever the host's separators.
    expect(viaFsPath).toMatch(/[/\\]pocketjs[/\\]?$/);
  });

  test("fsPath decodes percent-encoding; .pathname does not", () => {
    // A repo under a directory with a space (e.g. "Program Files"): the
    // URL pathname keeps %20, fileURLToPath decodes it to the real path —
    // passing the encoded string to fs fails even on POSIX.
    const url = "file:///C:/PocketJS/My%20Repo/tools/site-build.ts";
    expect(new URL("../", url).pathname).toBe("/C:/PocketJS/My%20Repo/");
    const root = fsPath("../", url);
    expect(root).not.toContain("%20");
    expect(root).toContain("My Repo");
    expect(fsPath("../", url)).toBe(fileURLToPath(new URL("../", url)));
  });

  test("fsPath handles a file (package.json) URL", () => {
    const p = fsPath("../package.json", WINDOWS_URL);
    expect(p.endsWith("package.json")).toBe(true);
  });

  test("fsPath handles a directory URL", () => {
    const p = fsPath("../dist", WINDOWS_URL);
    expect(p.endsWith("dist")).toBe(true);
  });
});
