import { Button } from "@circles/ui/components/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@circles/ui/components/dialog";
import { Input } from "@circles/ui/components/input";
import { useQueryClient } from "@tanstack/react-query";
import { Loader2Icon, SparklesIcon } from "lucide-react";
import { useEffect, useState } from "react";
import { useMatch } from "react-router";
import { toast } from "sonner";
import { Markdown } from "@/features/chat/markdown";
import { prKeys, useReadiness } from "@/features/pr/api";
import { ipc } from "@/lib/ipc";
import { useCreatePrefs } from "@/stores/create-prefs";
import { useUi } from "@/stores/ui";

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** Title and description of a pull request, written by hand or drafted by an agent. */
export function PrDialog() {
  const workspaceId = useMatch("/workspace/:id")?.params.id;
  const open = useUi((s) => s.prOpen);
  const setOpen = useUi((s) => s.setPrOpen);
  const queryClient = useQueryClient();

  const git = useReadiness(workspaceId, open).data?.git;
  const nothingCommitted = git !== undefined && git.ahead === 0;
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [asDraft, setAsDraft] = useState(false);
  const [view, setView] = useState<"write" | "preview">("write");
  const [drafting, setDrafting] = useState(false);
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Each time the dialog opens it starts from the commits of the branch.
  useEffect(() => {
    if (!open || !workspaceId) return;
    setError(null);
    setDrafting(false);
    setView("write");
    ipc
      .prDraftSeed(workspaceId)
      .then((seed) => {
        setTitle(seed.title);
        setBody(seed.body);
      })
      .catch((e) => setError(message(e)));
  }, [open, workspaceId]);

  const draftWithClaude = async () => {
    if (!workspaceId) return;
    setError(null);
    setDrafting(true);
    try {
      const draft = await ipc.prDraft(workspaceId, useCreatePrefs.getState().model);
      setTitle(draft.title);
      setBody(draft.body);
    } catch (e) {
      setError(message(e));
    } finally {
      setDrafting(false);
    }
  };

  const create = async () => {
    if (!workspaceId || !title.trim()) return;
    setCreating(true);
    setError(null);
    try {
      const pr = await ipc.prCreate(workspaceId, title.trim(), body.trim(), asDraft);
      toast(`Created pull request #${pr.number}`);
      await queryClient.invalidateQueries({ queryKey: prKeys.all(workspaceId) });
      await queryClient.invalidateQueries({ queryKey: ["workspaces"] });
      setOpen(false);
    } catch (e) {
      setError(message(e));
    } finally {
      setCreating(false);
    }
  };

  return (
    <Dialog open={open && !!workspaceId} onOpenChange={setOpen}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>Create pull request</DialogTitle>
          <DialogDescription>
            The branch is pushed to origin first, then the pull request is opened with the GitHub
            CLI.
          </DialogDescription>
        </DialogHeader>
        <div className="flex flex-col gap-3">
          <Input
            aria-label="Title"
            placeholder="Title"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            disabled={creating}
          />
          <div className="flex flex-col gap-1.5">
            <div className="flex items-center gap-1">
              <fieldset aria-label="Description view" className="flex rounded-md bg-muted p-0.5">
                {(["write", "preview"] as const).map((mode) => (
                  <button
                    key={mode}
                    type="button"
                    aria-pressed={view === mode}
                    onClick={() => setView(mode)}
                    className={
                      view === mode
                        ? "rounded bg-background px-2 py-0.5 text-xs text-foreground shadow-sm"
                        : "rounded px-2 py-0.5 text-xs text-muted-foreground"
                    }
                  >
                    {mode === "write" ? "Write" : "Preview"}
                  </button>
                ))}
              </fieldset>
              <span className="text-xs text-muted-foreground">Markdown</span>
            </div>
            {view === "write" ? (
              <textarea
                aria-label="Description"
                placeholder="Description"
                value={body}
                rows={10}
                onChange={(e) => setBody(e.target.value)}
                disabled={creating}
                className="w-full resize-y rounded-lg border border-input bg-transparent px-2.5 py-2 font-mono text-xs outline-none focus-visible:border-ring"
              />
            ) : (
              <section
                aria-label="Description preview"
                className="max-h-64 min-h-[15rem] overflow-y-auto rounded-lg border border-input px-3 py-2"
              >
                {body.trim() ? (
                  <Markdown text={body} />
                ) : (
                  <p className="text-sm text-muted-foreground">Nothing to preview yet.</p>
                )}
              </section>
            )}
          </div>
          <div className="flex items-center justify-between gap-2">
            <label className="flex items-center gap-2 text-xs">
              <input
                type="checkbox"
                checked={asDraft}
                onChange={(e) => setAsDraft(e.target.checked)}
                className="size-3.5 accent-foreground"
              />
              Open as a draft
            </label>
            <Button
              size="sm"
              variant="outline"
              disabled={drafting || creating}
              onClick={() => void draftWithClaude()}
            >
              {drafting ? <Loader2Icon className="animate-spin" /> : <SparklesIcon />}
              {drafting ? "Agent is drafting…" : "Draft with Agent"}
            </Button>
          </div>
          {error ? (
            <p role="alert" className="text-xs text-destructive">
              {error}
            </p>
          ) : null}
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => setOpen(false)}>
            Cancel
          </Button>
          <Button
            disabled={!title.trim() || creating || drafting || nothingCommitted}
            onClick={() => void create()}
          >
            {creating ? <Loader2Icon className="animate-spin" /> : null}
            Create pull request
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
