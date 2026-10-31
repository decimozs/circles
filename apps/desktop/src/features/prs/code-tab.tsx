import type { PrFile } from "@circles/bindings/PrFile";
import type { PrFiles } from "@circles/bindings/PrFiles";
import { cn } from "@circles/ui/utils";
import {
  ChevronDownIcon,
  ChevronRightIcon,
  Columns2Icon,
  Loader2Icon,
  Rows2Icon,
} from "lucide-react";
import { useState } from "react";
import { DiffEditorView } from "@/features/diff/monaco-editors";
import { usePrFile, usePrFiles } from "./api";
import type { PrTab } from "./store";
import { usePrs } from "./store";

/** Editors open at once. Each one is heavy, so a long list does not freeze the page. */
const MOST_OPEN = 2;

const STATUS: Record<string, [string, string, string]> = {
  added: ["A", "text-success", "Added"],
  removed: ["D", "text-destructive", "Deleted"],
  renamed: ["R", "text-info", "Renamed"],
  copied: ["C", "text-info", "Copied"],
};

function lines(text: string | null): number {
  return text ? text.split("\n").length : 0;
}

/** One file's before and after, in a diff editor sized to its text. */
function FileDiff({
  tab,
  files,
  file,
  split,
}: {
  tab: PrTab;
  files: PrFiles;
  file: PrFile;
  split: boolean;
}) {
  const pair = usePrFile(
    {
      project_id: tab.projectId,
      repo: tab.repo,
      base_sha: files.base_sha,
      head_sha: files.head_sha,
      path: file.path,
      previous_path: file.previous_path,
      status: file.status,
    },
    true,
  );
  if (pair.isPending) {
    return (
      <p role="status" className="flex items-center gap-2 p-4 text-xs text-muted-foreground">
        <Loader2Icon className="size-3.5 animate-spin" />
        Loading {file.path}…
      </p>
    );
  }
  if (pair.isError) {
    return (
      <p role="alert" className="p-4 text-xs text-destructive">
        {String((pair.error as { message?: string })?.message ?? pair.error)}
      </p>
    );
  }
  const data = pair.data;
  if (data.binary) {
    return (
      <p role="status" className="p-4 text-sm text-muted-foreground">
        This is a binary file, so there is no text to compare.
      </p>
    );
  }
  if (data.too_large) {
    return (
      <p role="status" className="p-4 text-sm text-muted-foreground">
        This file is too large to compare here. Open the pull request on GitHub to see it.
      </p>
    );
  }
  const height = Math.min(
    640,
    Math.max(140, Math.max(lines(data.original), lines(data.modified)) * 19 + 24),
  );
  return (
    <div style={{ height }}>
      <DiffEditorView
        path={`${tab.key}/${file.path}`}
        original={data.original ?? ""}
        modified={data.modified ?? ""}
        language={data.language}
        sideBySide={split}
      />
    </div>
  );
}

/** The files a pull request changes. Open one to see it in a diff editor. */
export function CodeTab({ tab, headSha }: { tab: PrTab; headSha: string }) {
  const query = usePrFiles(tab.projectId, tab.repo, tab.number, headSha);
  const viewedByPr = usePrs((s) => s.viewed);
  const setViewed = usePrs((s) => s.setViewed);
  const [open, setOpen] = useState<string[]>([]);
  const [split, setSplit] = useState(true);

  if (query.isPending) {
    return (
      <p role="status" className="flex items-center gap-2 px-5 py-6 text-sm text-muted-foreground">
        <Loader2Icon className="size-4 animate-spin" />
        Loading the files…
      </p>
    );
  }
  if (query.isError) {
    return (
      <p role="alert" className="px-5 py-6 text-sm text-destructive">
        {String((query.error as { message?: string })?.message ?? query.error)}
      </p>
    );
  }
  const files = query.data;
  const prKey = `${tab.key}@${files.head_sha}`;
  const viewed = viewedByPr[prKey] ?? [];
  const toggle = (path: string) =>
    setOpen((o) =>
      o.includes(path) ? o.filter((p) => p !== path) : [...o.slice(-(MOST_OPEN - 1)), path],
    );

  return (
    <div className="flex flex-col gap-2 px-4 py-3">
      <div className="flex items-center gap-3 text-xs text-muted-foreground">
        <span>
          {files.files.length} {files.files.length === 1 ? "file" : "files"} · {viewed.length} /{" "}
          {files.files.length} viewed
        </span>
        <button type="button" className="ml-auto hover:text-foreground" onClick={() => setOpen([])}>
          Collapse all
        </button>
        <button
          type="button"
          aria-label={split ? "Show one column" : "Show side by side"}
          className="hover:text-foreground"
          onClick={() => setSplit((s) => !s)}
        >
          {split ? <Rows2Icon className="size-4" /> : <Columns2Icon className="size-4" />}
        </button>
      </div>
      {files.files.length === 0 ? (
        <p className="py-4 text-sm text-muted-foreground">No files changed.</p>
      ) : null}
      <ul className="flex flex-col gap-1.5">
        {files.files.map((file) => {
          const expanded = open.includes(file.path);
          const done = viewed.includes(file.path);
          const [letter, tone, word] = STATUS[file.status] ?? ["M", "text-warning", "Modified"];
          return (
            <li key={file.path} className="overflow-hidden rounded-lg border border-border">
              <div
                className={cn(
                  "flex items-center gap-2 px-2 py-1.5",
                  expanded && "border-b border-border bg-muted/40",
                )}
              >
                <button
                  type="button"
                  aria-expanded={expanded}
                  aria-label={`${expanded ? "Collapse" : "Expand"} ${file.path}`}
                  onClick={() => toggle(file.path)}
                  className="flex min-w-0 flex-1 items-center gap-2 text-left text-sm outline-none"
                >
                  {expanded ? (
                    <ChevronDownIcon className="size-3.5 shrink-0" />
                  ) : (
                    <ChevronRightIcon className="size-3.5 shrink-0" />
                  )}
                  <span
                    title={word}
                    className={cn("w-4 shrink-0 text-center font-mono text-xs", tone)}
                  >
                    <span aria-hidden>{letter}</span>
                    <span className="sr-only">{word}</span>
                  </span>
                  <span
                    className={cn(
                      "min-w-0 truncate font-mono text-xs",
                      done && "text-muted-foreground",
                    )}
                  >
                    {file.previous_path ? `${file.previous_path} → ` : ""}
                    {file.path}
                  </span>
                </button>
                <span className="shrink-0 text-xs tabular-nums">
                  <span className="text-success">+{file.additions}</span>{" "}
                  <span className="text-destructive">-{file.deletions}</span>
                </span>
                <label className="flex shrink-0 items-center gap-1 text-xs text-muted-foreground">
                  <input
                    type="checkbox"
                    checked={done}
                    onChange={(e) => {
                      setViewed(prKey, file.path, e.target.checked);
                      // A file that is done with is put away.
                      if (e.target.checked) setOpen((o) => o.filter((p) => p !== file.path));
                    }}
                  />
                  Viewed
                </label>
              </div>
              {expanded ? (
                !file.has_patch &&
                file.additions + file.deletions === 0 &&
                file.status !== "renamed" &&
                file.status !== "added" ? (
                  <p className="p-4 text-sm text-muted-foreground">
                    GitHub gives no changes for this file. It may be binary or too large.
                  </p>
                ) : (
                  <FileDiff tab={tab} files={files} file={file} split={split} />
                )
              ) : null}
            </li>
          );
        })}
      </ul>
    </div>
  );
}
