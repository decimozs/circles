import type { Session } from "@circles/bindings/Session";
import { Popover, PopoverContent, PopoverTrigger } from "@circles/ui/components/popover";
import { cn } from "@circles/ui/utils";
import { HistoryIcon, Loader2Icon, PlusIcon, XIcon } from "lucide-react";
import { useState } from "react";
import { useChat } from "@/stores/chat";
import {
  useActiveSession,
  useCloseSession,
  useCreateChat,
  useRenameSession,
  useReopenSession,
  useReorderSessions,
  useSessionHistory,
} from "./api";

export function tabTitle(session: Session): string {
  return session.title ?? "New chat";
}

function Tab({
  session,
  active,
  onSelect,
  onClose,
  onRename,
  dragProps,
}: {
  session: Session;
  active: boolean;
  onSelect: () => void;
  onClose: () => void;
  onRename: (title: string) => void;
  dragProps: Record<string, unknown>;
}) {
  const [editing, setEditing] = useState(false);
  return (
    <div
      role="tab"
      tabIndex={0}
      aria-selected={active}
      draggable={!editing}
      onClick={onSelect}
      onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && !editing && onSelect()}
      onDoubleClick={() => setEditing(true)}
      {...dragProps}
      className={cn(
        "group relative flex h-10 max-w-44 shrink-0 cursor-default items-center gap-1.5 px-3 text-sm outline-none",
        "focus-visible:bg-muted/60",
        active ? "text-foreground" : "text-muted-foreground hover:text-foreground",
      )}
    >
      {session.status === "working" ? (
        <Loader2Icon aria-label="Working" className="size-3 shrink-0 animate-spin" />
      ) : session.status === "blocked" ? (
        <span
          role="img"
          aria-label="Needs approval"
          className="size-2 shrink-0 rounded-full bg-warning"
        />
      ) : null}
      {active ? (
        <span
          aria-hidden
          className="absolute inset-x-2 bottom-0 h-0.5 rounded-full bg-foreground"
        />
      ) : null}
      {editing ? (
        <input
          // biome-ignore lint/a11y/noAutofocus: focus moves to the field the user just opened
          autoFocus
          aria-label="Chat name"
          defaultValue={tabTitle(session)}
          maxLength={60}
          className="w-28 bg-transparent outline-none"
          onClick={(e) => e.stopPropagation()}
          onBlur={(e) => {
            const title = e.target.value.trim();
            if (title && title !== tabTitle(session)) onRename(title);
            setEditing(false);
          }}
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === "Enter") e.currentTarget.blur();
            if (e.key === "Escape") setEditing(false);
          }}
        />
      ) : (
        <span className="truncate">{tabTitle(session)}</span>
      )}
      <button
        type="button"
        aria-label={`Close ${tabTitle(session)}`}
        className="shrink-0 rounded-sm opacity-0 hover:bg-background group-hover:opacity-100 focus-visible:opacity-100"
        onClick={(e) => {
          e.stopPropagation();
          onClose();
        }}
      >
        <XIcon className="size-3" />
      </button>
    </div>
  );
}

/** The chat tabs and the new chat button. They live in the top bar of a workspace. */
export function ChatTabs({
  workspaceId,
  single,
}: {
  workspaceId: string;
  /**
   * A Composer has one chat. There is no button for another one, and no tab until a second one
   * exists (a schedule or trigger started it).
   */
  single?: boolean;
}) {
  const { sessions, active } = useActiveSession(workspaceId);
  const select = useChat((s) => s.selectTab);
  const create = useCreateChat(workspaceId);
  const rename = useRenameSession(workspaceId);
  const close = useCloseSession(workspaceId);
  const reorder = useReorderSessions(workspaceId);
  const [dragging, setDragging] = useState<string | null>(null);
  const activeId = active?.id;
  const onSelect = (id: string) => select(workspaceId, id);

  const drop = (targetId: string) => {
    if (!dragging || dragging === targetId) return;
    const ids = sessions.map((s) => s.id).filter((id) => id !== dragging);
    ids.splice(ids.indexOf(targetId), 0, dragging);
    reorder.mutate(ids);
    setDragging(null);
  };

  if (single && sessions.length <= 1) return null;
  return (
    // As tall as the top bar, so the plus sits on its centre line with or without any tab.
    <div className="flex h-10 min-w-0 items-center">
      <div
        role="tablist"
        aria-label="Chats"
        className="flex min-w-0 items-center overflow-x-auto [scrollbar-width:none]"
      >
        {sessions.map((session) => (
          <Tab
            key={session.id}
            session={session}
            active={session.id === activeId}
            onSelect={() => onSelect(session.id)}
            onClose={() => {
              // Closing the open tab moves to its neighbour.
              if (session.id === activeId) {
                const index = sessions.findIndex((s) => s.id === session.id);
                const next = sessions[index + 1] ?? sessions[index - 1];
                if (next) onSelect(next.id);
              }
              close.mutate(session.id);
            }}
            onRename={(title) => rename.mutate({ id: session.id, title })}
            dragProps={{
              onDragStart: () => setDragging(session.id),
              onDragOver: (e: React.DragEvent) => dragging && e.preventDefault(),
              onDrop: () => drop(session.id),
              onDragEnd: () => setDragging(null),
            }}
          />
        ))}
      </div>
      {single ? null : (
        <button
          type="button"
          aria-label="New chat"
          onClick={() => void create()}
          className="ml-1 flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground"
        >
          <PlusIcon className="size-4" />
        </button>
      )}
    </div>
  );
}

/** Closed chats of a workspace, to bring one back. */
export function ChatHistory({ workspaceId }: { workspaceId: string }) {
  const select = useChat((s) => s.selectTab);
  const reopen = useReopenSession(workspaceId);
  const [open, setOpen] = useState(false);
  const history = useSessionHistory(workspaceId, open);
  const closed = (history.data ?? []).filter((s) => s.status === "closed");
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger
        aria-label="Chat history"
        className="flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground"
      >
        <HistoryIcon className="size-4" />
      </PopoverTrigger>
      <PopoverContent align="end" className="w-64">
        <p className="px-1 text-xs font-medium text-muted-foreground">Closed chats</p>
        {closed.length === 0 ? (
          <p className="px-1 text-xs text-muted-foreground">Nothing closed yet.</p>
        ) : (
          <ul className="flex max-h-60 flex-col overflow-auto">
            {closed.map((session) => (
              <li key={session.id}>
                <button
                  type="button"
                  className="w-full truncate rounded-md px-1.5 py-1 text-left text-xs hover:bg-muted"
                  onClick={() => {
                    reopen.mutate(session.id, {
                      onSuccess: () => select(workspaceId, session.id),
                    });
                    setOpen(false);
                  }}
                >
                  {tabTitle(session)}
                </button>
              </li>
            ))}
          </ul>
        )}
      </PopoverContent>
    </Popover>
  );
}
