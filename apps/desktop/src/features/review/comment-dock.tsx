import type { Comment } from "@circles/bindings/Comment";
import { Button } from "@circles/ui/components/button";
import { CheckIcon, SendIcon, Trash2Icon, Undo2Icon } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { useActiveSession } from "@/features/chat/api";
import {
  useComments,
  useCreateComment,
  useDeleteComment,
  useResolveComment,
  useSendComments,
} from "@/features/files/api";

export type DraftRange = { start: number; end: number };

export function lineLabel(comment: Pick<Comment, "line_start" | "line_end">): string {
  return comment.line_start === comment.line_end
    ? `line ${comment.line_start}`
    : `lines ${comment.line_start}–${comment.line_end}`;
}

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** Sends open comments to the workspace's chat, then reports the result. */
export function useSendToChat(workspaceId: string) {
  const { active } = useActiveSession(workspaceId);
  const send = useSendComments(workspaceId);
  return {
    canSend: !!active,
    pending: send.isPending,
    send: (comments: Comment[]) => {
      if (!active || comments.length === 0) return;
      send.mutate(
        { sessionId: active.id, ids: comments.map((c) => c.id) },
        {
          onSuccess: () =>
            toast(
              `Sent ${comments.length} ${comments.length === 1 ? "comment" : "comments"} to the chat`,
            ),
          onError: (e) => toast.error(message(e)),
        },
      );
    },
  };
}

/** One comment with the actions that belong to it. */
export function CommentItem({
  workspaceId,
  comment,
  showPath,
}: {
  workspaceId: string;
  comment: Comment;
  showPath?: boolean;
}) {
  const resolve = useResolveComment(workspaceId);
  const remove = useDeleteComment(workspaceId);
  const resolved = comment.status === "resolved";
  return (
    <li className="flex flex-col gap-1 rounded-lg border border-border p-2 text-xs">
      <div className="flex items-center gap-1.5 text-muted-foreground">
        <span className="min-w-0 truncate font-mono">
          {showPath ? `${comment.path}, ` : ""}
          {lineLabel(comment)}
        </span>
        {comment.source === "agent" ? (
          <span className="rounded bg-muted px-1 py-px">review</span>
        ) : null}
        {comment.sent_at ? <span>sent</span> : null}
        <div className="ml-auto flex">
          <button
            type="button"
            aria-label={resolved ? "Reopen comment" : "Resolve comment"}
            onClick={() => resolve.mutate({ id: comment.id, resolved: !resolved })}
            className="flex size-6 items-center justify-center rounded-md hover:bg-muted hover:text-foreground"
          >
            {resolved ? <Undo2Icon className="size-3.5" /> : <CheckIcon className="size-3.5" />}
          </button>
          <button
            type="button"
            aria-label="Delete comment"
            onClick={() => remove.mutate(comment.id)}
            className="flex size-6 items-center justify-center rounded-md hover:bg-muted hover:text-destructive"
          >
            <Trash2Icon className="size-3.5" />
          </button>
        </div>
      </div>
      <p
        className={
          resolved
            ? "whitespace-pre-wrap text-muted-foreground line-through"
            : "whitespace-pre-wrap"
        }
      >
        {comment.body}
      </p>
    </li>
  );
}

/**
 * The comments of one file, under its editor, and the box for a new one. Open comments can be
 * sent to the chat as instructions for the agent.
 */
export function CommentDock({
  workspaceId,
  path,
  draft,
  onCloseDraft,
}: {
  workspaceId: string;
  path: string;
  draft: DraftRange | null;
  onCloseDraft: () => void;
}) {
  const comments = (useComments(workspaceId).data ?? []).filter((c) => c.path === path);
  const create = useCreateComment(workspaceId);
  const toChat = useSendToChat(workspaceId);
  const [text, setText] = useState("");
  const open = comments.filter((c) => c.status === "open");

  if (!draft && comments.length === 0) return null;

  const submit = () => {
    if (!draft || !text.trim()) return;
    create.mutate(
      { path, line_start: draft.start, line_end: draft.end, body: text.trim(), source: "user" },
      {
        onSuccess: () => {
          setText("");
          onCloseDraft();
        },
        onError: (e) => toast.error(message(e)),
      },
    );
  };

  return (
    <section
      aria-label="Comments"
      className="flex max-h-64 shrink-0 flex-col gap-2 overflow-y-auto border-t border-border p-3"
    >
      {draft ? (
        <form
          className="flex flex-col gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            submit();
          }}
        >
          <label htmlFor="new-comment" className="text-xs text-muted-foreground">
            Comment on {lineLabel({ line_start: draft.start, line_end: draft.end })}
          </label>
          <textarea
            id="new-comment"
            // biome-ignore lint/a11y/noAutofocus: the box opens because the user clicked a line
            autoFocus
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) submit();
              if (e.key === "Escape") onCloseDraft();
            }}
            rows={2}
            placeholder="What should change here?"
            className="w-full resize-none rounded-md border border-input bg-transparent px-2 py-1.5 text-xs outline-none focus-visible:border-ring"
          />
          <div className="flex justify-end gap-2">
            <Button type="button" size="sm" variant="ghost" onClick={onCloseDraft}>
              Cancel
            </Button>
            <Button type="submit" size="sm" disabled={!text.trim() || create.isPending}>
              Add comment
            </Button>
          </div>
        </form>
      ) : null}
      {comments.length > 0 ? (
        <>
          <ul className="flex flex-col gap-1.5">
            {comments.map((comment) => (
              <CommentItem key={comment.id} workspaceId={workspaceId} comment={comment} />
            ))}
          </ul>
          {open.length > 0 ? (
            <Button
              size="sm"
              variant="outline"
              className="self-start"
              disabled={!toChat.canSend || toChat.pending}
              onClick={() => toChat.send(open)}
            >
              <SendIcon />
              Send {open.length} to chat
            </Button>
          ) : null}
        </>
      ) : null}
    </section>
  );
}
