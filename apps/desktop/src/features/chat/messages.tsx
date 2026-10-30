import type { Attachment } from "@circles/bindings/Attachment";
import type { PermissionDecision } from "@circles/bindings/PermissionDecision";
import type { PermissionRequest } from "@circles/bindings/PermissionRequest";
import { Button } from "@circles/ui/components/button";
import { cn } from "@circles/ui/utils";
import { FileIcon, GitBranchIcon, ShieldQuestionIcon, Undo2Icon } from "lucide-react";
import { CopyButton, Markdown } from "./markdown";
import type { TimelineItem } from "./timeline";

export function formatDuration(ms: number): string {
  const seconds = Math.round(ms / 1000);
  if (seconds < 1) return "<1s";
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  return `${minutes}m ${seconds % 60}s`;
}

export function AttachmentChip({ attachment }: { attachment: Pick<Attachment, "name"> }) {
  return (
    <span className="inline-flex max-w-48 items-center gap-1 rounded-md bg-background/60 px-1.5 py-0.5 text-xs">
      <FileIcon className="size-3 shrink-0" />
      <span className="truncate">{attachment.name}</span>
    </span>
  );
}

export function UserBubble({
  item,
  onRevert,
}: {
  item: Extract<TimelineItem, { type: "user" }>;
  /** Set when this message can be gone back to. */
  onRevert?: () => void;
}) {
  const content = item.message.content as {
    text?: string;
    steered?: boolean;
    attachments?: Attachment[];
  };
  return (
    <div className="group/user flex flex-col items-end gap-1">
      <div className="flex max-w-[85%] items-start gap-1.5">
        {onRevert ? (
          <button
            type="button"
            aria-label="Revert to before this turn"
            title="Revert to before this turn"
            onClick={onRevert}
            className="mt-1.5 shrink-0 rounded-md p-1 text-muted-foreground opacity-0 transition-opacity hover:bg-muted hover:text-foreground focus-visible:opacity-100 group-hover/user:opacity-100"
          >
            <Undo2Icon className="size-3.5" />
          </button>
        ) : null}
        <div className="rounded-xl bg-muted px-3 py-2 text-sm">
          <p className="whitespace-pre-wrap break-words">{content.text}</p>
          {content.attachments?.length ? (
            <div className="mt-1.5 flex flex-wrap gap-1">
              {content.attachments.map((a) => (
                <AttachmentChip key={a.path} attachment={a} />
              ))}
            </div>
          ) : null}
        </div>
      </div>
      {content.steered ? (
        <span className="text-[11px] text-muted-foreground">Sent while the agent was working</span>
      ) : null}
    </div>
  );
}

export function SystemNote({ text, tone }: { text: string; tone: "info" | "error" }) {
  return (
    <p
      role={tone === "error" ? "alert" : "status"}
      className={cn(
        "px-1.5 text-left text-xs",
        tone === "error" ? "text-destructive" : "text-muted-foreground",
      )}
    >
      {text}
    </p>
  );
}

export function TurnSummary({
  item,
  onBranch,
}: {
  item: Extract<TimelineItem, { type: "summary" }>;
  /** Set when a new chat can be started from the end of this turn. */
  onBranch?: () => void;
}) {
  const shown = item.changed.slice(0, 6);
  return (
    <div className="flex flex-wrap items-center gap-1.5 text-[11px] text-muted-foreground">
      <span>{formatDuration(item.durationMs)}</span>
      {item.text ? <CopyButton text={item.text} label="Copy reply" className="size-5" /> : null}
      {onBranch ? (
        <button
          type="button"
          aria-label="Branch the conversation from here"
          title="Branch the conversation from here"
          onClick={onBranch}
          className="inline-flex size-5 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
        >
          <GitBranchIcon className="size-3.5" />
        </button>
      ) : null}
      {shown.map((file) => (
        <span
          key={file.path}
          title={file.path}
          className="inline-flex max-w-56 items-center gap-1.5 rounded-md bg-muted px-1.5 py-0.5 font-mono"
        >
          <span className="truncate">{file.path.split("/").pop()}</span>
          {file.added > 0 ? <span className="text-success">+{file.added}</span> : null}
          {file.removed > 0 ? <span className="text-destructive">−{file.removed}</span> : null}
        </span>
      ))}
      {item.changed.length > shown.length ? (
        <span>+{item.changed.length - shown.length} more</span>
      ) : null}
    </div>
  );
}

export function AssistantText({ text }: { text: string }) {
  return <Markdown text={text} />;
}

export function PermissionCard({
  request,
  onReply,
  root,
}: {
  request: PermissionRequest;
  onReply: (decision: PermissionDecision) => void;
  root?: string;
}) {
  const input = request.input as Record<string, unknown> | null;
  const command = typeof input?.command === "string" ? input.command : null;
  const filePath = typeof input?.file_path === "string" ? input.file_path : null;
  const name = request.display_name ?? request.tool_name;
  return (
    <fieldset
      aria-label={`Permission for ${name}`}
      className="flex flex-col gap-2 rounded-lg border border-warning/60 bg-warning/5 p-3 text-sm"
    >
      <div className="flex items-center gap-2 font-medium">
        <ShieldQuestionIcon className="size-4 text-warning" />
        <span>The agent wants to use {name}</span>
      </div>
      {request.description ? (
        <p className="text-xs text-muted-foreground">{request.description}</p>
      ) : null}
      {command ? (
        <pre className="overflow-auto whitespace-pre-wrap rounded-md bg-muted/60 p-2 font-mono text-xs">
          <span className="text-muted-foreground">$ </span>
          {command}
        </pre>
      ) : (
        <pre className="max-h-40 overflow-auto whitespace-pre-wrap rounded-md bg-muted/60 p-2 font-mono text-xs">
          {filePath && root && filePath.startsWith(`${root}/`)
            ? `${filePath.slice(root.length + 1)}\n\n`
            : ""}
          {JSON.stringify(request.input, null, 2)}
        </pre>
      )}
      {request.blocked_path ? (
        <p className="text-xs text-warning">Outside the workspace: {request.blocked_path}</p>
      ) : null}
      <div className="flex flex-wrap gap-2">
        <Button size="sm" onClick={() => onReply({ decision: "allow" })}>
          Allow
        </Button>
        <Button size="sm" variant="outline" onClick={() => onReply({ decision: "allow_session" })}>
          Allow for session
        </Button>
        <Button
          size="sm"
          variant="outline"
          onClick={() => onReply({ decision: "deny", message: null })}
        >
          Deny
        </Button>
      </div>
    </fieldset>
  );
}
