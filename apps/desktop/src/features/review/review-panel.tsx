import { Button } from "@circles/ui/components/button";
import { useQueryClient } from "@tanstack/react-query";
import { DownloadIcon, PlayIcon, SendIcon } from "lucide-react";
import { useCallback, useEffect, useRef } from "react";
import { toast } from "sonner";
import { chatKeys, useSessions } from "@/features/chat/api";
import { fileKeys, useComments } from "@/features/files/api";
import { ipc } from "@/lib/ipc";
import { useChat } from "@/stores/chat";
import { CommentItem, useSendToChat } from "./comment-dock";
import { asComment, parseFindings, reviewPrompt } from "./findings";

const REVIEW_TITLE = "Review";
const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** Comments on the workspace, and an agent review that adds its findings to them. */
export function ReviewPanel({
  workspaceId,
  baseBranch,
}: {
  workspaceId: string;
  baseBranch: string;
}) {
  const queryClient = useQueryClient();
  const comments = useComments(workspaceId).data ?? [];
  const sessions = useSessions(workspaceId).data ?? [];
  const toChat = useSendToChat(workspaceId);
  const open = comments.filter((c) => c.status === "open");
  const resolved = comments.filter((c) => c.status === "resolved");
  const review = [...sessions].reverse().find((s) => s.title === REVIEW_TITLE);

  const importFindings = useCallback(
    async (sessionId: string, quiet = false) => {
      try {
        const messages = await ipc.sessionMessages(sessionId);
        const text = messages
          .filter((m) => m.role === "assistant" && m.kind === "text")
          .map((m) => String((m.content as { text?: string }).text ?? ""))
          .join("\n");
        const known = new Set(
          (await ipc.commentList(workspaceId)).map((c) => `${c.path}:${c.line_start}:${c.body}`),
        );
        let added = 0;
        for (const finding of parseFindings(text)) {
          if (known.has(`${finding.path}:${finding.line}:${finding.body}`)) continue;
          await ipc.commentCreate(workspaceId, asComment(finding)).then(
            () => {
              added += 1;
            },
            () => {
              // A finding about a path that is not valid is dropped.
            },
          );
        }
        await queryClient.invalidateQueries({ queryKey: fileKeys.comments(workspaceId) });
        if (!quiet || added > 0) {
          toast(
            added > 0
              ? `Added ${added} review ${added === 1 ? "finding" : "findings"}`
              : "No new findings",
          );
        }
      } catch (e) {
        toast.error(message(e));
      }
    },
    [workspaceId, queryClient],
  );

  // Findings are added when the review chat finishes its turn.
  const lastStatus = useRef<string | undefined>(undefined);
  useEffect(() => {
    if (review && lastStatus.current === "working" && review.status === "idle") {
      void importFindings(review.id, true);
    }
    lastStatus.current = review?.status;
  }, [review, importFindings]);

  const run = async () => {
    try {
      const session = await ipc.sessionCreate(workspaceId, {
        title: REVIEW_TITLE,
        permission_mode: "plan",
      });
      await queryClient.invalidateQueries({ queryKey: chatKeys.sessions(workspaceId) });
      useChat.getState().selectTab(workspaceId, session.id);
      await ipc.sessionSend(session.id, reviewPrompt(baseBranch));
      toast("Review started in a new chat");
    } catch (e) {
      toast.error(message(e));
    }
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-3">
      <div className="flex flex-col gap-2">
        <Button size="sm" onClick={run}>
          <PlayIcon />
          Run review
        </Button>
        <p className="text-xs text-muted-foreground">
          Starts a chat that reads your changes against {baseBranch}. Its findings show up here as
          comments.
        </p>
        {review ? (
          <Button size="sm" variant="outline" onClick={() => void importFindings(review.id)}>
            <DownloadIcon />
            Import findings
          </Button>
        ) : null}
      </div>

      <section aria-label="Open comments" className="flex flex-col gap-2">
        <div className="flex items-center justify-between">
          <h3 className="text-xs font-medium">Open ({open.length})</h3>
          {open.length > 0 ? (
            <Button
              size="sm"
              variant="outline"
              disabled={!toChat.canSend || toChat.pending}
              onClick={() => toChat.send(open)}
            >
              <SendIcon />
              Send all to chat
            </Button>
          ) : null}
        </div>
        {open.length === 0 ? (
          <p className="text-xs text-muted-foreground">
            Click a line in the diff or in a file to leave a comment.
          </p>
        ) : (
          <ul className="flex flex-col gap-1.5">
            {open.map((c) => (
              <CommentItem key={c.id} workspaceId={workspaceId} comment={c} showPath />
            ))}
          </ul>
        )}
      </section>

      {resolved.length > 0 ? (
        <section aria-label="Resolved comments" className="flex flex-col gap-2">
          <h3 className="text-xs font-medium text-muted-foreground">
            Resolved ({resolved.length})
          </h3>
          <ul className="flex flex-col gap-1.5">
            {resolved.map((c) => (
              <CommentItem key={c.id} workspaceId={workspaceId} comment={c} showPath />
            ))}
          </ul>
        </section>
      ) : null}
    </div>
  );
}
