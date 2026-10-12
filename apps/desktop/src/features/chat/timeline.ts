import type { Message } from "@circles/bindings/Message";

/** A tool call with what came back from it. */
export type ToolCall = {
  id: string;
  name: string;
  input: Record<string, unknown>;
  /** Text Claude Code returned for the call. Unset while the call is still running. */
  result?: string;
  isError?: boolean;
  /** Calls made by a sub-agent started with this call. */
  children: ToolCall[];
};

export type ChangedFile = { path: string; added: number; removed: number };

/** One step of Claude's work between messages: some thinking, or a tool call. */
export type WorkStep =
  | { kind: "thinking"; key: string; text: string }
  | { kind: "tool"; key: string; call: ToolCall };

export type TimelineItem =
  | {
      type: "user";
      key: string;
      message: Message /** Which turn it starts. Unset when it steers one. */;
      turn?: number;
    }
  | { type: "assistant"; key: string; text: string }
  /** Thinking and tool calls that follow each other, shown as one block. */
  | { type: "work"; key: string; steps: WorkStep[] }
  | { type: "system"; key: string; text: string; tone: "info" | "error" }
  | {
      type: "summary";
      key: string;
      /** The turn it closes. */
      turn: number;
      durationMs: number;
      changed: ChangedFile[];
      /** Everything Claude wrote in the turn, for the copy button. */
      text: string;
    };

type Content = Record<string, unknown>;

const asContent = (message: Message): Content => (message.content ?? {}) as Content;
const str = (value: unknown): string => (typeof value === "string" ? value : "");

/** Claude Code returns tool output as text or as a list of blocks. */
export function resultText(content: unknown): string {
  if (typeof content === "string") return content;
  if (Array.isArray(content)) {
    return content
      .map((block) => {
        if (typeof block === "string") return block;
        const b = block as Content;
        return b.type === "text" ? str(b.text) : b.type === "image" ? "[image]" : "";
      })
      .filter(Boolean)
      .join("\n");
  }
  return content == null ? "" : JSON.stringify(content);
}

const lineCount = (text: string) => (text === "" ? 0 : text.split("\n").length);

/** Lines added and removed by an edit-like call, counted from what the call sent. */
export function editStats(call: ToolCall): ChangedFile | null {
  const input = call.input;
  const path = str(input.file_path) || str(input.notebook_path);
  if (!path) return null;
  if (call.name === "Write") {
    return { path, added: lineCount(str(input.content)), removed: 0 };
  }
  if (call.name === "Edit") {
    return {
      path,
      added: lineCount(str(input.new_string)),
      removed: lineCount(str(input.old_string)),
    };
  }
  if (call.name === "MultiEdit" && Array.isArray(input.edits)) {
    let added = 0;
    let removed = 0;
    for (const edit of input.edits as Content[]) {
      added += lineCount(str(edit.new_string));
      removed += lineCount(str(edit.old_string));
    }
    return { path, added, removed };
  }
  return null;
}

function mergeChanged(calls: ToolCall[]): ChangedFile[] {
  const byPath = new Map<string, ChangedFile>();
  for (const call of calls) {
    if (call.isError) continue;
    const stats = editStats(call);
    if (!stats) continue;
    const seen = byPath.get(stats.path);
    if (seen) {
      seen.added += stats.added;
      seen.removed += stats.removed;
    } else byPath.set(stats.path, { ...stats });
  }
  return [...byPath.values()];
}

function allCalls(calls: ToolCall[]): ToolCall[] {
  return calls.flatMap((c) => [c, ...allCalls(c.children)]);
}

/**
 * Turns stored messages into what the chat shows. Thinking and tool calls that follow each other
 * form one work block, each result is attached to its call, calls made by a sub-agent move under their Task call, and
 * a summary closes every finished turn. A turn starts with a user message that is not steering.
 *
 * @param active whether Claude is still working, so the last turn gets no summary yet
 */
export function buildTimeline(messages: Message[], active: boolean): TimelineItem[] {
  const ordered = [...messages].sort((a, b) => a.seq - b.seq);
  const items: TimelineItem[] = [];
  const callsById = new Map<string, ToolCall>();
  let work: Extract<TimelineItem, { type: "work" }> | null = null;
  let turn: {
    number: number;
    startedAt: number;
    endedAt: number;
    calls: ToolCall[];
    text: string[];
  } | null = null;
  let turnCount = 0;

  const closeTurn = (key: string) => {
    if (!turn) return;
    items.push({
      type: "summary",
      key: `summary-${key}`,
      turn: turn.number,
      durationMs: Math.max(0, turn.endedAt - turn.startedAt),
      changed: mergeChanged(allCalls(turn.calls)),
      text: turn.text.join("\n\n"),
    });
    turn = null;
  };

  for (const message of ordered) {
    const content = asContent(message);
    const parent = str(content.parent_tool_use_id);

    if (message.kind === "tool_call") {
      const call: ToolCall = {
        id: str(content.id),
        name: str(content.name),
        input: (content.input ?? {}) as Record<string, unknown>,
        children: [],
      };
      callsById.set(call.id, call);
      const owner = parent ? callsById.get(parent) : undefined;
      if (owner) owner.children.push(call);
      else {
        if (!work) {
          work = { type: "work", key: `work-${message.id}`, steps: [] };
          items.push(work);
        }
        work.steps.push({ kind: "tool", key: message.id, call });
        turn?.calls.push(call);
      }
      if (turn) turn.endedAt = message.created_at;
      continue;
    }

    if (message.kind === "tool_result") {
      const call = callsById.get(str(content.tool_use_id));
      if (call) {
        call.result = resultText(content.content);
        call.isError = content.is_error === true;
      }
      if (turn) turn.endedAt = message.created_at;
      continue;
    }

    if (message.kind === "thinking") {
      if (parent) continue;
      if (!work) {
        work = { type: "work", key: `work-${message.id}`, steps: [] };
        items.push(work);
      }
      work.steps.push({ kind: "thinking", key: message.id, text: str(content.text) });
      if (turn) turn.endedAt = message.created_at;
      continue;
    }

    // Anything else ends a run of work.
    work = null;

    if (message.role === "user" && message.kind === "text") {
      if (content.steered !== true) {
        closeTurn(message.id);
        turnCount += 1;
        turn = {
          number: turnCount,
          startedAt: message.created_at,
          endedAt: message.created_at,
          calls: [],
          text: [],
        };
        items.push({ type: "user", key: message.id, message, turn: turnCount });
        continue;
      }
      items.push({ type: "user", key: message.id, message });
      continue;
    }

    if (message.role === "system") {
      items.push({
        type: "system",
        key: message.id,
        text: str(content.text),
        tone: content.error === true ? "error" : "info",
      });
      if (turn) turn.endedAt = message.created_at;
      continue;
    }

    if (message.kind === "text" && message.role === "assistant") {
      if (parent) continue;
      items.push({ type: "assistant", key: message.id, text: str(content.text) });
      turn?.text.push(str(content.text));
      if (turn) turn.endedAt = message.created_at;
    }
  }

  if (!active) closeTurn("last");
  return items;
}
