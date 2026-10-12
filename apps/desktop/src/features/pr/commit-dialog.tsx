import type { CommitInfo } from "@circles/bindings/CommitInfo";
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
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@circles/ui/components/dropdown-menu";
import { Input } from "@circles/ui/components/input";
import { useQueryClient } from "@tanstack/react-query";
import { ChevronDownIcon, Loader2Icon, SparklesIcon } from "lucide-react";
import { useEffect, useState } from "react";
import { useMatch } from "react-router";
import { toast } from "sonner";
import { fileKeys, useChanges } from "@/features/files/api";
import { prKeys } from "@/features/pr/api";
import { ipc } from "@/lib/ipc";
import { useCreatePrefs } from "@/stores/create-prefs";
import { useUi } from "@/stores/ui";

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

/**
 * Commits the uncommitted changes, which a pull request needs, then goes on to the pull request
 * dialog. Claude groups the files into atomic commits, or everything goes in one.
 */
export function CommitDialog() {
  const workspaceId = useMatch("/workspace/:id")?.params.id;
  const open = useUi((s) => s.commitOpen);
  const setOpen = useUi((s) => s.setCommitOpen);
  const queryClient = useQueryClient();
  const changes = useChanges(workspaceId, "uncommitted").data ?? [];
  const [working, setWorking] = useState(false);
  const [single, setSingle] = useState(false);
  /** The way to commit that the main button does. The agent is the usual one. */
  const [mode, setMode] = useState<"agent" | "one">("agent");
  const [text, setText] = useState("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (open) {
      setError(null);
      setSingle(false);
      setMode("agent");
      setText("");
    }
  }, [open]);

  const run = async (commit: () => Promise<CommitInfo[]>) => {
    if (!workspaceId) return;
    setWorking(true);
    setError(null);
    try {
      const made = await commit();
      toast(`Made ${made.length} ${made.length === 1 ? "commit" : "commits"}`);
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: prKeys.all(workspaceId) }),
        queryClient.invalidateQueries({ queryKey: ["changes", workspaceId] }),
        queryClient.invalidateQueries({ queryKey: fileKeys.comments(workspaceId) }),
      ]);
      const ui = useUi.getState();
      ui.setCommitOpen(false);
      ui.setPrOpen(true);
    } catch (e) {
      setError(message(e));
    } finally {
      setWorking(false);
    }
  };

  return (
    <Dialog
      open={open && !!workspaceId}
      onOpenChange={(next) => {
        // Leaving while commits are being made would hide the result.
        if (!working) setOpen(next);
      }}
    >
      <DialogContent className="sm:max-w-lg" showCloseButton={!working}>
        <DialogHeader>
          <DialogTitle>Commit your changes</DialogTitle>
          <DialogDescription>
            A pull request needs commits, and nothing is committed on this branch yet.
          </DialogDescription>
        </DialogHeader>

        {working ? (
          <div
            role="status"
            className="flex min-h-48 flex-col items-center justify-center gap-3 text-center"
          >
            <Loader2Icon className="size-7 animate-spin text-muted-foreground" />
            <div className="flex flex-col gap-1">
              <p className="text-sm font-medium">Making your commits…</p>
              <p className="text-xs text-muted-foreground">The pull request comes next.</p>
            </div>
          </div>
        ) : (
          <div className="flex flex-col gap-3">
            <p className="text-xs text-muted-foreground">
              {changes.length} {changes.length === 1 ? "file" : "files"} changed
            </p>
            <ul
              aria-label="Changed files"
              className="max-h-48 overflow-y-auto rounded-lg border border-border p-1 text-xs"
            >
              {changes.map((file) => (
                <li key={file.path} className="flex items-center gap-2 px-2 py-0.5">
                  <span className="min-w-0 flex-1 truncate font-mono" title={file.path}>
                    {file.path}
                  </span>
                  {file.added > 0 ? (
                    <span className="font-mono text-success">+{file.added}</span>
                  ) : null}
                  {file.removed > 0 ? (
                    <span className="font-mono text-destructive">−{file.removed}</span>
                  ) : null}
                </li>
              ))}
            </ul>
            {single ? (
              <Input
                aria-label="Commit message"
                placeholder="Commit message"
                value={text}
                onChange={(e) => setText(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && text.trim()) {
                    void run(() => ipc.commitEverything(workspaceId as string, text.trim()));
                  }
                }}
              />
            ) : null}
            {error ? (
              <p role="alert" className="text-xs text-destructive">
                {error}
              </p>
            ) : null}
          </div>
        )}

        {working ? null : (
          <DialogFooter>
            <Button variant="outline" onClick={() => setOpen(false)}>
              Cancel
            </Button>
            {single ? (
              <>
                <Button variant="ghost" onClick={() => setSingle(false)}>
                  Back
                </Button>
                <Button
                  disabled={!text.trim()}
                  onClick={() =>
                    void run(() => ipc.commitEverything(workspaceId as string, text.trim()))
                  }
                >
                  Commit everything
                </Button>
              </>
            ) : (
              <div className="flex">
                <Button
                  className="rounded-r-none"
                  disabled={mode === "agent" && changes.length === 0}
                  onClick={() =>
                    mode === "one"
                      ? setSingle(true)
                      : void run(() =>
                          ipc.commitWithClaude(
                            workspaceId as string,
                            useCreatePrefs.getState().model,
                          ),
                        )
                  }
                >
                  {mode === "agent" ? <SparklesIcon /> : null}
                  {mode === "agent" ? "Commit with Agent" : "One commit"}
                </Button>
                <DropdownMenu>
                  <DropdownMenuTrigger
                    render={
                      <Button
                        aria-label="Choose how to commit"
                        className="rounded-l-none border-l border-primary-foreground/25 px-1.5"
                      />
                    }
                  >
                    <ChevronDownIcon />
                  </DropdownMenuTrigger>
                  <DropdownMenuContent align="end" className="w-48">
                    <DropdownMenuRadioGroup
                      value={mode}
                      onValueChange={(v) => setMode(v === "one" ? "one" : "agent")}
                    >
                      <DropdownMenuRadioItem value="agent">Commit with Agent</DropdownMenuRadioItem>
                      <DropdownMenuRadioItem value="one">One commit</DropdownMenuRadioItem>
                    </DropdownMenuRadioGroup>
                  </DropdownMenuContent>
                </DropdownMenu>
              </div>
            )}
          </DialogFooter>
        )}
      </DialogContent>
    </Dialog>
  );
}
