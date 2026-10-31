import type { Attachment } from "@circles/bindings/Attachment";
import type { Composer as ComposerDef } from "@circles/bindings/Composer";
import type { PermissionDecision } from "@circles/bindings/PermissionDecision";
import type { PermissionRequest } from "@circles/bindings/PermissionRequest";
import type { Session } from "@circles/bindings/Session";
import type { Workspace } from "@circles/bindings/Workspace";
import { Button } from "@circles/ui/components/button";
import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";
import {
  useApprovals,
  useFirstPrompt,
  useSetMode,
  useWorkspaceLinks,
} from "@/features/composers/api";
import { ApprovalCard } from "@/features/composers/approval-card";
import { CardsPanel } from "@/features/genui/cards-panel";
import { useWorkspaceEvents } from "@/features/workspaces/api";
import { ipc } from "@/lib/ipc";
import { useChat } from "@/stores/chat";
import { useSide } from "@/stores/side";
import {
  chatKeys,
  useActiveSession,
  useCheckpoints,
  useCreateChat,
  useLastRevert,
  useRevertedSync,
} from "./api";
import { Composer } from "./composer";
import { MessageList, type TurnActions } from "./message-list";
import { RevertBanner, RevertDialog } from "./revert-dialog";
import { SidePane } from "./side-pane";
import { buildTimeline } from "./timeline";
import { describeError, useSession } from "./use-session";

const EVENT_STYLE: Record<string, string> = {
  created: "bg-muted text-foreground",
  info: "text-muted-foreground",
  warning: "text-warning",
  hint: "text-muted-foreground",
  error: "text-destructive",
};

const SUGGESTIONS = [
  "Explain how this codebase is organised",
  "Find and fix a bug",
  "Write tests for the riskiest code",
];

/** A name for a chat taken from its first message. */
export function titleFrom(text: string): string {
  const line = text.trim().split("\n")[0] ?? "";
  return line.length > 40 ? `${line.slice(0, 39)}…` : line;
}

let queuedId = 0;

/** What a Composer run is waiting on a person for, above the input. */
function RunApprovals({ sessionId }: { sessionId: string }) {
  const pending = useApprovals({ sessionId, onlyPending: true }).data ?? [];
  if (pending.length === 0) return null;
  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-2 px-6 pb-2">
      {pending.map((a) => (
        <ApprovalCard key={a.id} approval={a} />
      ))}
    </div>
  );
}

