/// <reference types="node" />
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const atLeast = (version: string, minimum: string) => {
  const a = version.split(".").map(Number);
  const b = minimum.split(".").map(Number);
  for (let i = 0; i < 3; i++) {
    if ((a[i] ?? 0) !== (b[i] ?? 0)) return (a[i] ?? 0) > (b[i] ?? 0);
  }
  return true;
};

describe("the copy of DOMPurify inside Monaco", () => {
  // Monaco ships its own copy of DOMPurify, which `pnpm audit` does not see. Its releases have had a
  // copy with known vulnerabilities, so a patch (patches/monaco-editor@*.patch) puts a fixed one in.
  // This fails if that patch stops applying or Monaco is changed to something older.
  it("is a release that has the fixes for the known advisories", () => {
    // Read from the folder, since the tests replace the package with a stand-in. The tests run in
    // the app's folder.
    const path = join(
      process.cwd(),
      "node_modules/monaco-editor/esm/vs/base/browser/dompurify/dompurify.js",
    );
    const file = readFileSync(path, "utf8");
    const version = file.match(/@license DOMPurify (\d+\.\d+\.\d+)/)?.[1];
    expect(version, "the copy names its version").toBeDefined();
    expect(atLeast(version as string, "3.4.17"), `DOMPurify ${version} is too old`).toBe(true);
  });
});
