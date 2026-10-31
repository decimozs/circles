import type { Composer } from "@circles/bindings/Composer";
import { Button } from "@circles/ui/components/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@circles/ui/components/dialog";
import { Popover, PopoverContent, PopoverTrigger } from "@circles/ui/components/popover";
import { cn } from "@circles/ui/utils";
import {
  CheckIcon,
  ChevronDownIcon,
  ChevronRightIcon,
  LaptopIcon,
  ListIcon,
  Loader2Icon,
  SettingsIcon,
} from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router";
import { useMachineName } from "@/features/app/api";
import { useNow } from "@/hooks/use-now";
import { formatAge, formatElapsed } from "@/lib/time";
import { useChat } from "@/stores/chat";
import { useUi } from "@/stores/ui";
import { useApprovals, useFleet } from "./api";
import {
  ComposerSettingsDialog,
  FleetTab,
  groupFleet,
  IntegrationsSection,
  RepositoriesSection,
} from "./composer-panel";
import { type LiveRow, liveRows, runningCount } from "./live";
import { ApprovalsSection } from "./run-approvals";

/** What a row is doing, as a picture and, for people who do not see it, in words. */
function Marker({ row }: { row: LiveRow }) {
  if (row.state === "working") {
    return (
      <>
        <Loader2Icon
          aria-hidden
          className="size-3.5 shrink-0 animate-spin text-success motion-reduce:animate-none"
        />
        <span className="sr-only">Working</span>
      </>
    );
  }
  if (row.state === "waiting") {
    return (
      <>
        <span
          aria-hidden
          className="size-2 shrink-0 animate-pulse rounded-full bg-warning motion-reduce:animate-none"
        />
        <span className="sr-only">Waiting for you</span>
      </>
    );
  }
  return row.done ? (
    <>
      <CheckIcon aria-hidden className="size-3.5 shrink-0 text-muted-foreground" />
      <span className="sr-only">Done</span>
    </>
  ) : (
    <>
      <span aria-hidden className="size-2 shrink-0 rounded-full bg-muted-foreground/40" />
      <span className="sr-only">Idle</span>
    </>
  );
}

function FleetRow({ row, now }: { row: LiveRow; now: number }) {
  const navigate = useNavigate();
  const time =
    row.state === "idle"
      ? row.lastActive > 0
        ? `${formatAge(row.lastActive, now)}${formatAge(row.lastActive, now) === "now" ? "" : " ago"}`
        : ""
      : formatElapsed(now - (row.since ?? now));
  return (
    <li>
      <button
        type="button"
        disabled={!row.workspaceId}
        aria-label={`Open ${row.name}`}
        onClick={() => row.workspaceId && navigate(`/workspace/${row.workspaceId}`)}
        className="flex w-full items-center gap-2.5 px-3 py-2 text-left text-sm outline-none hover:bg-muted/50 focus-visible:bg-muted/50 disabled:hover:bg-transparent"
      >
        <Marker row={row} />
        <span
          className={cn("min-w-0 flex-1 truncate", row.state === "idle" && "text-muted-foreground")}
        >
          {row.name}
        </span>
        <span className="shrink-0 text-xs tabular-nums text-muted-foreground">{time}</span>
      </button>
    </li>
  );
}

/** The workspaces this run started, live: what works, what waits for you, and what is done. */
function FleetSection({ rows, now }: { rows: LiveRow[]; now: number }) {
  const [showDone, setShowDone] = useState(false);
  const running = rows.filter((r) => r.state !== "idle");
  const rest = rows.filter((r) => r.state === "idle");
  // With nothing running, what is there is what there is, so it is not folded away.
  const listed = running.length === 0 ? rest : running;
  const folded = running.length === 0 ? [] : rest;
  return (
    <section aria-label="Fleet" className="flex flex-col border-t border-border py-1">
      <h2 className="flex items-baseline gap-1.5 px-3 pt-2 pb-1 text-xs text-muted-foreground">
        <span>Fleet</span>
        <span role="status" aria-live="polite">
          {rows.length === 0
            ? "· nothing started"
            : running.length > 0
              ? `· ${running.length} running`
              : "· nothing running"}
        </span>
      </h2>
      {rows.length === 0 ? (
        <p className="px-3 pb-2 text-xs text-muted-foreground">
          This run has not started any workspace yet.
        </p>
      ) : (
        <ul aria-label="Workspaces of this run">
          {listed.map((row) => (
            <FleetRow key={row.key} row={row} now={now} />
          ))}
        </ul>
      )}
      {folded.length > 0 ? (
        <>
          <button
            type="button"
            aria-expanded={showDone}
            onClick={() => setShowDone((v) => !v)}
            className="flex items-center gap-1 px-3 py-1.5 text-left text-xs text-muted-foreground hover:text-foreground"
          >
            {showDone ? (
              <ChevronDownIcon className="size-3" />
            ) : (
              <ChevronRightIcon className="size-3" />
            )}
            Done {folded.length}
          </button>
          {showDone ? (
            <ul aria-label="Finished workspaces">
              {folded.map((row) => (
                <FleetRow key={row.key} row={row} now={now} />
              ))}
            </ul>
          ) : null}
        </>
      ) : null}
    </section>
  );
}

