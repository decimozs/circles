import type { ChangeKind } from "@circles/bindings/ChangeKind";
import type { Checkpoint } from "@circles/bindings/Checkpoint";
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
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Loader2Icon, Undo2Icon } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { ipc } from "@/lib/ipc";
import { TOAST_WITH_ACTION_MS } from "@/lib/toast";
import { checkpointKeys } from "./api";

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

const KIND: Record<ChangeKind, { label: string; tone: string }> = {
  restored: { label: "comes back", tone: "text-success" },
  modified: { label: "goes back to how it was", tone: "text-warning" },
  removed: { label: "is removed", tone: "text-destructive" },
};

/** Undoing the last revert of a chat, from a toast or the banner. */
export async function undoRevert(
  queryClient: ReturnType<typeof useQueryClient>,
  revertId: string,
  sessionId: string,
) {
  try {
    const files = await ipc.checkpointUndoRevert(revertId, sessionId);
    toast(`Undid the revert. ${files.length} ${files.length === 1 ? "file" : "files"} restored.`);
    queryClient.invalidateQueries({ queryKey: ["checkpoints", sessionId] });
  } catch (e) {
    toast.error(message(e));
  }
}

/**
 * The confirmation before going back to before a turn: what is removed from the chat, which files
 * change, and what to watch out for.
 */
export function RevertDialog({
  checkpoint,
  sessionId,
  onClose,
}: {
  checkpoint: Checkpoint | null;
  sessionId: string;
  onClose: () => void;
}) {
  const queryClient = useQueryClient();
  const [working, setWorking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const preview = useQuery({
    queryKey: ["checkpoints", sessionId, "preview", checkpoint?.id],
    queryFn: () => ipc.checkpointPreview((checkpoint as Checkpoint).id),
    enabled: !!checkpoint,
    gcTime: 0,
  });
  const data = preview.data;

  const revert = async () => {
    if (!checkpoint) return;
    setWorking(true);
    setError(null);
    try {
      const outcome = await ipc.checkpointRevert(checkpoint.id);
      await queryClient.invalidateQueries({ queryKey: checkpointKeys.list(sessionId) });
      onClose();
      toast(`Went back to before turn ${checkpoint.turn}`, {
        description: "You can still undo this from the bar above the chat input.",
        duration: TOAST_WITH_ACTION_MS,
        action: {
          label: "Undo",
          onClick: () => void undoRevert(queryClient, outcome.revert_id, sessionId),
        },
      });
    } catch (e) {
      setError(message(e));
    } finally {
      setWorking(false);
    }
  };

  return (
    <Dialog open={checkpoint !== null} onOpenChange={(open) => !open && !working && onClose()}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Revert to before turn {checkpoint?.turn}?</DialogTitle>
          <DialogDescription>
            The files and the chat go back to how they were just before you sent this message.
          </DialogDescription>
        </DialogHeader>

        {preview.isLoading ? (
          <p role="status" className="flex items-center gap-2 text-sm text-muted-foreground">
            <Loader2Icon className="size-4 animate-spin" />
            Looking at what would change…
          </p>
        ) : preview.error ? (
          <p role="alert" className="text-sm text-destructive">
            {message(preview.error)}
          </p>
        ) : data ? (
          <div className="flex flex-col gap-3 text-sm">
            <p>
              This deletes turn {data.checkpoint.turn}
              {data.later_turns > 0
                ? ` and the ${data.later_turns} later ${data.later_turns === 1 ? "turn" : "turns"}`
                : ""}{" "}
              from the chat. You can undo it for an hour, as long as you do not send a new message.
            </p>
            {data.files.length === 0 ? (
              <p className="text-muted-foreground">No file would change.</p>
            ) : (
              <div className="flex flex-col gap-1">
                <p className="text-xs font-medium">
                  {data.files.length} {data.files.length === 1 ? "file" : "files"} would change
                </p>
                <ul
                  aria-label="Files that would change"
                  className="max-h-48 overflow-y-auto rounded-lg border border-border p-1 text-xs"
                >
                  {data.files.map((file) => (
                    <li key={file.path} className="flex items-baseline gap-2 px-2 py-0.5">
                      <span className="min-w-0 flex-1 truncate font-mono" title={file.path}>
                        {file.path}
                      </span>
                      <span className={cn("shrink-0", KIND[file.kind].tone)}>
                        {KIND[file.kind].label}
                      </span>
                    </li>
                  ))}
                </ul>
              </div>
            )}
            {data.other_chats_active > 0 ? (
              <p
                role="status"
                className="rounded-lg border border-warning/60 bg-warning/5 p-2 text-xs"
              >
                {data.other_chats_active === 1
                  ? "Another chat of this workspace is working right now."
                  : `${data.other_chats_active} other chats of this workspace are working right now.`}{" "}
                They share these files, so their work would be changed too.
              </p>
            ) : null}
            {data.checkpoint.skipped.length > 0 ? (
              <p
                role="status"
                className="rounded-lg border border-warning/60 bg-warning/5 p-2 text-xs"
              >
                {data.checkpoint.skipped.length === 1
                  ? "One large new file was too big to save, so it is not restored:"
                  : `${data.checkpoint.skipped.length} large new files were too big to save, so they are not restored:`}{" "}
                <span className="font-mono">{data.checkpoint.skipped.join(", ")}</span>
              </p>
            ) : null}
            {data.running ? (
              <p role="alert" className="text-xs text-destructive">
                A turn is running in this chat. Stop it first, then revert.
              </p>
            ) : null}
          </div>
        ) : null}

        {error ? (
          <p role="alert" className="text-sm text-destructive">
            {error}
          </p>
        ) : null}
        <DialogFooter>
          <Button variant="outline" disabled={working} onClick={onClose}>
            Cancel
          </Button>
          <Button
            variant="destructive"
            disabled={working || !data || data.running}
            onClick={() => void revert()}
          >
            {working ? <Loader2Icon className="animate-spin" /> : <Undo2Icon />}
            Revert
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

/** Shown after a revert, until the chat moves on: a way to take it back. */
export function RevertBanner({
  sessionId,
  revertId,
  turn,
}: {
  sessionId: string;
  revertId: string;
  turn: number;
}) {
  const queryClient = useQueryClient();
  const [busy, setBusy] = useState(false);
  return (
    <div
      role="status"
      className="mx-auto flex w-full max-w-3xl items-center justify-between gap-3 px-6 pb-2 text-xs text-muted-foreground"
    >
      <span>Went back to before turn {turn}. Everything after it is hidden.</span>
      <Button
        size="sm"
        variant="outline"
        disabled={busy}
        onClick={async () => {
          setBusy(true);
          await undoRevert(queryClient, revertId, sessionId);
          setBusy(false);
        }}
      >
        <Undo2Icon />
        Undo revert
      </Button>
    </div>
  );
}
