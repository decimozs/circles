import type { GitGraph } from "@circles/bindings/GitGraph";
import type { GraphRef } from "@circles/bindings/GraphRef";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@circles/ui/components/dialog";
import { cn } from "@circles/ui/utils";
import { ChevronDownIcon, ChevronRightIcon, Maximize2Icon } from "lucide-react";
import { type KeyboardEvent, type PointerEvent, useMemo, useRef, useState } from "react";
import { toast } from "sonner";
import { formatAge } from "@/lib/time";
import { useReview } from "@/stores/review";
import { useGitGraph } from "./api";
import { type GraphRow, layoutGraph, type Segment } from "./layout";

export const ROW = 56;
export const GAP = 12;
const PAD = 10;
const DOT = 5;
/** The dot sits on the line of the commit's title, not in the middle of the row. */
export const MID = 14;

const x = (lane: number) => PAD + lane * GAP;
const colorOf = (index: number) => `var(--graph-${(index % 5) + 1})`;
const yOf = (at: "top" | "mid" | "bottom") => (at === "top" ? 0 : at === "mid" ? MID : ROW);

/** One piece of line. Curves leave and arrive upright, so branches bend like in a railway map. */
function path(segment: Segment): string {
  if (segment.kind === "line") {
    const a = yOf(segment.from);
    const b = yOf(segment.to);
    return `M ${x(segment.lane)} ${a} L ${x(segment.lane)} ${b}`;
  }
  const x1 = x(segment.fromLane);
  const x2 = x(segment.toLane);
  const [y1, y2] = segment.half === "in" ? [0, MID] : [MID, ROW];
  const mid = (y1 + y2) / 2;
  return `M ${x1} ${y1} C ${x1} ${mid}, ${x2} ${mid}, ${x2} ${y2}`;
}

function GraphCell({ row, lanes, head }: { row: GraphRow; lanes: number; head: boolean }) {
  const width = PAD * 2 + (lanes - 1) * GAP;
  return (
    <svg
      width={width}
      height={ROW}
      viewBox={`0 0 ${width} ${ROW}`}
      aria-hidden="true"
      className="shrink-0"
    >
      {row.segments.map((segment, i) => (
        <path
          // biome-ignore lint/suspicious/noArrayIndexKey: segments are positional
          key={i}
          d={path(segment)}
          fill="none"
          stroke={colorOf(segment.kind === "line" ? segment.color : segment.color)}
          strokeWidth="2"
          strokeLinecap="round"
        />
      ))}
      <circle
        cx={x(row.lane)}
        cy={MID}
        r={DOT}
        fill={head ? "var(--background)" : colorOf(row.color)}
        stroke={head ? colorOf(row.color) : "var(--background)"}
        strokeWidth="2"
      />
    </svg>
  );
}

/**
 * Only the two branches the graph is about get a badge: the one checked out and the base. Other
 * branches, remote copies and tags would crowd the rows.
 */