/** What this run is doing, and the way in to what is too big for the panel. */
function RunContent({
  composer,
  runId,
  onGraph,
}: {
  composer: Composer;
  runId?: string;
  onGraph: () => void;
}) {
  const machine = useMachineName();
  const fleet = useFleet(composer.id, runId);
  const rows = liveRows(groupFleet(fleet.data ?? []));
  // The clock ticks only while something goes on.
  const now = useNow(runningCount(rows) > 0);
  const [settings, setSettings] = useState(false);
  return (
    <div className="flex flex-col text-sm">
      <p className="flex items-center gap-2 px-3 py-2.5 text-muted-foreground">
        <LaptopIcon className="size-4 shrink-0" />
        <span className="min-w-0 truncate text-foreground">{machine}</span>
      </p>
      <FleetSection rows={rows} now={now} />
      <ApprovalsSection composer={composer} />
      <div className="border-t border-border py-1">
        <button
          type="button"
          title="Every workspace and agent of this run, drawn as a graph"
          onClick={onGraph}
          className="flex w-full items-center gap-2 px-3 py-2 text-left text-sm outline-none hover:bg-muted/50 focus-visible:bg-muted/50"
        >
          <span className="min-w-0 flex-1">Fleet graph</span>
          <ChevronRightIcon className="size-3.5 text-muted-foreground" />
        </button>
      </div>
      <div className="flex flex-col gap-4 border-t border-border p-3">
        <RepositoriesSection composer={composer} />
        <IntegrationsSection composer={composer} />
      </div>
      <div className="border-t border-border p-3">
        <Button
          size="sm"
          variant="outline"
          aria-label="Composer settings"
          onClick={() => setSettings(true)}
        >
          <SettingsIcon />
          Settings
        </Button>
      </div>
      <ComposerSettingsDialog composer={composer} open={settings} onOpenChange={setSettings} />
    </div>
  );
}

/** The button at the top right of a Composer's page. It shows or hides what the run is doing,
 * and shows it too when closed: a spinner while anything works, a number while anything waits. */
export function RunPopover({ composer }: { composer: Composer }) {
  const open = useUi((s) => s.runOpen);
  const setOpen = useUi((s) => s.setRunOpen);
  const runId = useChat((s) => s.activeTab[composer.workspace_id]);
  const pending = (useApprovals({ composerId: composer.id, onlyPending: true }).data ?? []).length;
  const fleet = useFleet(composer.id, runId);
  const going = runningCount(liveRows(groupFleet(fleet.data ?? [])));
  const [graph, setGraph] = useState(false);

  return (
    <>
      <Popover
        open={open}
        onOpenChange={(next, details) => {
          // A toggle: it stays open while you work in the chat, and only the button (or
          // Cmd+Option+B, or Escape) puts it away. Clicking elsewhere does not.
          if (!next && details?.reason === "outside-press") return;
          setOpen(next);
        }}
      >
        <PopoverTrigger
          render={
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label="Run details"
              aria-pressed={open}
              className={cn(
                "relative text-muted-foreground hover:text-foreground",
                open && "bg-muted text-foreground",
              )}
            />
          }
        >
          <ListIcon />
          {going > 0 ? (
            <Loader2Icon
              role="status"
              aria-label={`${going} running`}
              className="absolute -right-0.5 -bottom-0.5 size-3 animate-spin text-success motion-reduce:animate-none"
            />
          ) : null}
          {pending > 0 ? (
            <span
              role="status"
              aria-label={`${pending} waiting for you`}
              className="absolute -top-1 -right-1 flex min-w-4 items-center justify-center rounded-full bg-warning px-1 text-2xs leading-4 text-background"
            >
              {pending}
            </span>
          ) : null}
        </PopoverTrigger>
        <PopoverContent
          align="end"
          aria-label="This run"
          className="max-h-[70vh] w-[22rem] gap-0 overflow-y-auto p-0"
        >
          <RunContent
            composer={composer}
            runId={runId}
            onGraph={() => {
              setOpen(false);
              setGraph(true);
            }}
          />
        </PopoverContent>
      </Popover>
      <Dialog open={graph} onOpenChange={setGraph}>
        <DialogContent className="flex h-[80vh] flex-col gap-0 overflow-hidden p-0 sm:max-w-3xl">
          <DialogTitle className="sr-only">{composer.name}: fleet</DialogTitle>
          <DialogDescription className="sr-only">
            The workspaces and agents of this run, with their state and the graph.
          </DialogDescription>
          {graph ? <FleetTab composer={composer} runId={runId} /> : null}
        </DialogContent>
      </Dialog>
    </>
  );
}
