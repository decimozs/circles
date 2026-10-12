import type { FsEntry } from "@circles/bindings/FsEntry";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@circles/ui/components/context-menu";
import { Input } from "@circles/ui/components/input";
import { cn } from "@circles/ui/utils";
import { useQuery } from "@tanstack/react-query";
import { ChevronRightIcon, EyeIcon, EyeOffIcon, SearchIcon } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { useActiveSession } from "@/features/chat/api";
import { useEditors, useFsList } from "@/features/files/api";
import { FileIconFor } from "@/features/files/file-icon";
import { ipc } from "@/lib/ipc";
import { useChat } from "@/stores/chat";
import { useReview } from "@/stores/review";

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** The menu of a file or folder: copy its path, show it, open it elsewhere, hand it to the chat. */
function EntryMenu({
  workspaceId,
  workspacePath,
  entry,
  children,
}: {
  workspaceId: string;
  workspacePath: string;
  entry: Pick<FsEntry, "path" | "is_dir">;
  children: React.ReactNode;
}) {
  const editors = useEditors().data ?? [];
  const { active } = useActiveSession(workspaceId);
  const copy = (text: string, what: string) =>
    navigator.clipboard
      ?.writeText(text)
      .then(() => toast(`${what} copied`))
      .catch(() => toast.error("Could not copy"));

  return (
    <ContextMenu>
      <ContextMenuTrigger render={<div />}>{children}</ContextMenuTrigger>
      <ContextMenuContent className="w-52">
        {entry.is_dir ? null : (
          <ContextMenuItem onClick={() => useReview.getState().openFile(workspaceId, entry.path)}>
            Open
          </ContextMenuItem>
        )}
        <ContextMenuItem onClick={() => copy(entry.path, "Path")}>Copy path</ContextMenuItem>
        <ContextMenuItem onClick={() => copy(`${workspacePath}/${entry.path}`, "Full path")}>
          Copy full path
        </ContextMenuItem>
        <ContextMenuItem
          onClick={() =>
            ipc.fsReveal(workspaceId, entry.path).catch((e) => toast.error(message(e)))
          }
        >
          Reveal in Finder
        </ContextMenuItem>
        {editors.map((editor) => (
          <ContextMenuItem
            key={editor.id}
            onClick={() =>
              ipc
                .editorOpen(workspaceId, editor.id, entry.path)
                .catch((e) => toast.error(message(e)))
            }
          >
            Open in {editor.name}
          </ContextMenuItem>
        ))}
        <ContextMenuSeparator />
        <ContextMenuItem
          disabled={!active}
          onClick={() => {
            if (!active) return;
            const chat = useChat.getState();
            const draft = chat.drafts[active.id] ?? "";
            chat.setDraft(
              active.id,
              `${draft}${draft && !draft.endsWith(" ") ? " " : ""}@${entry.path} `,
            );
            toast("Attached to the chat");
          }}
        >
          Attach to chat
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}

function Row({
  workspaceId,
  workspacePath,
  entry,
  depth,
  open,
  onActivate,
  label,
}: {
  workspaceId: string;
  workspacePath: string;
  entry: FsEntry;
  depth: number;
  open?: boolean;
  onActivate: () => void;
  label?: string;
}) {
  return (
    <EntryMenu workspaceId={workspaceId} workspacePath={workspacePath} entry={entry}>
      <button
        type="button"
        onClick={onActivate}
        aria-expanded={entry.is_dir ? open : undefined}
        style={{ paddingLeft: 8 + depth * 14 }}
        className={cn(
          "flex h-7 w-full items-center gap-1.5 rounded-md pr-2 text-left text-xs outline-none",
          "hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring/60",
          entry.ignored && "text-muted-foreground/70",
        )}
      >
        {entry.is_dir ? (
          <ChevronRightIcon
            className={cn(
              "size-3 shrink-0 text-muted-foreground transition-transform",
              open && "rotate-90",
            )}
          />
        ) : (
          <span className="size-3 shrink-0" />
        )}
        <FileIconFor
          name={entry.name}
          isDir={entry.is_dir}
          open={open}
          className="size-3.5 shrink-0 text-muted-foreground"
        />
        <span className="min-w-0 truncate">{label ?? entry.name}</span>
      </button>
    </EntryMenu>
  );
}

/** One folder's entries. Child folders load only when they are opened. */
function Level({
  workspaceId,
  workspacePath,
  path,
  depth,
  showIgnored,
}: {
  workspaceId: string;
  workspacePath: string;
  path: string;
  depth: number;
  showIgnored: boolean;
}) {
  const list = useFsList(workspaceId, path, showIgnored);
  const [openDirs, setOpenDirs] = useState<Record<string, boolean>>({});
  if (list.isLoading) {
    return <p className="px-3 py-1 text-xs text-muted-foreground">Loading…</p>;
  }
  if (list.error) {
    return <p className="px-3 py-1 text-xs text-destructive">{message(list.error)}</p>;
  }
  return (
    <>
      {list.data?.map((entry) => (
        <div key={entry.path}>
          <Row
            workspaceId={workspaceId}
            workspacePath={workspacePath}
            entry={entry}
            depth={depth}
            open={!!openDirs[entry.path]}
            onActivate={() =>
              entry.is_dir
                ? setOpenDirs((o) => ({ ...o, [entry.path]: !o[entry.path] }))
                : useReview.getState().openFile(workspaceId, entry.path)
            }
          />
          {entry.is_dir && openDirs[entry.path] ? (
            <Level
              workspaceId={workspaceId}
              workspacePath={workspacePath}
              path={entry.path}
              depth={depth + 1}
              showIgnored={showIgnored}
            />
          ) : null}
        </div>
      ))}
    </>
  );
}

export function FileTree({
  workspaceId,
  workspacePath,
}: {
  workspaceId: string;
  workspacePath: string;
}) {
  const showIgnored = useReview((s) => s.showIgnored);
  const setShowIgnored = useReview((s) => s.setShowIgnored);
  const [query, setQuery] = useState("");
  const term = query.trim();
  const found = useQuery({
    queryKey: ["fs", workspaceId, "search", term],
    queryFn: () => ipc.searchFiles(workspaceId, term, 60),
    enabled: term.length > 0,
    placeholderData: (previous) => previous,
  });

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center gap-1 px-2 py-1.5">
        <div className="relative flex-1">
          <SearchIcon className="pointer-events-none absolute top-1/2 left-2 size-3 -translate-y-1/2 text-muted-foreground" />
          <Input
            aria-label="Search files"
            placeholder="Search files"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            className="h-7 pl-7 text-xs"
          />
        </div>
        <button
          type="button"
          aria-label={showIgnored ? "Hide ignored files" : "Show ignored files"}
          aria-pressed={showIgnored}
          onClick={() => setShowIgnored(!showIgnored)}
          className="flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground"
        >
          {showIgnored ? <EyeIcon className="size-3.5" /> : <EyeOffIcon className="size-3.5" />}
        </button>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-1 pb-2" role="tree" aria-label="Files">
        {term ? (
          found.data?.length === 0 ? (
            <p className="px-3 py-2 text-xs text-muted-foreground">No files match.</p>
          ) : (
            found.data?.map((hit) => {
              const entry: FsEntry = {
                name: hit.path.split("/").pop() ?? hit.path,
                path: hit.path,
                is_dir: hit.is_dir,
                size: 0,
                ignored: false,
              };
              return (
                <Row
                  key={hit.path}
                  workspaceId={workspaceId}
                  workspacePath={workspacePath}
                  entry={entry}
                  depth={0}
                  label={hit.path}
                  onActivate={() => {
                    if (!hit.is_dir) useReview.getState().openFile(workspaceId, hit.path);
                  }}
                />
              );
            })
          )
        ) : (
          <Level
            workspaceId={workspaceId}
            workspacePath={workspacePath}
            path=""
            depth={0}
            showIgnored={showIgnored}
          />
        )}
      </div>
    </div>
  );
}
