import type { Message } from "@circles/bindings/Message";
import type { PermissionRequest } from "@circles/bindings/PermissionRequest";
import type { RateLimit } from "@circles/bindings/RateLimit";
import type { Session } from "@circles/bindings/Session";
import type { StreamItem } from "@circles/bindings/StreamItem";
import { Channel } from "@tauri-apps/api/core";
import { useCallback, useEffect, useReducer, useRef } from "react";
import { IpcError, ipc } from "@/lib/ipc";

/** Text Claude is writing right now, before it is stored as a message. */
export type Live = { text: string; thinking: string };

export type SessionView = {
  session: Session | null;
  messages: Message[];
  live: Live;
  pending: PermissionRequest[];
  rateLimit: RateLimit | null;
  error: string | null;
  loaded: boolean;
};

type Action =
  | { type: "loaded"; session: Session; pending: PermissionRequest[]; messages: Message[] }
  | { type: "item"; item: StreamItem }
  | { type: "session"; session: Session }
  | { type: "rate_limit"; limit: RateLimit | null }
  | { type: "error"; error: string | null };

const EMPTY: SessionView = {
  session: null,
  messages: [],
  live: { text: "", thinking: "" },
  pending: [],
  rateLimit: null,
  error: null,
  loaded: false,
};

/** Adds messages, skipping ones already known: history and the stream overlap on attach. */
function merge(known: Message[], incoming: Message[]): Message[] {
  const ids = new Set(known.map((m) => m.id));
  const added = incoming.filter((m) => !ids.has(m.id));
  if (added.length === 0) return known;
  return [...known, ...added].sort((a, b) => a.seq - b.seq);
}

export function reduce(state: SessionView, action: Action): SessionView {
  switch (action.type) {
    case "loaded":
      return {
        ...state,
        session: action.session,
        pending: action.pending,
        messages: merge(state.messages, action.messages),
        loaded: true,
      };
    case "session":
      return { ...state, session: action.session };
    case "rate_limit":
      return { ...state, rateLimit: action.limit };
    case "error":
      return { ...state, error: action.error };
    case "item": {
      const item = action.item;
      switch (item.type) {
        case "message":
          return {
            ...state,
            messages: merge(state.messages, [item]),
            // Stored text replaces what was streaming. A tool call or result does not.
            live:
              item.role === "assistant" && (item.kind === "text" || item.kind === "thinking")
                ? { ...state.live, [item.kind]: "" }
                : state.live,
          };
        case "delta":
          return {
            ...state,
            live:
              item.kind === "thinking"
                ? { ...state.live, thinking: state.live.thinking + item.text }
                : item.kind === "text"
                  ? { ...state.live, text: state.live.text + item.text }
                  : state.live,
          };
        case "status":
          return {
            ...state,
            session: state.session
              ? {
                  ...state.session,
                  status: item.status,
                  running: item.running,
                  last_error: item.error,
                }
              : state.session,
            live: item.status === "working" ? state.live : { text: "", thinking: "" },
            error: item.error ?? state.error,
          };
        case "permission":
          return state.pending.some((p) => p.request_id === item.request_id)
            ? state
            : { ...state, pending: [...state.pending, item] };
        case "permission_resolved":
          return {
            ...state,
            pending: state.pending.filter((p) => p.request_id !== item.request_id),
          };
        case "rate_limit":
          return { ...state, rateLimit: item };
        default:
          return state;
      }
    }
  }
}

export function describeError(error: unknown): string {
  if (error instanceof IpcError) return error.message;
  return error instanceof Error ? error.message : String(error);
}

/**
 * Follows one chat: loads its history, attaches to its live stream and keeps the pieces in step.
 * `onResult` is called when a turn ends, so the caller can refresh cost and counters.
 */
export function useSession(id: string) {
  const [state, dispatch] = useReducer(reduce, EMPTY);
  const idRef = useRef(id);
  idRef.current = id;

  const refresh = useCallback(async () => {
    try {
      dispatch({ type: "session", session: await ipc.sessionGet(idRef.current) });
    } catch {
      // The chat may have been deleted while the view was closing.
    }
  }, []);

  useEffect(() => {
    let active = true;
    const channel = new Channel<StreamItem>();
    channel.onmessage = (item) => {
      if (!active) return;
      dispatch({ type: "item", item });
      if (item.type === "result") void refresh();
    };
    (async () => {
      try {
        const snapshot = await ipc.sessionAttach(id, channel);
        const messages = await ipc.sessionMessages(id);
        if (!active) return;
        dispatch({
          type: "loaded",
          session: snapshot.session,
          pending: snapshot.pending,
          messages,
        });
        const limit = await ipc.claudeRateLimit();
        if (active) dispatch({ type: "rate_limit", limit });
      } catch (e) {
        if (active) dispatch({ type: "error", error: describeError(e) });
      }
    })();
    return () => {
      active = false;
      void ipc.sessionDetach(id).catch(() => {});
    };
  }, [id, refresh]);

  return {
    ...state,
    refresh,
    setError: (error: string | null) => dispatch({ type: "error", error }),
    setSession: (session: Session) => dispatch({ type: "session", session }),
  };
}
