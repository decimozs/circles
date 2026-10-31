import type { PrDetail } from "@circles/bindings/PrDetail";
import { Button } from "@circles/ui/components/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@circles/ui/components/dialog";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { toast } from "sonner";
import { describeError } from "@/features/chat/use-session";
import { ipc } from "@/lib/ipc";
import { prKeys } from "./api";

export type MergeMethodId = "merge" | "squash" | "rebase";

const METHODS: { id: MergeMethodId; label: string; hint: string }[] = [
  { id: "merge", label: "Merge commit", hint: "Keeps every commit and adds a merge commit." },
  { id: "squash", label: "Squash and merge", hint: "Combines the commits into one." },
  {
    id: "rebase",
    label: "Rebase and merge",
    hint: "Replays the commits on the base, with no merge commit.",
  },
];

/** What stands in the way of merging, or deserves a second look. `blocked` cannot be overridden
 * from here; the rest need a tick to show they were read. */
export function mergeConcerns(detail: PrDetail): { blocked: string | null; warnings: string[] } {
  const { row } = detail;
  const blocked =
    row.state !== "open"
      ? "This pull request is not open."
      : row.is_draft
        ? "This is a draft. Mark it ready for review on GitHub first."
        : detail.mergeable === "conflicting"
          ? "This branch has conflicts with its base. Resolve them first."
          : null;
  const warnings: string[] = [];
  if (row.checks.state === "failing") warnings.push("Some checks are failing.");
  if (row.checks.state === "pending") warnings.push("Some checks have not finished.");
  if (row.review === "changes_requested") warnings.push("A reviewer asked for changes.");
  if (row.review === "review_required") warnings.push("It has not been approved yet.");
  if (detail.merge_state === "blocked")
    warnings.push("GitHub says the branch rules block this merge, so it may be refused.");
  if (detail.merge_state === "behind") warnings.push("The branch is behind its base.");
  return { blocked, warnings };
}

/** Asks before merging, because it cannot be undone from here. Merging is always the person's
 * own choice: nothing in Circles merges a pull request by itself. */
export function MergeDialog({
  detail,
  projectId,
  open,
  onOpenChange,
}: {
  detail: PrDetail;
  projectId: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const queryClient = useQueryClient();
  const [method, setMethod] = useState<MergeMethodId>("merge");
  const [understood, setUnderstood] = useState(false);
  const { row } = detail;
  const { blocked, warnings } = mergeConcerns(detail);
  const merge = useMutation({
    mutationFn: () => ipc.prBrowseMerge(projectId, row.number, method),
    onSuccess: () => {
      toast.success(`Merged #${row.number}`);
      onOpenChange(false);
      queryClient.invalidateQueries({ queryKey: prKeys.detail(projectId, row.number) });
      queryClient.invalidateQueries({ queryKey: ["pr-browse"] });
    },
    onError: (e) => toast.error(describeError(e)),
  });
  const ready = !blocked && (warnings.length === 0 || understood) && !merge.isPending;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>
            Merge #{row.number} into {row.base_ref}?
          </DialogTitle>
          <DialogDescription>{row.title}</DialogDescription>
        </DialogHeader>
        {blocked ? (
          <p role="alert" className="text-sm text-destructive">
            {blocked}
          </p>
        ) : (
          <>
            <fieldset className="flex flex-col gap-2">
              <legend className="sr-only">How to merge</legend>
              {METHODS.map((m) => (
                <label key={m.id} className="flex cursor-pointer items-start gap-2 text-sm">
                  <input
                    type="radio"
                    name="merge-method"
                    className="mt-1"
                    checked={method === m.id}
                    onChange={() => setMethod(m.id)}
                  />
                  <span>
                    {m.label}
                    <span className="block text-xs text-muted-foreground">{m.hint}</span>
                  </span>
                </label>
              ))}
            </fieldset>
            {warnings.length > 0 ? (
              <div
                role="alert"
                className="rounded-md border border-warning/40 bg-warning/5 p-3 text-sm"
              >
                <ul className="list-disc pl-4">
                  {warnings.map((w) => (
                    <li key={w}>{w}</li>
                  ))}
                </ul>
                <label className="mt-2 flex items-center gap-2 text-xs">
                  <input
                    type="checkbox"
                    checked={understood}
                    onChange={(e) => setUnderstood(e.target.checked)}
                  />
                  Merge anyway
                </label>
              </div>
            ) : null}
          </>
        )}
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button disabled={!ready} onClick={() => merge.mutate()}>
            {merge.isPending ? "Merging…" : "Merge"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