export function shownRefs(refs: GraphRef[], graph: Pick<GitGraph, "branch" | "base">): GraphRef[] {
  const base = graph.base.replace(/^origin\//, "");
  const wanted = refs.filter((r) => r.name === graph.branch || r.name === base);
  // The remote copy of the base stands in only when the local one is not on this commit.
  if (!wanted.some((r) => r.name === base)) {
    const remote = refs.find((r) => r.name === `origin/${base}`);
    if (remote) wanted.push(remote);
  }
  return wanted;
}

const CHIP: Record<string, string> = {
  head: "bg-foreground text-background",
  branch: "bg-muted text-foreground",
  remote: "border border-border text-muted-foreground",
  tag: "bg-warning/20 text-warning",
};

function Row({ row, lanes, graph }: { row: GraphRow; lanes: number; graph: GitGraph }) {
  const { commit } = row;
  const isHead = commit.refs.some((r) => r.kind === "head");
  const short = commit.sha.slice(0, 7);
  const shown = shownRefs(commit.refs, graph);
  return (
    <li>
      <button
        type="button"
        title={`${commit.subject}\n${commit.author}, ${short}`}
        onClick={() => {
          navigator.clipboard
            ?.writeText(commit.sha)
            .then(() => toast(`Copied ${short}`))
            .catch(() => {});
        }}
        className="flex w-full items-start gap-1 pr-3 pl-3 text-left text-xs outline-none hover:bg-muted/60 focus-visible:bg-muted"
        style={{ height: ROW }}
      >
        <GraphCell row={row} lanes={lanes} head={isHead} />
        <span className="flex min-w-0 flex-1 flex-col gap-1.5 py-1.5">
          <span className="flex items-center gap-2">
            <span className="min-w-0 flex-1 truncate">{commit.subject}</span>
            <span className="shrink-0 text-2xs text-muted-foreground">
              {formatAge(commit.time * 1000)}
            </span>
          </span>
          <span className="flex min-w-0 items-center gap-1 overflow-hidden">
            {shown.length > 0 ? (
              shown.map((ref) => (
                <span
                  key={`${ref.kind}-${ref.name}`}
                  className={cn(
                    "min-w-0 max-w-40 shrink truncate rounded px-1.5 py-px text-2xs",
                    CHIP[ref.kind],
                    ref.name === graph.base.replace(/^origin\//, "") && "font-semibold",
                  )}
                >
                  {ref.name}
                </span>
              ))
            ) : (
              <span className="truncate text-2xs text-muted-foreground">
                {commit.author} · {short}
              </span>
            )}
          </span>
        </span>
      </button>
    </li>
  );
}

/** The commits of the graph, newest first. */
function GraphList({
  graph,
  layout,
}: {
  graph: GitGraph;
  layout: { rows: GraphRow[]; lanes: number };
}) {
  return (
    <ol>
      {layout.rows.map((row) => (
        <Row key={row.commit.sha} row={row} lanes={layout.lanes} graph={graph} />
      ))}
      {graph.truncated ? (
        <li className="px-3 py-2 text-2xs text-muted-foreground">Older commits are not shown.</li>
      ) : null}
    </ol>
  );
}

/** The graph never gets smaller than this, and the tabs above it keep at least this much. */
export const MIN_GRAPH = 120;
export const MIN_ABOVE = 160;
const KEY_STEP = 24;

/** The branch of the open workspace next to its base, drawn as a live graph. */
export function GraphPanel({ workspaceId }: { workspaceId: string }) {
  const open = useReview((s) => s.graphOpen);
  const toggle = useReview((s) => s.toggleGraph);
  const height = useReview((s) => s.graphHeight);
  const setHeight = useReview((s) => s.setGraphHeight);
  const box = useRef<HTMLElement>(null);
  const [expanded, setExpanded] = useState(false);
  const query = useGitGraph(workspaceId, open || expanded);

  const room = () => box.current?.parentElement?.getBoundingClientRect();
  const clamp = (value: number) => {
    const total = room()?.height ?? Number.POSITIVE_INFINITY;
    return Math.round(Math.max(MIN_GRAPH, Math.min(value, total - MIN_ABOVE)));
  };
  const drag = (event: PointerEvent<HTMLDivElement>) => {
    if (!event.currentTarget.hasPointerCapture(event.pointerId)) return;
    const bounds = room();
    if (bounds) setHeight(clamp(bounds.bottom - event.clientY));
  };
  const resizeWithKeys = (event: KeyboardEvent<HTMLDivElement>) => {
    const current = height ?? box.current?.getBoundingClientRect().height ?? MIN_GRAPH;
    if (event.key === "ArrowUp") setHeight(clamp(current + KEY_STEP));
    else if (event.key === "ArrowDown") setHeight(clamp(current - KEY_STEP));
    else if (event.key === "Home") setHeight(clamp(Number.POSITIVE_INFINITY));
    else if (event.key === "End") setHeight(MIN_GRAPH);
    else return;
    event.preventDefault();
  };
  const graph = query.data;
  const layout = useMemo(() => (graph ? layoutGraph(graph.commits) : null), [graph]);

  return (
    <section
      ref={box}
      aria-label="Git graph"
      style={open && height !== null ? { height } : undefined}
      className={cn(
        "relative flex min-h-0 shrink-0 flex-col border-t border-border",
        open ? (height === null ? "h-[36%] min-h-40" : "") : "h-9",
      )}
    >
      {open ? (
        // biome-ignore lint/a11y/useSemanticElements: a draggable splitter, not a rule
        <div
          role="separator"
          aria-orientation="horizontal"
          aria-label="Resize graph"
          aria-valuenow={height ?? 0}
          aria-valuemin={MIN_GRAPH}
          aria-valuetext={height === null ? "Default height" : `${height} pixels`}
          tabIndex={0}
          onPointerDown={(e) => e.currentTarget.setPointerCapture(e.pointerId)}
          onPointerMove={drag}
          onKeyDown={resizeWithKeys}
          onDoubleClick={() => setHeight(null)}
          className="absolute inset-x-0 -top-1 z-10 h-2 cursor-row-resize outline-none transition-colors hover:bg-ring/50 focus-visible:bg-ring active:bg-ring"
        />
      ) : null}
      <div className="flex h-9 shrink-0 items-center gap-2 px-3">
        <button
          type="button"
          aria-expanded={open}
          onClick={toggle}
          className="flex items-center gap-1 text-xs font-medium outline-none focus-visible:underline"
        >
          {open ? (
            <ChevronDownIcon className="size-3.5" />
          ) : (
            <ChevronRightIcon className="size-3.5" />
          )}
          Graph
        </button>
        <span className="flex-1" />
        {graph && (graph.ahead > 0 || graph.behind > 0) ? (
          <span className="shrink-0 text-2xs text-muted-foreground" data-testid="graph-counts">
            {graph.ahead > 0 ? `↑${graph.ahead}` : ""} {graph.behind > 0 ? `↓${graph.behind}` : ""}
          </span>
        ) : null}
        <button
          type="button"
          aria-label="Expand graph"
          title="Expand graph"
          onClick={() => setExpanded(true)}
          className="flex size-5 items-center justify-center rounded text-muted-foreground outline-none hover:bg-muted hover:text-foreground focus-visible:bg-muted"
        >
          <Maximize2Icon className="size-3" />
        </button>
      </div>
      {open ? (
        <div className="min-h-0 flex-1 overflow-y-auto">
          {query.error ? (
            <p role="alert" className="px-3 py-2 text-xs text-destructive">
              {(query.error as Error).message}
            </p>
          ) : null}
          {layout && graph ? <GraphList graph={graph} layout={layout} /> : null}
        </div>
      ) : null}
      <Dialog open={expanded} onOpenChange={setExpanded}>
        <DialogContent className="flex h-[88vh] flex-col gap-0 overflow-hidden p-0 sm:max-w-[min(720px,94vw)]">
          <DialogHeader className="shrink-0 border-b border-border px-5 py-3">
            <DialogTitle>Git graph</DialogTitle>
            <DialogDescription className="truncate">
              {graph ? `${graph.branch ?? "detached"} → ${graph.base}` : "Loading the commits."}
            </DialogDescription>
          </DialogHeader>
          <div className="min-h-0 flex-1 overflow-y-auto">
            {query.error ? (
              <p role="alert" className="px-3 py-2 text-xs text-destructive">
                {(query.error as Error).message}
              </p>
            ) : null}
            {layout && graph ? <GraphList graph={graph} layout={layout} /> : null}
          </div>
        </DialogContent>
      </Dialog>
    </section>
  );
}
