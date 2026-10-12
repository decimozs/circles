import type { BranchState } from "@circles/bindings/BranchState";
import type { WorkspaceState } from "@circles/bindings/WorkspaceState";
import { cn } from "@circles/ui/utils";
import {
  GitBranchIcon,
  GitMergeIcon,
  GitPullRequestClosedIcon,
  GitPullRequestDraftIcon,
  GitPullRequestIcon,
  Loader2Icon,
} from "lucide-react";

export const STATE_LABEL: Record<WorkspaceState, string> = {
  creating: "Creating",
  setup_running: "Setting up",
  ready: "Ready",
  working: "Working",
  idle: "Idle",
  blocked: "Needs approval",
  archived: "Archived",
  errored: "Error",
};

/** The icon, colour and words for where a branch stands. Nothing special for a plain branch. */
export function branchLook(branch: BranchState | undefined): {
  Icon: typeof GitBranchIcon;
  tone: string;
  label: string | null;
} {
  if (branch?.pr_state === "merged") {
    return { Icon: GitMergeIcon, tone: "text-info", label: "Merged" };
  }
  if (branch?.pr_state === "closed") {
    return { Icon: GitPullRequestClosedIcon, tone: "text-destructive", label: "Closed" };
  }
  if (branch?.pr_state === "open") {
    return branch.is_draft
      ? { Icon: GitPullRequestDraftIcon, tone: "text-muted-foreground", label: "Draft" }
      : { Icon: GitPullRequestIcon, tone: "text-success", label: "Open" };
  }
  if (branch && branch.ahead > 0) {
    return {
      Icon: GitBranchIcon,
      tone: "text-violet",
      label: branch.ahead === 1 ? "1 commit" : `${branch.ahead} commits`,
    };
  }
  return { Icon: GitBranchIcon, tone: "text-muted-foreground", label: null };
}

/** Small status marker for sidebar rows and lists. */
export function StatusIndicator({
  state,
  className,
  spinner = true,
  plain = false,
  branch,
}: {
  state: WorkspaceState;
  className?: string;
  /** Show a spinner while the workspace is busy. Off, the branch icon stays and the state is told in words. */
  spinner?: boolean;
  /** Always the branch icon for a workspace that is in use. The state is shown some other way. */
  plain?: boolean;
  /** Where the workspace's branch stands. In the plain icon, it picks the icon and its colour. */
  branch?: BranchState;
}) {
  if (plain && state !== "creating" && state !== "setup_running" && state !== "archived") {
    const look = branchLook(branch);
    return (
      <look.Icon
        aria-label={look.label ?? STATE_LABEL[state]}
        className={cn("size-3.5 shrink-0", look.tone, className)}
      />
    );
  }
  if (spinner && (state === "creating" || state === "setup_running" || state === "working")) {
    return (
      <Loader2Icon
        aria-label={STATE_LABEL[state]}
        className={cn("size-3.5 shrink-0 animate-spin text-muted-foreground", className)}
      />
    );
  }
  if (state === "blocked") {
    return (
      <span
        role="img"
        aria-label={STATE_LABEL[state]}
        className={cn("size-2 shrink-0 rounded-full bg-warning", className)}
      />
    );
  }
  if (state === "errored") {
    return (
      <span
        role="img"
        aria-label={STATE_LABEL[state]}
        className={cn("size-2 shrink-0 rounded-full bg-destructive", className)}
      />
    );
  }
  return (
    <GitBranchIcon
      aria-label={STATE_LABEL[state]}
      className={cn("size-3.5 shrink-0 text-muted-foreground", className)}
    />
  );
}
