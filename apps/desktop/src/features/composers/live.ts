import type { Workspace } from "@circles/bindings/Workspace";
import type { FleetGroup } from "./composer-panel";

export type LiveState = "waiting" | "working" | "idle";

/** One workspace of a run, as the panel's Fleet section shows it. */
export type LiveRow = {
  key: string;
  name: string;
  workspaceId: string | null;
  /** The workspace itself, for what is shown when the row is opened out. */
  workspace: Workspace | null;
  state: LiveState;
  /** When the turn running now began, in milliseconds. Set for a working or waiting row. */
  since: number | null;
  /** When it last did anything, for a row that is not running. */
  lastActive: number;
  /** An agent in it has finished at least one turn. */
  done: boolean;
};

const ORDER: Record<LiveState, number> = { waiting: 0, working: 1, idle: 2 };

/** The rows of the Fleet section: what waits for you first, then what works, then the rest, and
 * inside each the one that has gone on longest (or been quiet longest) first. */
export function liveRows(groups: FleetGroup[]): LiveRow[] {
  const rows = groups.map((group): LiveRow => {
    const going = group.chats.filter((c) => c.status === "working" || c.status === "blocked");
    const state: LiveState = going.some((c) => c.status === "blocked")
      ? "waiting"
      : going.length > 0
        ? "working"
        : "idle";
    // Older rows have no start time: the last change is the best that is known.
    const starts = going.map((c) => c.working_since ?? c.updated_at);
    const ws = group.workspace;
    return {
      key: group.key,
      name: ws?.title ?? ws?.city ?? "A removed workspace",
      workspaceId: ws?.id ?? null,
      workspace: ws ?? null,
      state,
      since: starts.length > 0 ? Math.min(...starts) : null,
      lastActive: Math.max(ws?.updated_at ?? 0, ...group.chats.map((c) => c.updated_at)),
      done: group.chats.some((c) => c.turns > 0),
    };
  });
  return rows.sort(
    (a, b) =>
      ORDER[a.state] - ORDER[b.state] ||
      (a.since ?? 0) - (b.since ?? 0) ||
      b.lastActive - a.lastActive,
  );
}

/** How many rows are going: working, or waiting for you. */
export function runningCount(rows: LiveRow[]): number {
  return rows.filter((r) => r.state !== "idle").length;
}