function ChatPane({
  workspace,
  session,
  composer,
}: {
  workspace: Workspace;
  session: Session;
  composer?: ComposerDef;
}) {
  const queryClient = useQueryClient();
  const events = useWorkspaceEvents(workspace.id);
  const view = useSession(session.id);
  const live = view.session ?? session;
  const working = live.status === "working" || live.status === "blocked";
  const queue = useChat((s) => s.queues[session.id]) ?? [];
  const usable = !["creating", "archived", "errored"].includes(workspace.state);

  const timeline = useMemo(() => buildTimeline(view.messages, working), [view.messages, working]);

  // Going back to before a turn, and branching a new chat from the end of one.
  const checkpoints = useCheckpoints(session.id).data;
  const lastRevert = useLastRevert(session.id).data;
  const [revertTurn, setRevertTurn] = useState<number | null>(null);
  const turns = useMemo<TurnActions | undefined>(() => {
    if (!checkpoints || checkpoints.length === 0) return undefined;
    const revertable = new Map(
      checkpoints.flatMap((c) =>
        c.message_seq === null ? [] : [[c.turn, { messageSeq: c.message_seq }] as const],
      ),
    );
    return {
      revertable,
      onRevert: (turn) => setRevertTurn(turn),
      onBranch: (turn) => {
        ipc
          .sessionBranch(session.id, turn)
          .then((created) => {
            useChat.getState().selectTab(workspace.id, created.id);
            toast("Started a new chat from here");
            return queryClient.invalidateQueries({ queryKey: chatKeys.sessions(workspace.id) });
          })
          .catch((e) => toast.error(describeError(e)));
      },
    };
  }, [checkpoints, session.id, workspace.id, queryClient]);

  const refreshLists = useCallback(
    () => queryClient.invalidateQueries({ queryKey: ["sessions", workspace.id] }),
    [queryClient, workspace.id],
  );

  const deliver = useCallback(
    async (text: string, attachments: Attachment[]) => {
      try {
        await ipc.sessionSend(session.id, text, attachments);
        if (!live.title && text) {
          // First its first words at once, then a model's title in their place when it is ready.
          await ipc.sessionAutoTitle(session.id, titleFrom(text), text);
          void refreshLists();
        }
      } catch (e) {
        toast.error(describeError(e));
        view.setError(describeError(e));
      }
    },
    [session.id, live.title, refreshLists, view.setError],
  );

  const send = (text: string, attachments: Attachment[], how: "send" | "queue") => {
    if (how === "queue" && working) {
      useChat.getState().enqueue(session.id, { id: `q${++queuedId}`, text, attachments });
      return;
    }
    void deliver(text, attachments);
  };

  // A queued message goes out when the turn it waited for is over.
  const draining = useRef(false);
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs when the status or the queue changes
  useEffect(() => {
    if (live.status !== "idle" || queue.length === 0 || draining.current) return;
    const next = useChat.getState().shiftQueue(session.id);
    if (!next) return;
    draining.current = true;
    deliver(next.text, next.attachments).finally(() => {
      draining.current = false;
    });
  }, [live.status, queue.length]);

  const lastStatus = useRef(live.status);
  const handled = useRef<string | null>(null);
  // What a model thinks might be said next, from the last exchange. Only when it is switched on.
  const [suggestion, setSuggestion] = useState<string | null>(null);
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs when a turn ends
  useEffect(() => {
    const ended = lastStatus.current === "working" && live.status === "idle";
    lastStatus.current = live.status;
    if (!ended) return;
    const reply = [...view.messages]
      .reverse()
      .find((m) => m.role === "assistant" && m.kind === "text");
    if (!reply || reply.id === handled.current) return;
    handled.current = reply.id;
    const asked = [...view.messages].reverse().find((m) => m.role === "user" && m.kind === "text");
    if (asked) {
      ipc
        .smartSuggest(
          String((asked.content as { text?: string }).text ?? ""),
          String((reply.content as { text?: string }).text ?? ""),
        )
        .then(setSuggestion)
        .catch(() => {});
    }
  }, [live.status]);

  // Looking at the chat clears its unread marker.
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs again when the status changes
  useEffect(() => {
    useChat.getState().markRead(workspace.id);
  }, [workspace.id, live.status]);

  const reply = useCallback(
    (request: PermissionRequest, decision: PermissionDecision) => {
      ipc.permissionReply(session.id, request.request_id, decision).catch((e) => {
        toast.error(describeError(e));
      });
    },
    [session.id],
  );

  const setComposerMode = useSetMode();
  const changeOptions = (options: { model?: string; effort?: string; permissionMode?: string }) => {
    ipc
      .sessionUpdate(session.id, options)
      .then(view.setSession)
      .catch((e) => toast.error(describeError(e)));
  };

  const empty = view.loaded && timeline.length === 0;
  // A Composer's first message box shows an example of what to ask it.
  const firstPrompt = useFirstPrompt(composer);
  const header = composer ? null : (
    <>
      <ul className="flex flex-col gap-1.5 text-sm">
        {events.data?.map((event) => (
          <li
            key={event.id}
            className={`w-fit max-w-full rounded-md px-3 py-1.5 ${EVENT_STYLE[event.kind] ?? ""}`}
          >
            {event.message}
          </li>
        ))}
      </ul>
      {empty ? (
        <div className="flex flex-wrap gap-2 pt-2">
          {SUGGESTIONS.map((suggestion) => (
            <button
              key={suggestion}
              type="button"
              className="rounded-full border border-border px-3 py-1 text-xs text-muted-foreground hover:bg-muted hover:text-foreground"
              onClick={() => useChat.getState().setDraft(session.id, suggestion)}
            >
              {suggestion}
            </button>
          ))}
        </div>
      ) : null}
    </>
  );

  // The message box floats over the end of the conversation, so messages scroll under it. The pane
  // learns how tall everything under the messages is, and the list leaves that much room.
  const pane = useRef<HTMLDivElement>(null);
  const dock = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const el = dock.current;
    const root = pane.current;
    if (!el || !root) return;
    const measure = () => root.style.setProperty("--dock-height", `${el.offsetHeight}px`);
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const watcher = new ResizeObserver(measure);
    watcher.observe(el);
    return () => watcher.disconnect();
  }, []);

  return (
    <div ref={pane} className="relative flex min-h-0 flex-1 flex-col">
      <MessageList
        items={timeline}
        live={view.live}
        pending={view.pending}
        working={live.status === "working"}
        root={workspace.path}
        onReply={reply}
        header={header}
        turns={turns}
      />
      <RevertDialog
        checkpoint={checkpoints?.find((c) => c.turn === revertTurn) ?? null}
        sessionId={session.id}
        onClose={() => setRevertTurn(null)}
      />
      <div
        ref={dock}
        data-testid="chat-dock"
        className="pointer-events-none absolute inset-x-0 bottom-0 z-10 flex flex-col [&>*]:pointer-events-auto"
      >
        {lastRevert && !working ? (
          <RevertBanner sessionId={session.id} revertId={lastRevert.id} turn={lastRevert.turn} />
        ) : null}
        {view.error ? (
          <div
            role="alert"
            className="mx-auto mb-2 flex w-full max-w-3xl items-center justify-between gap-3 px-6 text-xs text-destructive"
          >
            <span>{view.error}</span>
            <button type="button" className="underline" onClick={() => view.setError(null)}>
              Dismiss
            </button>
          </div>
        ) : null}
        {composer ? <RunApprovals sessionId={session.id} /> : null}
        <CardsPanel sessionId={session.id} />
        <SidePane sessionId={session.id} />
        <Composer
          variant={composer ? "composer" : "workspace"}
          modeControl={
            composer
              ? {
                  value: composer.mode,
                  onChange: (mode) =>
                    setComposerMode.mutate(
                      { id: composer.id, mode },
                      { onError: (e) => toast.error(describeError(e)) },
                    ),
                }
              : undefined
          }
          workspaceId={workspace.id}
          onSide={(question) => {
            useSide.getState().start(session.id, question);
            ipc.sessionSideQuestion(session.id, question).catch((e) => {
              useSide.getState().event(session.id, "failed", describeError(e), true);
            });
          }}
          suggestion={working ? null : (suggestion ?? (composer && empty ? firstPrompt : null))}
          session={live}
          working={working}
          disabled={!usable}
          queue={queue}
          onSend={send}
          onStop={() =>
            ipc.sessionInterrupt(session.id).catch((e) => toast.error(describeError(e)))
          }
          onChangeOptions={changeOptions}
        />
      </div>
    </div>
  );
}

