import type { ChangedFile } from "@circles/bindings/ChangedFile";
import type { ChangeStatus } from "@circles/bindings/ChangeStatus";
import { cn } from "@circles/ui/utils";
import {
  FileDiffIcon,
  FilePlusIcon,
  FileXIcon,
  FolderTreeIcon,
  ListIcon,
  MoveRightIcon,
} from "lucide-react";
import { useMemo } from "react";
import { useChanges } from "@/features/files/api";
import { useReview } from "@/stores/review";

const STATUS: Record<ChangeStatus, { label: string; Icon: typeof FileDiffIcon; tone: string }> = {
  added: { label: "Added", Icon: FilePlusIcon, tone: "text-success" },
  modified: { label: "Modified", Icon: FileDiffIcon, tone: "text-warning" },
  deleted: { label: "Deleted", Icon: FileXIcon, tone: "text-destructive" },
  renamed: { label: "Renamed", Icon: MoveRightIcon, tone: "text-info" },
};

function folderOf(path: string): string {
  const index = path.lastIndexOf("/");
  return index < 0 ? "" : path.slice(0, index);
}

export function ChangeRow({ workspaceId, file }: { workspaceId: string; file: ChangedFile }) {
  const view = useReview((s) => s.views[workspaceId]);
  const { label, Icon, tone } = STATUS[file.status];
  const name = file.path.split("/").pop() ?? file.path;
  const open = view?.kind === "diff" && view.path === file.path;
  return (
    <button
      type="button"
      onClick={() => useReview.getState().openDiff(workspaceId, file.path, file.old_path)}
      aria-current={open ? "true" : undefined}
      title={file.path}
      className={cn(
        "flex h-7 w-full items-center gap-1.5 rounded-md px-2 text-left text-xs outline-none",
        "hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring/60",
        open && "bg-muted",
      )}
    >
      <Icon aria-label={label} className={cn("size-3.5 shrink-0", tone)} />
      <span className="min-w-0 flex-1 truncate">{name}</span>
      {file.binary ? (
        <span className="shrink-0 text-muted-foreground">binary</span>
      ) : (
        <span className="flex shrink-0 gap-1.5 font-mono">
          {file.added > 0 ? <span className="text-success">+{file.added}</span> : null}
          {file.removed > 0 ? <span className="text-destructive">−{file.removed}</span> : null}
        </span>
      )}
    </button>
  );
}

export function ChangesPanel({ workspaceId }: { workspaceId: string }) {
  const scope = useReview((s) => s.scope);
  const setScope = useReview((s) => s.setScope);
  const grouped = useReview((s) => s.groupByFolder);
  const setGrouped = useReview((s) => s.setGroupByFolder);
  const changes = useChanges(workspaceId, scope);
  const files = changes.data ?? [];

  const groups = useMemo(() => {
    const map = new Map<string, ChangedFile[]>();
    for (const file of files) {
      const folder = folderOf(file.path);
      map.set(folder, [...(map.get(folder) ?? []), file]);
    }
    return [...map.entries()];
  }, [files]);

  const counts = {
    all: useChanges(workspaceId, "all").data?.length,
    uncommitted: useChanges(workspaceId, "uncommitted").data?.length,
  };
  const added = files.reduce((n, f) => n + f.added, 0);
  const removed = files.reduce((n, f) => n + f.removed, 0);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center gap-1 px-2 py-1.5">
        <fieldset aria-label="Which changes" className="flex rounded-md bg-muted p-0.5">
          {(
            [
              ["all", "All changes"],
              ["uncommitted", "Uncommitted"],
            ] as const
          ).map(([value, label]) => (
            <button
              key={value}
              type="button"
              aria-pressed={scope === value}
              onClick={() => setScope(value)}
              className={cn(
                "rounded px-2 py-0.5 text-xs",
                scope === value
                  ? "bg-background text-foreground shadow-sm"
                  : "text-muted-foreground",
              )}
            >
              {label}
              {counts[value] !== undefined ? (
                <>
                  {" "}
                  <span className="text-muted-foreground">{counts[value]}</span>
                </>
              ) : null}
            </button>
          ))}
        </fieldset>
        <button
          type="button"
          aria-label={grouped ? "Show a flat list" : "Group by folder"}
          aria-pressed={grouped}
          onClick={() => setGrouped(!grouped)}
          className="ml-auto flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground"
        >
          {grouped ? <ListIcon className="size-3.5" /> : <FolderTreeIcon className="size-3.5" />}
        </button>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-1 pb-2">
        {changes.isLoading ? (
          <p className="px-3 py-2 text-xs text-muted-foreground">Loading…</p>
        ) : changes.error ? (
          <p role="alert" className="px-3 py-2 text-xs text-destructive">
            {changes.error.message}
          </p>
        ) : files.length === 0 ? (
          <p className="px-3 py-6 text-center text-xs text-muted-foreground">
            {scope === "all"
              ? "No changes on this branch yet."
              : (counts.all ?? 0) > 0
                ? `Nothing uncommitted. The ${counts.all} changed ${counts.all === 1 ? "file is" : "files are"} all committed.`
                : "Nothing uncommitted."}
          </p>
        ) : grouped ? (
          groups.map(([folder, items]) => (
            <section key={folder} aria-label={folder || "Top level"}>
              <h3 className="truncate px-2 pt-2 pb-0.5 font-mono text-[11px] text-muted-foreground">
                {folder || "/"}
              </h3>
              {items.map((file) => (
                <ChangeRow key={file.path} workspaceId={workspaceId} file={file} />
              ))}
            </section>
          ))
        ) : (
          files.map((file) => <ChangeRow key={file.path} workspaceId={workspaceId} file={file} />)
        )}
      </div>

      {files.length > 0 ? (
        <p className="flex items-center justify-between border-t border-border px-3 py-1.5 text-xs text-muted-foreground">
          <span>
            {files.length} {files.length === 1 ? "file" : "files"}
          </span>
          <span className="flex gap-1.5">
            <span className="font-mono text-success">+{added}</span>
            <span className="font-mono text-destructive">−{removed}</span>
          </span>
        </p>
      ) : null}
    </div>
  );
}
