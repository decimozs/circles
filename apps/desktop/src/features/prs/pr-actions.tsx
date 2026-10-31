import type { PrDetail } from "@circles/bindings/PrDetail";
import { Button } from "@circles/ui/components/button";
import { GitBranchIcon, GitMergeIcon } from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router";
import { toast } from "sonner";
import { CopyButton } from "@/features/chat/markdown";
import { describeError } from "@/features/chat/use-session";
import { useCreateWorkspace } from "@/features/workspaces/api";
import { MergeDialog } from "./merge-dialog";

/** Merge, and a workspace on the pull request's branch. */
export function PrActions({ detail, projectId }: { detail: PrDetail; projectId: string }) {
  const navigate = useNavigate();
  const [mergeOpen, setMergeOpen] = useState(false);
  const create = useCreateWorkspace();
  const { row } = detail;

  const checkOut = () =>
    create.mutate(
      {
        projectId,
        options: {
          source: { kind: "pull_request", number: row.number },
          prompt: null,
          model: null,
          effort: null,
          attachments: [],
        },
      },
      {
        onSuccess: (workspace) => navigate(`/workspace/${workspace.id}`),
        onError: (e) => toast.error(describeError(e)),
      },
    );

  return (
    <div className="flex items-center gap-1.5">
      <code className="hidden rounded bg-muted px-1.5 py-0.5 font-mono text-xs text-muted-foreground lg:inline">
        gh pr checkout {row.number}
      </code>
      <CopyButton text={`gh pr checkout ${row.number}`} label="Copy the checkout command" />
      <Button variant="outline" size="xs" disabled={create.isPending} onClick={checkOut}>
        <GitBranchIcon />
        {create.isPending ? "Checking out…" : "Check out"}
      </Button>
      {row.state === "open" ? (
        <Button size="xs" onClick={() => setMergeOpen(true)}>
          <GitMergeIcon />
          Merge
        </Button>
      ) : null}
      <MergeDialog
        detail={detail}
        projectId={projectId}
        open={mergeOpen}
        onOpenChange={setMergeOpen}
      />
    </div>
  );
}
