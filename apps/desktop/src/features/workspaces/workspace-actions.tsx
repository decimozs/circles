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
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@circles/ui/components/dropdown-menu";
import { Input } from "@circles/ui/components/input";
import { Switch } from "@circles/ui/components/switch";
import { cn } from "@circles/ui/utils";
import { MoreHorizontalIcon } from "lucide-react";
import { type ReactNode, useState } from "react";
import { toast } from "sonner";
import { ErrorMessage } from "@/features/projects/error-message";
import {
  useArchiveWorkspace,
  useDeleteWorkspace,
  usePinWorkspace,
  useRenameBranch,
  useRestoreWorkspace,
} from "@/features/workspaces/api";
import { errorKind } from "@/lib/ipc";

type OpenDialog = "rename" | "dirty" | "delete" | null;

/** Actions menu for one workspace, with the confirmations that protect uncommitted work. */
export function WorkspaceActions({
  workspace,
  onDeleted,
  label,
  className,
  extra,
}: {
  workspace: Workspace;
  onDeleted?: () => void;
  /** Entries shown above the usual ones, in the same menu. */
  extra?: ReactNode;
  /** Accessible name of the button, when the default would repeat on the same screen. */
  label?: string;
  className?: string;
}) {
  const archive = useArchiveWorkspace();
  const restore = useRestoreWorkspace();
  const rename = useRenameBranch();
  const pin = usePinWorkspace();
  const remove = useDeleteWorkspace();

  const [dialog, setDialog] = useState<OpenDialog>(null);
  const [dirtyMessage, setDirtyMessage] = useState("");
  const [branch, setBranch] = useState(workspace.branch);
  const [deleteBranch, setDeleteBranch] = useState(false);

  const state = workspace.state;
  const provisioning = state === "creating" || state === "setup_running";
  const canArchive = state === "ready" || state === "idle" || state === "errored";
  const canRestore = state === "archived" || state === "errored";
  const canDelete = state === "archived" || state === "errored";

  const runArchive = (commitDirty: boolean) =>
    archive.mutate(
      { id: workspace.id, commitDirty },
      {
        onSuccess: () => {
          setDialog(null);
          toast(`Archived ${workspace.city}`, {
            description: `The branch ${workspace.branch} was kept.`,
          });
        },
        onError: (error) => {
          if (errorKind(error) === "dirty_tree") {
            setDirtyMessage(error.message);
            setDialog("dirty");
          } else {
            setDialog(null);
            toast.error(error.message);
          }
        },
      },
    );

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger
          render={
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={label ?? `Actions for ${workspace.city}`}
              className={cn("text-muted-foreground", className)}
            />
          }
        >
          <MoreHorizontalIcon />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-60">
          {extra}
          <DropdownMenuItem
            onClick={() => pin.mutate({ id: workspace.id, pinned: !workspace.pinned })}
          >
            {workspace.pinned ? "Unpin" : "Pin"}
          </DropdownMenuItem>
          <DropdownMenuItem
            disabled={provisioning}
            onClick={() => {
              setBranch(workspace.branch);
              rename.reset();
              setDialog("rename");
            }}
          >
            Rename branch
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          {canArchive ? (
            <DropdownMenuItem disabled={archive.isPending} onClick={() => runArchive(false)}>
              Archive
            </DropdownMenuItem>
          ) : null}
          {canRestore ? (
            <DropdownMenuItem
              disabled={restore.isPending}
              onClick={() =>
                restore.mutate(workspace.id, {
                  onSuccess: () => toast.success(`Restored ${workspace.city}`),
                  onError: (error) => toast.error(error.message),
                })
              }
            >
              Restore
            </DropdownMenuItem>
          ) : null}
          {canDelete ? (
            <DropdownMenuItem
              className="text-destructive"
              onClick={() => {
                setDeleteBranch(false);
                remove.reset();
                setDialog("delete");
              }}
            >
              Delete
            </DropdownMenuItem>
          ) : null}
        </DropdownMenuContent>
      </DropdownMenu>

      <Dialog open={dialog === "rename"} onOpenChange={(o) => !o && setDialog(null)}>
        <DialogContent className="sm:max-w-md">
          <form
            className="flex flex-col gap-4"
            onSubmit={(e) => {
              e.preventDefault();
              rename.mutate(
                { id: workspace.id, name: branch },
                { onSuccess: () => setDialog(null) },
              );
            }}
          >
            <DialogHeader>
              <DialogTitle>Rename branch</DialogTitle>
              <DialogDescription>
                Renames the git branch of {workspace.city}. Anything already pushed keeps its old
                name.
              </DialogDescription>
            </DialogHeader>
            <Input
              value={branch}
              onChange={(e) => setBranch(e.target.value)}
              aria-label="Branch name"
              className="font-mono text-xs"
              autoFocus
            />
            <ErrorMessage error={rename.error} />
            <DialogFooter>
              <Button type="button" variant="outline" onClick={() => setDialog(null)}>
                Cancel
              </Button>
              <Button type="submit" disabled={rename.isPending || !branch.trim()}>
                Rename
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

      <Dialog open={dialog === "dirty"} onOpenChange={(o) => !o && setDialog(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Uncommitted changes</DialogTitle>
            <DialogDescription>{dirtyMessage}</DialogDescription>
          </DialogHeader>
          <p className="text-sm text-muted-foreground">
            Auto-commit saves everything on the branch with the message "WIP: archived from
            Circles", so nothing is lost and restoring brings it all back.
          </p>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDialog(null)}>
              Cancel
            </Button>
            <Button disabled={archive.isPending} onClick={() => runArchive(true)}>
              Commit and archive
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={dialog === "delete"} onOpenChange={(o) => !o && setDialog(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Delete {workspace.city}?</DialogTitle>
            <DialogDescription>
              Removes the workspace from Circles. The branch stays in your repository unless you
              choose to delete it too.
            </DialogDescription>
          </DialogHeader>
          <div className="flex items-center gap-2 text-sm">
            <Switch id="delete-branch" checked={deleteBranch} onCheckedChange={setDeleteBranch} />
            <label htmlFor="delete-branch">
              Also delete the branch <span className="font-mono text-xs">{workspace.branch}</span>
            </label>
          </div>
          <ErrorMessage error={remove.error} />
          <DialogFooter>
            <Button variant="outline" onClick={() => setDialog(null)}>
              Cancel
            </Button>
            <Button
              variant="destructive"
              disabled={remove.isPending}
              onClick={() =>
                remove.mutate(
                  { id: workspace.id, deleteBranch },
                  {
                    onSuccess: () => {
                      setDialog(null);
                      toast(`Deleted ${workspace.city}`);
                      onDeleted?.();
                    },
                  },
                )
              }
            >
              Delete
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
