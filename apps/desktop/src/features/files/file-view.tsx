import { Button } from "@circles/ui/components/button";
import { useQueryClient } from "@tanstack/react-query";
import { PencilIcon, SaveIcon, XIcon } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import { IconButton } from "@/components/shell/icon-button";
import { CodeView, type CommentMark } from "@/features/diff/monaco-editors";
import { fileKeys, useComments, useFileContent } from "@/features/files/api";
import { CommentDock, type DraftRange } from "@/features/review/comment-dock";
import { ipc } from "@/lib/ipc";
import { useReview } from "@/stores/review";

function sizeLabel(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

/** One file of the workspace: read only by default, with an Edit switch. */
export function FileView({ workspaceId, path }: { workspaceId: string; path: string }) {
  const queryClient = useQueryClient();
  const file = useFileContent(workspaceId, path);
  const comments = useComments(workspaceId).data ?? [];
  const [editing, setEditing] = useState(false);
  const [text, setText] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [draft, setDraft] = useState<DraftRange | null>(null);

  // A different file starts clean.
  // biome-ignore lint/correctness/useExhaustiveDependencies: reset when the file changes
  useEffect(() => {
    setEditing(false);
    setText(null);
    setDraft(null);
  }, [path]);

  const data = file.data;
  const dirty = editing && text !== null && text !== data?.text;
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

  const save = async () => {
    if (text === null) return;
    setSaving(true);
    try {
      await ipc.fsWrite(workspaceId, path, text);
      await queryClient.invalidateQueries({ queryKey: fileKeys.file(workspaceId, path) });
      await queryClient.invalidateQueries({ queryKey: ["changes", workspaceId] });
      toast("Saved");
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <section aria-label="File" className="flex h-full min-h-0 flex-col">
      <header className="flex h-10 shrink-0 items-center gap-1 border-b border-border px-3">
        <p className="min-w-0 flex-1 truncate font-mono text-xs" title={path}>
          {path}
          {dirty ? <span className="ml-1 text-muted-foreground">(unsaved)</span> : null}
        </p>
        {data?.kind === "text" ? (
          <>
            <Button
              size="sm"
              variant={editing ? "secondary" : "ghost"}
              aria-pressed={editing}
              onClick={() => {
                if (editing) setText(null);
                setEditing((v) => !v);
              }}
            >
              <PencilIcon />
              Edit
            </Button>
            {dirty ? (
              <Button size="sm" disabled={saving} onClick={save}>
                <SaveIcon />
                Save
              </Button>
            ) : null}
          </>
        ) : null}
        <IconButton
          label="Close file"
          hint="Esc"
          onClick={() => useReview.getState().closeView(workspaceId)}
        >
          <XIcon />
        </IconButton>
      </header>

      <div className="min-h-0 flex-1">
        {file.isLoading ? (
          <p className="p-4 text-xs text-muted-foreground">Loading…</p>
        ) : file.error ? (
          <p role="alert" className="p-4 text-xs text-destructive">
            {file.error.message}
          </p>
        ) : data?.kind === "image" ? (
          <div className="flex h-full items-center justify-center overflow-auto p-4">
            <img
              alt={path}
              src={`data:${data.mime};base64,${data.data_base64}`}
              className="max-h-full max-w-full object-contain"
            />
          </div>
        ) : data?.kind === "binary" ? (
          <p role="status" className="p-4 text-sm text-muted-foreground">
            This is a binary file ({sizeLabel(data.size)}), so it cannot be shown as text.
          </p>
        ) : data?.kind === "too_large" ? (
          <p role="status" className="p-4 text-sm text-muted-foreground">
            This file is too large to open here ({sizeLabel(data.size)}). Open it in your editor
            instead.
          </p>
        ) : data ? (
          <CodeView
            path={path}
            value={editing && text !== null ? text : (data.text ?? "")}
            language={data.language}
            readOnly={!editing}
            marks={marks}
            onChange={(value) => editing && setText(value)}
            onSave={() => void save()}
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
