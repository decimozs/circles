/// <reference types="node" />
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

// The test setup turns stylesheet imports off, so the file is read as text.
const css = readFileSync(`${process.cwd()}/src/styles/globals.css`, "utf8");

describe("the window does not scroll as a whole", () => {
  it("fixes the page in place and stops the trackpad bounce", () => {
    expect(css).toMatch(
      /html,\s*body\s*\{[^}]*overflow:\s*hidden;[^}]*overscroll-behavior:\s*none;/,
    );
    expect(css).toMatch(/body\s*\{[^}]*position:\s*fixed;[^}]*inset:\s*0;/);
  });

  it("clips the root, so even focus or scrollIntoView cannot move it", () => {
    expect(css).toMatch(/#root\s*\{[^}]*overflow:\s*clip;/);
  });
});

describe("the glass panel", () => {
  it("blurs what is behind it, in both spellings WebKit needs", () => {
    expect(css).toMatch(/\.glass-panel\s*\{[^}]*-webkit-backdrop-filter:\s*blur/);
    expect(css).toMatch(/\.glass-panel\s*\{[^}]*\bbackdrop-filter:\s*blur/);
  });

  it("is opaque for people who ask for less transparency", () => {
    expect(css).toMatch(
      /prefers-reduced-transparency:\s*reduce\)\s*\{\s*\.glass-panel\s*\{[^}]*backdrop-filter:\s*none/,
    );
  });
});
