import type { NextAction } from "@circles/bindings/NextAction";
import type { PrStatus } from "@circles/bindings/PrStatus";
import type { Readiness } from "@circles/bindings/Readiness";
import type { Workspace } from "@circles/bindings/Workspace";
import { Button } from "@circles/ui/components/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@circles/ui/components/dialog";
import { cn } from "@circles/ui/utils";
import { useQueryClient } from "@tanstack/react-query";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  ArchiveIcon,
  ClockIcon,
  ExternalLinkIcon,
  GitCompareIcon,
  GitMergeIcon,
  GitPullRequestIcon,
  Loader2Icon,
  MessageSquareIcon,
  WrenchIcon,
} from "lucide-react";
import type { ReactNode } from "react";
import { useState } from "react";
import { toast } from "sonner";
import { archiveWorkspace } from "@/components/shell/shell-actions";
import { useActiveSession } from "@/features/chat/api";
import { prKeys, useReadiness } from "@/features/pr/api";
import { startPullRequest } from "@/features/pr/flow";
import { workspaceKeys } from "@/features/workspaces/api";
import { ipc } from "@/lib/ipc";
import { TOAST_WITH_ACTION_MS } from "@/lib/toast";

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

const LABEL: Record<NextAction, string> = {
  create_pr: "Create PR",
  resolve_conflicts: "Resolve conflicts",
  fix_checks: "Fix failing checks",
  respond_to_review: "Respond to review",
  wait_for_checks: "Checks running…",
  merge: "Merge",
  archive: "Archive",
  none: "",
};

const ICON: Partial<Record<NextAction, ReactNode>> = {
  create_pr: <GitPullRequestIcon />,
  resolve_conflicts: <GitCompareIcon />,
  fix_checks: <WrenchIcon />,
  respond_to_review: <MessageSquareIcon />,
  wait_for_checks: <ClockIcon />,
  merge: <GitMergeIcon />,
  archive: <ArchiveIcon />,
};

/** The next-step button is the same tint as the badge, with a border of the colour. */
const BUTTON: Record<Tone | "warning", string> = {
  success: "border-success/40 bg-success/20 text-success hover:bg-success/30 hover:text-success",
  info: "border-info/40 bg-info/20 text-info hover:bg-info/30 hover:text-info",
  violet: "border-violet/40 bg-violet/20 text-violet hover:bg-violet/30 hover:text-violet",
  destructive:
    "border-destructive/40 bg-destructive/20 text-destructive hover:bg-destructive/30 hover:text-destructive",
  warning: "border-warning/40 bg-warning/20 text-warning hover:bg-warning/30 hover:text-warning",
  muted: "border-border bg-muted text-muted-foreground hover:bg-muted/80 hover:text-foreground",
};

/** What the button does decides its colour. Archive and waiting follow the pull request's state. */
const ACTION_COLOR: Partial<Record<NextAction, Tone | "warning">> = {
  create_pr: "violet",
  fix_checks: "destructive",
  resolve_conflicts: "destructive",
  respond_to_review: "warning",
  merge: "success",
};

export type Tone = "success" | "info" | "violet" | "destructive" | "muted";

/** The row, the pull request number and the badge share one colour, the row's a faint tint of it. */
export const TONE: Record<Tone, { row: string; chip: string; badge: string }> = {
  success: {
    row: "bg-success/10 border-success/30",
    chip: "bg-success/20 text-success",
    badge: "bg-success/20 text-success",
  },
  info: {
    row: "bg-info/10 border-info/30",
    chip: "bg-info/20 text-info",
    badge: "bg-info/20 text-info",
  },
  violet: {
    row: "bg-violet/10 border-violet/30",
    chip: "bg-violet/20 text-violet",
    badge: "bg-violet/20 text-violet",
  },
  destructive: {
    row: "bg-destructive/10 border-destructive/30",
    chip: "bg-destructive/20 text-destructive",
    badge: "bg-destructive/20 text-destructive",
  },
  muted: {
    row: "bg-muted/40",
    chip: "bg-muted text-muted-foreground",
    badge: "bg-muted text-muted-foreground",
  },
};

/** What the badge says about the pull request, and in which colour. */
export function stateOf(
  pr: Pick<PrStatus, "state" | "is_draft">,
  ready: boolean,
): { label: string; tone: Tone } {
  if (pr.state === "merged") return { label: "Merged", tone: "info" };
  if (pr.state === "closed") return { label: "Closed", tone: "destructive" };
  if (pr.is_draft) return { label: "Draft", tone: "muted" };
  return { label: ready ? "Ready to merge" : "Open", tone: "success" };
}

