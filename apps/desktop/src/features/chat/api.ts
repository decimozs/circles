import type { SessionChange } from "@circles/bindings/SessionChange";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect } from "react";
import { toast } from "sonner";
import { ipc } from "@/lib/ipc";
import { useChat } from "@/stores/chat";
import { useCreatePrefs } from "@/stores/create-prefs";
import { useSide } from "@/stores/side";

export const checkpointKeys = {
  list: (sessionId: string) => ["checkpoints", sessionId, "list"] as const,
  lastRevert: (sessionId: string) => ["checkpoints", sessionId, "last-revert"] as const,
};

export const chatKeys = {
  sessions: (workspaceId: string) => ["sessions", workspaceId] as const,
  history: (workspaceId: string) => ["sessions", workspaceId, "history"] as const,
  commands: (workspaceId: string) => ["slash-commands", workspaceId] as const,
};

/** Open chat tabs of a workspace, in their saved order. */
export function useSessions(workspaceId: string | undefined) {
  return useQuery({
    queryKey: chatKeys.sessions(workspaceId ?? ""),
    queryFn: () => ipc.sessionList(workspaceId as string),
    enabled: !!workspaceId,
  });
}

/** Every chat of a workspace including closed ones, for the history menu. */
export function useSessionHistory(workspaceId: string | undefined, enabled: boolean) {
  return useQuery({
    queryKey: chatKeys.history(workspaceId ?? ""),
    queryFn: () => ipc.sessionList(workspaceId as string, true),
    enabled: !!workspaceId && enabled,
  });
}

/** The turns of a chat that can be reverted to. */
export function useCheckpoints(sessionId: string) {
  return useQuery({
    queryKey: checkpointKeys.list(sessionId),
    queryFn: () => ipc.checkpointList(sessionId),
  });
}

/** The last revert of a chat, while it can still be undone. */
export function useLastRevert(sessionId: string) {
  return useQuery({
    queryKey: checkpointKeys.lastRevert(sessionId),
    queryFn: () => ipc.checkpointLastRevert(sessionId),
  });
}

export function useSlashCommands(workspaceId: string | undefined) {
  return useQuery({
    queryKey: chatKeys.commands(workspaceId ?? ""),
    queryFn: () => ipc.slashCommands(workspaceId as string),
    enabled: !!workspaceId,
    staleTime: 30_000,
  });
}

function useSessionMutation<TVars, TData>(
  workspaceId: string,
  fn: (vars: TVars) => Promise<TData>,
) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: fn,
    onSettled: () => queryClient.invalidateQueries({ queryKey: ["sessions", workspaceId] }),
  });
}

export const useRenameSession = (workspaceId: string) =>
  useSessionMutation(workspaceId, (v: { id: string; title: string }) =>
    ipc.sessionRename(v.id, v.title),
  );
export const useCloseSession = (workspaceId: string) =>
  useSessionMutation(workspaceId, (id: string) => ipc.sessionClose(id));
export const useReopenSession = (workspaceId: string) =>
  useSessionMutation(workspaceId, (id: string) => ipc.sessionReopen(id));
export const useReorderSessions = (workspaceId: string) =>
  useSessionMutation(workspaceId, (ids: string[]) => ipc.sessionReorder(workspaceId, ids));

/** Lets the pane of a side question hear its answer. Mount once. */
export function useSideSync() {
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    listen<{
      session_id: string;
      kind: "started" | "delta" | "done" | "failed";
      text: string;
      with_context: boolean;
    }>("side:event", (event) => {
      const p = event.payload;
      useSide.getState().event(p.session_id, p.kind, p.text, p.with_context);
    })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {
        // Not running inside Tauri: nothing to listen to.
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);
}

/**
 * Keeps chat lists fresh and tracks news for the sidebar: a chat that finishes or needs approval
 * in a workspace the user is not looking at marks that workspace as unread. Mount once.
 */
export function useSessionSync(openWorkspaceId: () => string | undefined) {
  const queryClient = useQueryClient();
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    // A model named a chat after its first words were shown.
    const titled = listen<{ workspace_id: string }>("session:title-changed", (event) => {
      queryClient.invalidateQueries({ queryKey: ["sessions", event.payload.workspace_id] });
    });
    listen<SessionChange>("session:changed", (event) => {
      const change = event.payload;
      queryClient.invalidateQueries({ queryKey: ["sessions", change.workspace_id] });
      // A turn that starts or ends changes which checkpoints exist and whether a revert can be undone.
      queryClient.invalidateQueries({ queryKey: ["checkpoints", change.session_id] });
      const looking = openWorkspaceId() === change.workspace_id && document.hasFocus();
      if (!looking && (change.status === "idle" || change.status === "blocked")) {
        useChat.getState().markUnread(change.workspace_id);
      }
    })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {
        // Not running inside Tauri: nothing to listen to.
      });
    return () => {
      cancelled = true;
      unlisten?.();
      void titled.then((fn) => fn()).catch(() => {});
    };
  }, [queryClient, openWorkspaceId]);
}

/** The open chats of a workspace and the one that is selected. */
export function useActiveSession(workspaceId: string | undefined) {
  const sessions = useSessions(workspaceId);
  const selected = useChat((s) => (workspaceId ? s.activeTab[workspaceId] : undefined));
  const list = sessions.data ?? [];
  return {
    query: sessions,
    sessions: list,
    active: list.find((s) => s.id === selected) ?? list[0],
  };
}

/** Starts a chat in the workspace and selects it. Resolves to nothing when it fails. */
export function useCreateChat(workspaceId: string | undefined) {
  const queryClient = useQueryClient();
  const prefs = useCreatePrefs();
  const select = useChat((s) => s.selectTab);
  return useCallback(
    async (options: { model?: string | null; effort?: string | null } = {}) => {
      if (!workspaceId) return undefined;
      try {
        const session = await ipc.sessionCreate(workspaceId, {
          model: options.model ?? prefs.model,
          effort: options.effort ?? prefs.effort,
        });
        await queryClient.invalidateQueries({ queryKey: chatKeys.sessions(workspaceId) });
        select(workspaceId, session.id);
        return session;
      } catch (e) {
        toast.error(e instanceof Error ? e.message : String(e));
        return undefined;
      }
    },
    [workspaceId, prefs.model, prefs.effort, queryClient, select],
  );
}

/**
 * Calls `onReverted` when a chat was reverted, or a revert undone, so its view can start again from
 * what is stored. Mount once, in the chat.
 */
export function useRevertedSync(onReverted: (sessionId: string) => void) {
  const queryClient = useQueryClient();
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    listen<{ session_id: string; workspace_id: string }>("session:reverted", (event) => {
      queryClient.invalidateQueries({ queryKey: ["checkpoints", event.payload.session_id] });
      queryClient.invalidateQueries({ queryKey: ["sessions", event.payload.workspace_id] });
      onReverted(event.payload.session_id);
    })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {
        // Not running inside Tauri: nothing to listen to.
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [queryClient, onReverted]);
}
