import type { Message } from "@circles/bindings/Message";
import { describe, expect, it } from "vitest";
import { buildTimeline, resultText, type TimelineItem, type ToolCall } from "./timeline";

let seq = 0;
function msg(
  role: Message["role"],
  kind: Message["kind"],
  content: Record<string, unknown>,
  at = 1000 + seq * 1000,
): Message {
  seq += 1;
  return { id: `m${seq}`, session_id: "s", seq, role, kind, content, created_at: at };
}
const user = (text: string, extra = {}, at?: number) => msg("user", "text", { text, ...extra }, at);
const say = (text: string, at?: number) => msg("assistant", "text", { text }, at);
const call = (id: string, name: string, input = {}, extra = {}) =>
  msg("assistant", "tool_call", { id, name, input, ...extra });
const result = (id: string, content: unknown, is_error = false) =>
  msg("user", "tool_result", { tool_use_id: id, content, is_error });
/** The tool calls inside a work block. */
function callsOf(item: TimelineItem | undefined): ToolCall[] {
  if (item?.type !== "work") throw new Error("expected a work block");
  return item.steps.flatMap((s) => (s.kind === "tool" ? [s.call] : []));
}
const types = (items: TimelineItem[]) => items.map((i) => i.type);

describe("buildTimeline", () => {
  it("shows a plain exchange with a summary once the turn is done", () => {
    seq = 0;
    const items = buildTimeline([user("hi", {}, 0), say("hello", 4000)], false);
    expect(types(items)).toEqual(["user", "assistant", "summary"]);
    const summary = items[2];
    expect(summary).toMatchObject({ durationMs: 4000, text: "hello", changed: [] });
  });

  it("gives the running turn no summary", () => {
    seq = 0;
    expect(types(buildTimeline([user("hi"), say("working")], true))).toEqual(["user", "assistant"]);
  });

  it("groups consecutive tool calls into one block and attaches results", () => {
    seq = 0;
    const items = buildTimeline(
      [
        user("go"),
        call("a", "Read", { file_path: "/x" }),
        result("a", [{ type: "text", text: "file body" }]),
        call("b", "Bash", { command: "ls" }),
        result("b", "boom", true),
        say("done"),
      ],
      true,
    );
    expect(types(items)).toEqual(["user", "work", "assistant"]);
    const calls = callsOf(items[1]);
    expect(calls).toHaveLength(2);
    expect(calls[0]).toMatchObject({ name: "Read", result: "file body", isError: false });
    expect(calls[1]).toMatchObject({ name: "Bash", result: "boom", isError: true });
  });

  it("starts a new group after text", () => {
    seq = 0;
    const items = buildTimeline(
      [user("go"), call("a", "Read"), say("mid"), call("b", "Glob")],
      true,
    );
    expect(types(items)).toEqual(["user", "work", "assistant", "work"]);
  });

  it("puts thinking and tool calls that follow each other into one block", () => {
    seq = 0;
    const items = buildTimeline(
      [
        user("go"),
        msg("assistant", "thinking", { text: "first idea" }),
        call("a", "Bash", { command: "ls" }),
        result("a", "x"),
        msg("assistant", "thinking", { text: "second idea" }),
        call("b", "Read", { file_path: "/x" }),
        result("b", "y"),
        msg("assistant", "thinking", { text: "third idea" }),
        say("here you go"),
        msg("assistant", "thinking", { text: "later" }),
      ],
      true,
    );
    expect(types(items)).toEqual(["user", "work", "assistant", "work"]);
    const first = items[1];
    if (first.type !== "work") throw new Error("expected work");
    expect(first.steps.map((s) => s.kind)).toEqual([
      "thinking",
      "tool",
      "thinking",
      "tool",
      "thinking",
    ]);
  });

  it("leaves a call without a result as still running", () => {
    seq = 0;
    const items = buildTimeline([user("go"), call("a", "Bash", { command: "sleep 9" })], true);
    expect(callsOf(items[1])[0].result).toBeUndefined();
  });

  it("moves sub-agent calls under their Task call", () => {
    seq = 0;
    const items = buildTimeline(
      [
        user("go"),
        call("t", "Task", { description: "look" }),
        call("c", "Grep", { pattern: "x" }, { parent_tool_use_id: "t" }),
        msg("assistant", "text", { text: "inner", parent_tool_use_id: "t" }),
        result("t", "found"),
      ],
      true,
    );
    expect(types(items)).toEqual(["user", "work"]);
    const calls = callsOf(items[1]);
    expect(calls).toHaveLength(1);
    expect(calls[0].children.map((c) => c.name)).toEqual(["Grep"]);
  });

  it("counts changed files across a turn", () => {
    seq = 0;
    const items = buildTimeline(
      [
        user("edit"),
        call("a", "Edit", { file_path: "/a.ts", old_string: "x\ny", new_string: "x\ny\nz" }),
        result("a", "ok"),
        call("b", "Write", { file_path: "/b.ts", content: "1\n2\n3" }),
        result("b", "ok"),
        call("c", "Edit", { file_path: "/a.ts", old_string: "z", new_string: "w" }),
        result("c", "ok"),
        call("d", "Edit", { file_path: "/bad.ts", old_string: "q", new_string: "r" }),
        result("d", "no match", true),
        say("done"),
      ],
      false,
    );
    const summary = items[items.length - 1];
    if (summary?.type !== "summary") throw new Error("expected summary");
    expect(summary.changed).toEqual([
      { path: "/a.ts", added: 4, removed: 3 },
      { path: "/b.ts", added: 3, removed: 0 },
    ]);
  });

  it("keeps steering messages inside the turn", () => {
    seq = 0;
    const items = buildTimeline(
      [user("one"), say("a"), user("actually two", { steered: true }), say("b")],
      false,
    );
    expect(types(items)).toEqual(["user", "assistant", "user", "assistant", "summary"]);
  });

  it("numbers the turns, and a steering message does not start one", () => {
    seq = 0;
    const items = buildTimeline(
      [user("one"), say("a"), user("actually", { steered: true }), say("b"), user("two"), say("c")],
      false,
    );
    const users = items.filter((i) => i.type === "user");
    expect(users.map((i) => (i.type === "user" ? i.turn : null))).toEqual([1, undefined, 2]);
    const summaries = items.filter((i) => i.type === "summary");
    expect(summaries.map((i) => (i.type === "summary" ? i.turn : null))).toEqual([1, 2]);
  });

  it("closes each turn with its own summary", () => {
    seq = 0;
    const items = buildTimeline([user("one"), say("a"), user("two"), say("b")], false);
    expect(types(items)).toEqual(["user", "assistant", "summary", "user", "assistant", "summary"]);
  });

  it("shows interruptions and errors as system notes", () => {
    seq = 0;
    const items = buildTimeline(
      [
        user("go"),
        msg("system", "system", { text: "Interrupted", interrupted: true }),
        msg("system", "system", { text: "Boom", error: true }),
      ],
      false,
    );
    expect(items[1]).toMatchObject({ type: "system", text: "Interrupted", tone: "info" });
    expect(items[2]).toMatchObject({ type: "system", text: "Boom", tone: "error" });
  });

  it("orders by sequence even when rows arrive shuffled", () => {
    seq = 0;
    const a = user("first");
    const b = say("second");
    expect(types(buildTimeline([b, a], true))).toEqual(["user", "assistant"]);
  });
});

describe("resultText", () => {
  it("reads strings, block lists and other values", () => {
    expect(resultText("plain")).toBe("plain");
    expect(resultText([{ type: "text", text: "a" }, { type: "image" }, "b"])).toBe("a\n[image]\nb");
    expect(resultText(null)).toBe("");
    expect(resultText({ a: 1 })).toBe('{"a":1}');
  });
});