/** Merging, with the reasons not to and the explicit choice to merge anyway. */
function MergeDialog({
  workspace,
  readiness,
  open,
  onOpenChange,
}: {
  workspace: Workspace;
  readiness: Readiness;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const queryClient = useQueryClient();
  const [anyway, setAnyway] = useState(false);
  const [merging, setMerging] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const blockers = readiness.blockers;
  const hard = blockers.some((b) => !b.overridable);
  const canMerge = !hard && (blockers.length === 0 || anyway);

  const merge = async () => {
    setMerging(true);
    setError(null);
    try {
      const pr = await ipc.prMerge(workspace.id, anyway);
      await queryClient.invalidateQueries({ queryKey: prKeys.all(workspace.id) });
      onOpenChange(false);
      toast(`Merged pull request #${pr.number}`, {
        duration: TOAST_WITH_ACTION_MS,
        action: {
          label: "Archive workspace",
          onClick: () => void archiveWorkspace(queryClient, workspace.id, workspace.city, false),
        },
      });
    } catch (e) {
      setError(message(e));
    } finally {
      setMerging(false);
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) setAnyway(false);
        onOpenChange(next);
      }}
    >
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Merge #{readiness.pr?.number}?</DialogTitle>
          <DialogDescription>{readiness.pr?.title}</DialogDescription>
        </DialogHeader>
        {blockers.length === 0 ? (
          <p className="text-sm text-muted-foreground">Everything looks ready.</p>
        ) : (
          <div className="flex flex-col gap-2">
            <p className="text-sm font-medium">Before you merge:</p>
            <ul className="flex list-disc flex-col gap-1 pl-5 text-sm">
              {blockers.map((b) => (
                <li key={b.kind}>{b.message}</li>
              ))}
            </ul>
            {hard ? null : (
              <label className="mt-1 flex items-center gap-2 text-sm">
                <input
                  type="checkbox"
                  checked={anyway}
                  onChange={(e) => setAnyway(e.target.checked)}
                  className="size-4 accent-foreground"
                />
                Merge anyway
              </label>
            )}
          </div>
        )}
        {error ? (
          <p role="alert" className="text-sm text-destructive">
            {error}
          </p>
        ) : null}
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button disabled={!canMerge || merging} onClick={() => void merge()}>
            {merging ? <Loader2Icon className="animate-spin" /> : null}
            Merge
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

/**
 * The pull request chip, the "ready to merge" pill and the one action that is most worth taking
 * next, for the top bar.
 */
export function NextActionBar({
  workspace,
  className,
}: {
  workspace: Workspace;
  className?: string;
}) {
  const usable = workspace.state !== "archived" && workspace.state !== "errored";
  const readiness = useReadiness(workspace.id, usable);
  const queryClient = useQueryClient();
  const { active } = useActiveSession(workspace.id);
  const [mergeOpen, setMergeOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const data = readiness.data;
  if (!data || !usable) return null;

  const refresh = () => queryClient.invalidateQueries({ queryKey: prKeys.all(workspace.id) });
  const toAgent = async (send: (sessionId: string) => Promise<unknown>, started: string) => {
    if (!active) {
      toast.error("Open a chat first.");
      return;
    }
    setBusy(true);
    try {
      await send(active.id);
      toast(started);
      void refresh();
    } catch (e) {
      toast.error(message(e));
    } finally {
      setBusy(false);
    }
  };

  const action = data.next_action;
  const run = () => {
    switch (action) {
      case "create_pr":
        return void startPullRequest(queryClient, workspace.id);
      case "merge":
        return setMergeOpen(true);
      case "fix_checks":
        return void toAgent(
          (id) => ipc.agentFixChecks(workspace.id, id),
          "Asked the agent to fix the checks",
        );
      case "respond_to_review":
        return void toAgent(
          (id) => ipc.agentRespondReview(workspace.id, id),
          "Asked the agent to respond to the review",
        );
      case "resolve_conflicts":
        return void toAgent(
          (id) => ipc.agentResolveConflicts(workspace.id, id),
          "Asked the agent to resolve the conflicts",
        );
      case "archive":
        return void archiveWorkspace(queryClient, workspace.id, workspace.city, false).then(() =>
          queryClient.invalidateQueries({ queryKey: workspaceKeys.all }),
        );
      default:
    }
  };

  const pr = data.pr;
  // A branch with nothing to say about gets no bar at all.
  if (!pr && action === "none" && !data.ready && !data.gh_error) return null;
  const state = pr ? stateOf(pr, data.ready) : null;
  // Before there is a pull request, the bar is tinted for the step that starts one.
  const tone = state ? TONE[state.tone] : action === "create_pr" ? TONE.violet : null;
  return (
    <section
      aria-label="Next step"
      className={cn("flex shrink-0 items-center gap-2", className, tone?.row)}
    >
      {pr ? (
        <button
          type="button"
          aria-label={`Open pull request ${pr.number}`}
          onClick={() => openUrl(pr.url).catch(() => {})}
          className={cn(
            "flex h-6 items-center gap-1 rounded-md px-1.5 text-xs font-medium hover:brightness-110",
            tone?.chip,
          )}
        >
          #{pr.number}
          <ExternalLinkIcon className="size-3" />
        </button>
      ) : null}
      {state ? (
        <span
          data-testid="pr-state"
          className={cn("flex h-6 items-center rounded-md px-2 text-xs font-medium", tone?.badge)}
        >
          {state.label}
        </span>
      ) : null}
      <div className="ml-auto flex min-w-0 items-center gap-1.5">
        {data.gh_error && !pr ? (
          <span className="text-xs text-muted-foreground" title={data.gh_error.message}>
            GitHub unavailable
          </span>
        ) : null}
        {action !== "none" ? (
          <Button
            size="sm"
            variant="outline"
            disabled={busy || action === "wait_for_checks"}
            onClick={run}
            className={cn(
              "h-6 gap-1.5 rounded-md px-2 text-xs [&_svg]:size-3",
              BUTTON[ACTION_COLOR[action] ?? state?.tone ?? "muted"],
            )}
          >
            {busy ? <Loader2Icon className="animate-spin" /> : ICON[action]}
            {LABEL[action]}
          </Button>
        ) : null}
      </div>
      <MergeDialog
        workspace={workspace}
        readiness={data}
        open={mergeOpen}
        onOpenChange={setMergeOpen}
      />
    </section>
  );
}
