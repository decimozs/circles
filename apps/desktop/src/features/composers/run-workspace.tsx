import type { Workspace } from "@circles/bindings/Workspace";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@circles/ui/components/dropdown-menu";
import { cn } from "@circles/ui/utils";
import {
  ChevronDownIcon,
  CircleCheckIcon,
  CircleDotDashedIcon,
  CircleXIcon,
  CodeIcon,
  FileDiffIcon,
  GitBranchIcon,
} from "lucide-react";
import { useNavigate } from "react-router";
import { toast } from "sonner";
import { useChanges, useEditors } from "@/features/files/api";
import { useReadiness } from "@/features/pr/api";
import { NextActionBar } from "@/features/pr/next-action";
import { ipc } from "@/lib/ipc";
import { useReview } from "@/stores/review";

const fail = (e: unknown) => toast.error(e instanceof Error ? e.message : String(e));

const ROW =
  "flex w-full items-center gap-2 rounded-md px-1.5 py-1 text-left text-xs outline-none hover:bg-muted/50 focus-visible:bg-muted/50";

/** Open the workspace in an editor: the first one found, with a menu for the others. */
function EditorRow({ workspace }: { workspace: Workspace }) {
  const editors = useEditors().data ?? [];
  const first = editors[0];
  if (!first) {
    return (
      <p className={cn(ROW, "text-muted-foreground")}>
        <CodeIcon className="size-3.5 shrink-0" />
        No editor found (Cursor, VS Code)
      </p>
    );
  }
  return (
    <div className="flex items-center">
      <button
        type="button"
        className={cn(ROW, "flex-1")}
        onClick={() => ipc.editorOpen(workspace.id, first.id).catch(fail)}
      >
        <CodeIcon className="size-3.5 shrink-0 text-muted-foreground" />
        Open in {first.name}
      </button>
      {editors.length > 1 ? (
        <DropdownMenu>
          <DropdownMenuTrigger
            aria-label="Choose an editor"
            className="rounded-md p-1.5 text-muted-foreground outline-none hover:bg-muted/50 focus-visible:bg-muted/50"
          >
            <ChevronDownIcon className="size-3.5" />
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            {editors.map((editor) => (
              <DropdownMenuItem
                key={editor.id}
                onClick={() => ipc.editorOpen(workspace.id, editor.id).catch(fail)}
              >
                Open in {editor.name}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      ) : null}
    </div>
  );
}

/** What a workspace of the run holds: where to open it, its branch, its pull request and what
 * to do next about it, and how much it changed. Asked for only while it is open. */
export function WorkspaceDetails({ workspace }: { workspace: Workspace }) {
  const navigate = useNavigate();
  const usable = workspace.state !== "archived" && workspace.state !== "errored";
  const readiness = useReadiness(workspace.id, usable).data;
  const changes = useChanges(usable ? workspace.id : undefined, "all").data ?? [];
  const added = changes.reduce((n, f) => n + f.added, 0);
  const removed = changes.reduce((n, f) => n + f.removed, 0);
  const git = readiness?.git;
  const pr = readiness?.pr ?? null;
  const checks = readiness?.checks;

  return (
    <section
      aria-label={`${workspace.title ?? workspace.city} details`}
      className="mb-1 ml-5 flex flex-col gap-0.5 border-l border-border pr-2 pl-2 pb-1"
    >
      <EditorRow workspace={workspace} />
      <p
        className={cn(ROW, "cursor-default hover:bg-transparent")}
        title={git?.base ? `from ${git.base}` : undefined}
      >
        <GitBranchIcon className="size-3.5 shrink-0 text-muted-foreground" />
        <span className="min-w-0 flex-1 truncate font-mono">{git?.branch ?? workspace.branch}</span>
        {git && git.ahead > 0 ? (
          <span className="shrink-0 text-muted-foreground">{git.ahead} ahead</span>
        ) : null}
        {git && git.behind > 0 ? (
          <span className="shrink-0 text-warning">{git.behind} behind</span>
        ) : null}
      </p>
      {pr ? (
        <p className={cn(ROW, "cursor-default items-start hover:bg-transparent")}>
          <span className="line-clamp-2 min-w-0 flex-1 break-words">{pr.title}</span>
          {checks && checks.total > 0 ? (
            checks.failed > 0 ? (
              <CircleXIcon
                aria-label={`${checks.failed} checks failing`}
                className="mt-0.5 size-3.5 shrink-0 text-destructive"
              />
            ) : checks.pending > 0 ? (
              <CircleDotDashedIcon
                aria-label={`${checks.pending} checks running`}
                className="mt-0.5 size-3.5 shrink-0 text-warning"
              />
            ) : (
              <CircleCheckIcon
                aria-label="Checks passing"
                className="mt-0.5 size-3.5 shrink-0 text-success"
              />
            )
          ) : null}
        </p>
      ) : null}
      <NextActionBar
        workspace={workspace}
        className="flex-wrap rounded-md border border-transparent px-1.5 py-1"
      />
      <button
        type="button"
        className={ROW}
        onClick={() => {
          useReview.getState().setRightTab("changes");
          navigate(`/workspace/${workspace.id}`);
        }}
      >
        <FileDiffIcon className="size-3.5 shrink-0 text-muted-foreground" />
        <span className="flex-1">Changes</span>
        <span className="tabular-nums">
          <span className="text-success">+{added}</span>{" "}
          <span className="text-destructive">-{removed}</span>
        </span>
      </button>
    </section>
  );
}
