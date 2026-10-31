import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { anchors, checkContent, frontmatter, proseLines } from "./check-content.mjs";

const page = (title, body, description = "About it.") =>
  `---\ntitle: "${title}"\ndescription: "${description}"\n---\n\n${body}\n`;

/** A guide in a temporary folder: `pages` maps a file name to its text. */
function guide(pages, order) {
  const dir = mkdtempSync(join(tmpdir(), "guide-"));
  mkdirSync(dir, { recursive: true });
  for (const [name, text] of Object.entries(pages)) writeFileSync(join(dir, name), text);
  const pagesInOrder = order ?? Object.keys(pages).map((n) => n.replace(/\.mdx?$/, ""));
  writeFileSync(join(dir, "meta.json"), JSON.stringify({ pages: pagesInOrder }));
  return dir;
}

describe("proseLines", () => {
  it("takes out fenced code and inline code and keeps the line numbers", () => {
    const text = "a `<x>` b\n```sh\n<y> {z}\n```\nc <d>";
    expect(proseLines(text)).toEqual(["a  b", "", "", "", "c <d>"]);
  });
});

describe("frontmatter and anchors", () => {
  it("reads the title and says where the body starts", () => {
    const text = page("Chats", "Body");
    expect(frontmatter(text).data.title).toBe("Chats");
    expect(frontmatter(text).bodyLine).toBe(5);
    expect(frontmatter("no frontmatter").data).toEqual({});
  });

  it("makes the slug of each heading the way the site does, ignoring headings in code", () => {
    const set = anchors(
      "# One\n\n## Two words & more\n\n```\n## Not a heading\n```\n\n## Two words & more\n",
    );
    expect([...set]).toEqual(["one", "two-words--more", "two-words--more-1"]);
  });
});

describe("the guide's pages", () => {
  it("accepts pages that are right", () => {
    const dir = guide({
      "index.md": page(
        "Guide",
        "See [Chats](./chats.md#sending) and `<owner>/<name>` and [out](https://example.com).",
      ),
      "chats.md": page("Chats", "## Sending\n\nText with `{ticket}`."),
    });
    expect(checkContent(dir)).toEqual([]);
  });

  it("asks for a title and a description, because the build fails without a title", () => {
    const dir = guide({ "a.md": "# No frontmatter\n\nText.\n" });
    expect(checkContent(dir)).toEqual([
      "a.md:1: no title in the frontmatter (the build fails without one)",
      "a.md:1: no description in the frontmatter",
    ]);
  });

  it("finds a placeholder or a brace outside code, which the build swallows without a word", () => {
    const dir = guide({
      "a.md": page(
        "A",
        "Use <owner>/<name> here.\n\nAnd {ticket}-{slug} there.\n\n`<ok>` is fine.",
      ),
    });
    const problems = checkContent(dir);
    expect(problems).toHaveLength(2);
    expect(problems[0]).toMatch(/^a\.md:6: a `<` outside code/);
    expect(problems[1]).toMatch(/^a\.md:8: a `\{` or `\}` outside code/);
  });

  it("finds links that are broken on the site: bare, with ../, to a page that is not there", () => {
    const dir = guide({
      "a.md": page(
        "A",
        "[bare](b.md) [up](../b.md) [missing](./nope.md) [anchor](./b.md#nothing) [ok](./b.md#top) [site](/docs/zzz) [self](#nowhere)\n\n## Here",
      ),
      "b.md": page("B", "## Top"),
    });
    const problems = checkContent(dir).join("\n");
    expect(problems).toMatch(/write the link to b\.md as \.\/b\.md/);
    expect(problems).toMatch(/write the link to \.\.\/b\.md as \.\/\.\.\/b\.md/);
    expect(problems).toMatch(/\.\/nope\.md is not a page of the guide/);
    expect(problems).toMatch(/b\.md has no heading for #nothing/);
    expect(problems).toMatch(/\/docs\/zzz is not a page/);
    expect(problems).toMatch(/there is no heading for #nowhere/);
    expect(problems).not.toMatch(/#top/);
  });

  it("wants every page in the order of the sidebar and nothing in it that is not a page", () => {
    const dir = guide({ "a.md": page("A", "x"), "b.md": page("B", "y") }, ["a", "gone"]);
    expect(checkContent(dir)).toEqual([
      'meta.json: "gone" is not a page',
      'meta.json: "b" is not in the order',
    ]);
  });

  it("is happy with the real guide", async () => {
    const real = new URL("../content/docs", import.meta.url).pathname;
    expect(checkContent(real)).toEqual([]);
  });
});
