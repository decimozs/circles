import { describe, expect, it } from "vitest";
import { findSlash, highlightSkills } from "./skill-textarea";

describe("findSlash", () => {
  it("finds a slash that starts a word", () => {
    expect(findSlash("use /code-ch", 12)).toEqual({ query: "code-ch", start: 4 });
    expect(findSlash("/rev", 4)).toEqual({ query: "rev", start: 0 });
  });
  it("ignores a slash inside a path", () => {
    expect(findSlash("look in src/lib", 15)).toBeNull();
  });
});

describe("highlightSkills", () => {
  const known = new Set(["reproduce", "code-check", "aint:propose"]);
  const marked = (text: string) =>
    highlightSkills(text, known)
      .filter((s) => s.skill)
      .map((s) => s.text);

  it("marks the skills that exist, anywhere a word starts", () => {
    expect(marked("/reproduce \u2192 /code-check, then /aint:propose.")).toEqual([
      "/reproduce",
      "/code-check",
      "/aint:propose",
    ]);
    expect(marked("line one\n/reproduce")).toEqual(["/reproduce"]);
  });

  it("leaves words that are not skills, and paths, plain", () => {
    expect(marked("/missing and src/reproduce and a/code-check")).toEqual([]);
  });

  it("keeps every character of the text, in order", () => {
    const text = "use /code-check now\nand /reproduce";
    expect(
      highlightSkills(text, known)
        .map((s) => s.text)
        .join(""),
    ).toBe(text);
  });
});
