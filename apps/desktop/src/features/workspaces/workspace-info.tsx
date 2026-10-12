import type { Project } from "@circles/bindings/Project";
import type { Workspace } from "@circles/bindings/Workspace";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@circles/ui/components/dialog";
import { DropdownMenuItem, DropdownMenuSeparator } from "@circles/ui/components/dropdown-menu";
import { toast } from "sonner";
import { useEditors } from "@/features/files/api";
import { ipc } from "@/lib/ipc";

/** Project, branch, base and folder of a workspace. */
export function WorkspaceDetailsDialog({
  workspace,
  project,
  open,
  onOpenChange,
}: {
  workspace: Workspace;
  project: Project | undefined;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{workspace.title ?? workspace.city}</DialogTitle>
          <DialogDescription>Where this workspace lives.</DialogDescription>
        </DialogHeader>
        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 text-xs">
          <dt className="text-muted-foreground">Project</dt>
          <dd>{project?.name ?? "…"}</dd>
          <dt className="text-muted-foreground">Branch</dt>
          <dd className="font-mono break-all">{workspace.branch}</dd>
          <dt className="text-muted-foreground">Base</dt>
          <dd className="font-mono">{workspace.base_branch}</dd>
          <dt className="text-muted-foreground">Folder</dt>
          <dd className="font-mono break-all">{workspace.path}</dd>
        </dl>
      </DialogContent>
    </Dialog>
  );
}

const fail = (e: unknown) => toast.error(e instanceof Error ? e.message : String(e));

/**
 * The entries at the top of a workspace's `…` menu: open it in an editor, show it in Finder, copy
 * its path or its pull request link, and see its details.
 */
export function WorkspaceMenuItems({
  workspace,
  onShowDetails,
}: {
  workspace: Workspace;
  onShowDetails: () => void;
}) {
  const editors = useEditors().data ?? [];
  const copy = (text: string, done: string) =>
    navigator.clipboard
      ?.writeText(text)
      .then(() => toast(done))
      .catch(() => toast.error("Could not copy"));
  return (
    <>
      {editors.map((editor) => (
        <DropdownMenuItem
          key={editor.id}
          onClick={() => ipc.editorOpen(workspace.id, editor.id).catch(fail)}
        >
          Open in {editor.name}
        </DropdownMenuItem>
      ))}
      {editors.length === 0 ? (
        <DropdownMenuItem disabled>No editor found (Cursor, VS Code)</DropdownMenuItem>
      ) : null}
      <DropdownMenuItem onClick={() => ipc.fsReveal(workspace.id).catch(fail)}>
        Reveal in Finder
      </DropdownMenuItem>
      <DropdownMenuItem onClick={() => copy(workspace.path, "Path copied")}>
        Copy path
      </DropdownMenuItem>
      <DropdownMenuItem
        disabled={!workspace.pr_url}
        onClick={() => workspace.pr_url && copy(workspace.pr_url, "Pull request link copied")}
      >
        {workspace.pr_url
          ? "Copy pull request link"
          : "Copy pull request link (no pull request yet)"}
      </DropdownMenuItem>
      <DropdownMenuItem onClick={onShowDetails}>Workspace details</DropdownMenuItem>
      <DropdownMenuSeparator />
    </>
  );
}
