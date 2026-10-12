import { cn } from "@circles/ui/utils";
import { Loader2Icon } from "lucide-react";
import type { FleetGroup } from "./composer-panel";

const NODE_W = 188;
const NODE_H = 52;
const COL_GAP = 72;
const ROW_GAP = 14;
const PAD = 20;

type Tone = "working" | "waiting" | "idle";

const DOT: Record<Tone, string> = {
  working: "bg-success animate-pulse",
  waiting: "bg-warning animate-pulse",
  idle: "bg-muted-foreground/40",
};

const label = (status: string) => `${status.charAt(0).toUpperCase()}${status.slice(1)}`;

/** An agent is named by its chat. Two with the same name are told apart by a number. */
function agentTitles(groups: FleetGroup[]): Map<string, string> {
  const seen = new Map<string, number>();
  const out = new Map<string, string>();
  for (const group of groups) {
    for (const chat of group.chats) {
      const base = chat.title?.trim() || "New chat";
      const n = (seen.get(base) ?? 0) + 1;
      seen.set(base, n);
      out.set(chat.id, n === 1 ? base : `${base} (${n})`);
    }
  }
  return out;
}

const toneOf = (status: string): Tone =>
  status === "blocked" ? "waiting" : status === "working" ? "working" : "idle";

type Node = {
  id: string;
  col: 0 | 1 | 2;
  y: number;
  title: string;
  sub: string;
  tone: Tone | null;
  spin?: boolean;
  open?: string;
};

type Edge = { from: string; to: string };

/** Lays the fleet out in three columns: the Composer, its workspaces, and the agents in them. */
function layout(name: string, groups: FleetGroup[]) {
  const nodes: Node[] = [];
  const edges: Edge[] = [];
  const row = NODE_H + ROW_GAP;
  const titles = agentTitles(groups);
  let next = 0;
  const workspaceYs: number[] = [];
  groups.forEach((group, i) => {
    const rows = Math.max(1, group.chats.length);
    const top = next * row;
    const wsId = `w:${group.key}`;
    const ws = group.workspace;
    const status = group.chats.some((c) => c.status === "blocked")
      ? "blocked"
      : group.chats.some((c) => c.status === "working")
        ? "working"
        : "idle";
    nodes.push({
      id: wsId,
      col: 1,
      y: top + ((rows - 1) * row) / 2,
      title: ws?.title ?? ws?.city ?? "A removed workspace",
      sub: [ws?.branch, ws?.pr_number ? `#${ws.pr_number}` : null].filter(Boolean).join(" · "),
      tone: toneOf(status),
      open: ws?.id,
    });
    workspaceYs.push(top + ((rows - 1) * row) / 2);
    edges.push({ from: "root", to: wsId });
    group.chats.forEach((chat, at) => {
      const id = `a:${chat.id}`;
      nodes.push({
        id,
        col: 2,
        y: top + at * row,
        title: titles.get(chat.id) ?? "New chat",
        sub: chat.status === "blocked" ? "Waits for a permission" : label(chat.status),
        tone: toneOf(chat.status),
        spin: chat.status === "working",
      });
      edges.push({ from: wsId, to: id });
    });
    next += rows;
    void i;
  });
  const middle = workspaceYs.length
    ? (workspaceYs[0] + workspaceYs[workspaceYs.length - 1]) / 2
    : 0;
  nodes.unshift({ id: "root", col: 0, y: middle, title: name, sub: "Composer", tone: null });
  return { nodes, edges, height: Math.max(next, 1) * row - ROW_GAP };
}

/** The fleet as a graph, left to right: Composer, workspaces, agents. */
export function FleetGraph({
  name,
  groups,
  onOpen,
}: {
  name: string;
  groups: FleetGroup[];
  onOpen: (workspaceId: string) => void;
}) {
  const { nodes, edges, height } = layout(name, groups);
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const x = (col: number) => PAD + col * (NODE_W + COL_GAP);
  const width = PAD * 2 + 3 * NODE_W + 2 * COL_GAP;
  return (
    <section
      aria-label="Fleet graph"
      className="relative"
      style={{ width, height: height + PAD * 2 }}
    >
      <svg aria-hidden width={width} height={height + PAD * 2} className="absolute inset-0">
        {edges.map((e) => {
          const a = byId.get(e.from);
          const b = byId.get(e.to);
          if (!a || !b) return null;
          const x1 = x(a.col) + NODE_W;
          const y1 = PAD + a.y + NODE_H / 2;
          const x2 = x(b.col);
          const y2 = PAD + b.y + NODE_H / 2;
          const mid = (x1 + x2) / 2;
          return (
            <path
              key={`${e.from}>${e.to}`}
              d={`M${x1} ${y1} C${mid} ${y1} ${mid} ${y2} ${x2} ${y2}`}
              fill="none"
              className="stroke-border"
              strokeWidth={1.5}
            />
          );
        })}
      </svg>
      {nodes.map((n) => {
        const body = (
          <>
            {n.tone ? (
              <span
                role="img"
                aria-label={
                  n.tone === "waiting" ? "Waiting" : n.tone === "working" ? "Working" : "Idle"
                }
                className={cn("mt-1.5 size-2 shrink-0 rounded-full", DOT[n.tone])}
              />
            ) : (
              <span aria-hidden className="mt-1.5 size-2 shrink-0 rounded-full bg-foreground" />
            )}
            <span className="min-w-0 flex-1 text-left">
              <span className="flex items-center gap-1 truncate text-sm font-medium">
                {n.spin ? <Loader2Icon className="size-3 shrink-0 animate-spin" /> : null}
                <span className="truncate">{n.title}</span>
              </span>
              {n.sub ? (
                <span className="block truncate text-xs text-muted-foreground">{n.sub}</span>
              ) : null}
            </span>
          </>
        );
        const cls = cn(
          "absolute flex items-start gap-2 rounded-md border bg-background px-2.5 py-2",
          n.tone === "waiting" ? "border-warning/50" : "border-border",
        );
        const style = { left: x(n.col), top: PAD + n.y, width: NODE_W, height: NODE_H };
        return n.open ? (
          <button
            key={n.id}
            type="button"
            aria-label={`Open ${n.title}`}
            onClick={() => onOpen(n.open as string)}
            className={cn(
              cls,
              "outline-none hover:bg-muted/50 focus-visible:ring-2 focus-visible:ring-ring/60",
            )}
            style={style}
          >
            {body}
          </button>
        ) : (
          <div key={n.id} className={cls} style={style}>
            {body}
          </div>
        );
      })}
    </section>
  );
}
