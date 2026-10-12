import { describe, expect, it } from "vitest";
import { parseAgentTodos, todosContext } from "./draft";

describe("agent todos", () => {
  it("reads suggested todos and ignores everything else", () => {
    const reply =
      'Done.\n```circles-todos\n["Add a migration", " ", 5, "Update docs"]\n```\n```circles-todos\nnope\n```';
    expect(parseAgentTodos(reply)).toEqual(["Add a migration", "Update docs"]);
    expect(parseAgentTodos("nothing here")).toEqual([]);
  });

  it("describes the open todos for an @todos mention", () => {
    const text = todosContext([
      { text: "Write tests", done: false },
      { text: "Old", done: true },
    ]);
    expect(text).toContain("- [ ] Write tests");
    expect(text).not.toContain("Old");
    expect(text).toContain("circles-todos");
    expect(todosContext([])).toContain("(none open)");
  });
});
