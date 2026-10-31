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
import { ChevronRightIcon, LaptopIcon, ListIcon } from "lucide-react";
import { useState } from "react";
import { useMachineName } from "@/features/app/api";
import { useChat } from "@/stores/chat";
import { useUi } from "@/stores/ui";
import { useApprovals, useFleet } from "./api";
import { ComposerPanel, FleetCard, fleetTone, groupFleet, type Tab } from "./composer-panel";

const MORE: { tab: Tab; label: string; hint: string }[] = [
  { tab: "fleet", label: "Fleet", hint: "Every workspace and agent, and the graph" },
  { tab: "approvals", label: "Approvals", hint: "What waits for you, and what was answered" },
  { tab: "steps", label: "Steps", hint: "Every reply and tool call, with its cost" },
  { tab: "details", label: "Details", hint: "Instructions, notes and autonomy" },
];

/** What this run started, and the way in to everything the right pane used to hold. */
function RunContent({
  composer,
  runId,
  onMore,
}: {
  composer: Composer;
  runId?: string;
  onMore: (tab: Tab) => void;
}) {
  const machine = useMachineName();
  const fleet = useFleet(composer.id, runId);
  const pending = (useApprovals({ composerId: composer.id, onlyPending: true }).data ?? []).length;
  const groups = groupFleet(fleet.data ?? []);
  return (
    <div className="flex flex-col text-sm">
      <p className="flex items-center gap-2 px-3 py-2.5 text-muted-foreground">
        <LaptopIcon className="size-4 shrink-0" />
        <span className="min-w-0 truncate text-foreground">{machine}</span>
      </p>
      <section aria-label="Workspaces" className="flex flex-col gap-2 border-t border-border p-3">
        {groups.length === 0 ? (
          <p className="text-xs text-muted-foreground">
            This run has not started any workspace yet.
          </p>
        ) : (
          groups.map((group) => (
            <FleetCard
              key={group.key}
              group={group}
              tone={fleetTone(group)}
              name={group.workspace?.title ?? group.workspace?.city ?? "A removed workspace"}
            />
          ))
        )}
      </section>
      <ul aria-label="More" className="flex flex-col border-t border-border py-1">
        {MORE.map(({ tab, label, hint }) => (
          <li key={tab}>
            <button
              type="button"
              title={hint}
              onClick={() => onMore(tab)}
              className="flex w-full items-center gap-2 px-3 py-2 text-left text-sm outline-none hover:bg-muted/50 focus-visible:bg-muted/50"
            >
              <span className="min-w-0 flex-1">{label}</span>
              {tab === "approvals" && pending > 0 ? (
                <span className="rounded-full bg-warning/20 px-1.5 text-xs text-warning">
                  {pending}
                </span>
              ) : null}
              <ChevronRightIcon className="size-3.5 text-muted-foreground" />
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}

/** The button at the top right of a Composer's page. It opens what the run started and, in a
 * dialog, what the right pane used to show: the fleet, approvals, steps and details. */
export function RunPopover({ composer }: { composer: Composer }) {
  const open = useUi((s) => s.runOpen);
  const setOpen = useUi((s) => s.setRunOpen);
  const runId = useChat((s) => s.activeTab[composer.workspace_id]);
  const pending = (useApprovals({ composerId: composer.id, onlyPending: true }).data ?? []).length;
  const [dialog, setDialog] = useState<Tab | null>(null);

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
            onMore={(tab) => {
              setOpen(false);
              setDialog(tab);
            }}
          />
        </PopoverContent>
      </Popover>
      <Dialog open={dialog !== null} onOpenChange={(next) => (next ? null : setDialog(null))}>
        <DialogContent className="flex h-[80vh] flex-col gap-0 overflow-hidden p-0 sm:max-w-3xl">
          <DialogTitle className="sr-only">{composer.name}: this run</DialogTitle>
          <DialogDescription className="sr-only">
            The fleet, approvals, steps and details of this run.
          </DialogDescription>
          {dialog ? (
            <ComposerPanel key={dialog} composer={composer} runId={runId} initialTab={dialog} />
          ) : null}
        </DialogContent>
      </Dialog>
    </>
  );
}