/** The chat of a workspace: messages and the composer. The tabs are in the top bar. */
export function ChatPanel({
  workspace,
  composer,
  empty,
}: {
  workspace: Workspace;
  /** Set when the chats are the runs of a Composer. */
  composer?: ComposerDef;
  /** Shown when a Composer has no runs yet. */
  empty?: React.ReactNode;
}) {
  const queryClient = useQueryClient();
  const { query, sessions, active } = useActiveSession(workspace.id);
  const create = useCreateChat(workspace.id);
  const composerMade = useWorkspaceLinks().get(workspace.id);
  const usable = !["creating", "archived", "errored"].includes(workspace.state);
  // A chat that was reverted starts again from what is stored, so its view is made anew.
  const [reverted, setReverted] = useState<Record<string, number>>({});
  useRevertedSync(
    useCallback(
      (sessionId: string) => setReverted((r) => ({ ...r, [sessionId]: (r[sessionId] ?? 0) + 1 })),
      [],
    ),
  );

  // First visit: start a chat, with the prompt written in the Create dialog if there was one.
  const started = useRef<string | null>(null);
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs once per workspace, after the chats load
  useEffect(() => {
    if (
      composer ||
      !query.isSuccess ||
      sessions.length > 0 ||
      !usable ||
      started.current === workspace.id
    ) {
      return;
    }
    started.current = workspace.id;
    (async () => {
      // A workspace a Composer made gets its chat from the Composer. An empty one made here would
      // only sit next to it.
      const links = await ipc.composerWorkspaceLinks().catch(() => []);
      if (links.some((l) => l.workspace_id === workspace.id)) return;
      const pending = await ipc.workspaceTakePendingPrompt(workspace.id).catch(() => null);
      const session = await create(pending ? { model: pending.model, effort: pending.effort } : {});
      if (session && pending) {
        await ipc
          .sessionAutoTitle(session.id, titleFrom(pending.prompt), pending.prompt)
          .catch(() => {});
        await ipc
          .sessionSend(session.id, pending.prompt)
          .catch((e) => toast.error(describeError(e)));
        queryClient.invalidateQueries({ queryKey: chatKeys.sessions(workspace.id) });
      }
    })();
  }, [query.isSuccess, sessions.length, usable, workspace.id]);

  return (
    <section className="flex h-full min-h-0 flex-col" aria-label="Chat">
      {active ? (
        <ChatPane
          key={`${active.id}:${reverted[active.id] ?? 0}`}
          workspace={workspace}
          session={active}
          composer={composer}
        />
      ) : composer && query.isSuccess ? (
        (empty ?? <div className="flex-1" />)
      ) : composerMade && query.isSuccess && usable ? (
        // The Composer made this workspace and has not started an agent in it, because it was
        // stopped, hit its limit or failed. Without this the pane would be blank, with no way in.
        <div
          role="status"
          className="flex flex-1 flex-col items-center justify-center gap-3 px-6 text-center"
        >
          <p className="text-sm font-medium">No agent has started here yet</p>
          <p className="max-w-sm text-xs text-muted-foreground">
            {composerMade.composer_name} made this workspace but has not started an agent in it. It
            may still be working, or it may have stopped before it got to it. You can start a chat
            yourself.
          </p>
          <Button size="sm" onClick={() => void create()}>
            Start a chat
          </Button>
        </div>
      ) : (
        <div className="flex-1" />
      )}
    </section>
  );
}
