import { describe, expect, it } from "vitest";
import { asComment, parseFindings, reviewPrompt } from "./findings";

const block = (json: string) => `Summary.\n\n\`\`\`circles-findings\n${json}\n\`\`\`\n`;

describe("parseFindings", () => {
  it("reads findings from the labelled block", () => {
    const found = parseFindings(
      block(
        '[{"path":"src/a.ts","line":4,"body":"Null check missing"},{"path":"b.rs","line":10,"end_line":14,"body":"Extract"}]',
      ),
    );
    expect(found).toEqual([
      { path: "src/a.ts", line: 4, endLine: 4, body: "Null check missing" },
      { path: "b.rs", line: 10, endLine: 14, body: "Extract" },
    ]);
  });

  it("takes every block in the text and ignores other code blocks", () => {
    const text = `${block('[{"path":"a","line":1,"body":"one"}]')}\n\`\`\`json\n[{"path":"x","line":1,"body":"no"}]\n\`\`\`\n${block('[{"path":"b","line":2,"body":"two"}]')}`;
    expect(parseFindings(text).map((f) => f.body)).toEqual(["one", "two"]);
  });

  it("skips what is malformed or unsafe", () => {
    const bad = [
      '{"path":"a","line":1,"body":"not an array"}',
      '[{"path":"a","line":0,"body":"line zero"}]',
      '[{"path":"a","body":"no line"}]',
      '[{"path":"a","line":1,"body":"  "}]',
      '[{"path":"/etc/hosts","line":1,"body":"absolute"}]',
      '[{"path":"../x","line":1,"body":"escapes"}]',
      '[{"path":"a/../../x","line":1,"body":"escapes"}]',
      "[null, 5]",
      "not json at all",
    ];
    for (const json of bad) expect(parseFindings(block(json))).toEqual([]);
  });

  it("never lets the end come before the start, and accepts an empty review", () => {
    expect(parseFindings(block('[{"path":"a","line":9,"end_line":3,"body":"x"}]'))[0].endLine).toBe(
      9,
    );
    expect(parseFindings(block("[]"))).toEqual([]);
    expect(parseFindings("no block here")).toEqual([]);
  });
});

describe("review prompt and comments", () => {
  it("asks for the base branch and the findings block", () => {
    const prompt = reviewPrompt("main");
    expect(prompt).toContain("compared with main");
    expect(prompt).toContain("circles-findings");
  });

  it("turns a finding into an agent comment", () => {
    expect(asComment({ path: "a", line: 2, endLine: 3, body: "b" })).toEqual({
      path: "a",
      line_start: 2,
      line_end: 3,
      body: "b",
      source: "agent",
    });
  });
});
