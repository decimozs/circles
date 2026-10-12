import { ChevronDownIcon, ChevronUpIcon, Columns2Icon, Rows2Icon, XIcon } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { IconButton } from "@/components/shell/icon-button";
import { useChanges, useComments, useFilePair } from "@/features/files/api";
import { CommentDock, type DraftRange } from "@/features/review/comment-dock";
import { useReview } from "@/stores/review";
import { type CommentMark, DiffEditorView } from "./monaco-editors";

/** A changed file in Monaco's diff editor, with arrows to the neighbouring changed files. */
export function DiffView({
  workspaceId,
  path,
  oldPath,
}: {
  workspaceId: string;
  path: string;
  oldPath: string | null;
}) {
  const scope = useReview((s) => s.scope);
  const split = useReview((s) => s.split);
  const setSplit = useReview((s) => s.setSplit);
  const files = useChanges(workspaceId, scope).data ?? [];
  const pair = useFilePair(workspaceId, scope, path, oldPath);
  const comments = useComments(workspaceId).data ?? [];
  const [draft, setDraft] = useState<DraftRange | null>(null);

  // A new file starts without a half written comment.
  // biome-ignore lint/correctness/useExhaustiveDependencies: reset when the file changes
  useEffect(() => setDraft(null), [path]);

  const index = files.findIndex((f) => f.path === path);
  const go = (offset: number) => {
    const next = files[index + offset];
    if (next) useReview.getState().openDiff(workspaceId, next.path, next.old_path);
  };

  const marks = useMemo<CommentMark[]>(
    () =>
      comments
        .filter((c) => c.path === path)
        .map((c) => ({
          line_start: c.line_start,
          line_end: c.line_end,
          open: c.status === "open",
        })),
    [comments, path],
  );

  const data = pair.data;
  return (
    <section aria-label="Diff" className="flex h-full min-h-0 flex-col">
      <header className="flex h-10 shrink-0 items-center gap-1 border-b border-border px-3">
        <p className="min-w-0 flex-1 truncate font-mono text-xs" title={path}>
          {path}
        </p>
        {files.length > 0 && index >= 0 ? (
          <span className="shrink-0 text-xs text-muted-foreground">
            {index + 1} of {files.length}
          </span>
        ) : null}
        <IconButton label="Previous file" disabled={index <= 0} onClick={() => go(-1)}>
          <ChevronUpIcon />
        </IconButton>
        <IconButton
          label="Next file"
          disabled={index < 0 || index >= files.length - 1}
          onClick={() => go(1)}
        >
          <ChevronDownIcon />
        </IconButton>
        <IconButton
          label={split ? "Show one column" : "Show side by side"}
          onClick={() => setSplit(!split)}
        >
          {split ? <Rows2Icon /> : <Columns2Icon />}
        </IconButton>
        <IconButton
          label="Close diff"
          hint="Esc"
          onClick={() => useReview.getState().closeView(workspaceId)}
        >
          <XIcon />
        </IconButton>
      </header>

      <div className="min-h-0 flex-1">
        {pair.isLoading ? (
          <p className="p-4 text-xs text-muted-foreground">Loading…</p>
        ) : pair.error ? (
          <p role="alert" className="p-4 text-xs text-destructive">
            {pair.error.message}
          </p>
        ) : data?.binary ? (
          <p role="status" className="p-4 text-sm text-muted-foreground">
            This is a binary file, so there is no text to compare.
          </p>
        ) : data?.too_large ? (
          <p role="status" className="p-4 text-sm text-muted-foreground">
            This file is too large to compare here. Open it in your editor instead.
          </p>
        ) : data ? (
          <DiffEditorView
            path={path}
            original={data.original ?? ""}
            modified={data.modified ?? ""}
            language={data.language}
            sideBySide={split}
            marks={marks}
            onComment={(start, end) => setDraft({ start, end })}
          />
        ) : null}
      </div>

      <CommentDock
        workspaceId={workspaceId}
        path={path}
        draft={draft}
        onCloseDraft={() => setDraft(null)}
      />
    </section>
  );
}
