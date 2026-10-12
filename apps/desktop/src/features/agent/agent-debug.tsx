import type { Message } from "@circles/bindings/Message";
import type { PermissionDecision } from "@circles/bindings/PermissionDecision";
import type { PermissionRequest } from "@circles/bindings/PermissionRequest";
import type { RateLimit } from "@circles/bindings/RateLimit";
import type { Session } from "@circles/bindings/Session";
import type { StreamItem } from "@circles/bindings/StreamItem";
import { Badge } from "@circles/ui/components/badge";
import { Button } from "@circles/ui/components/button";
import { Input } from "@circles/ui/components/input";
import { Channel } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";
import { useWorkspaces } from "@/features/workspaces/api";
import { IpcError, ipc } from "@/lib/ipc";

const MODELS = ["", "haiku", "sonnet", "opus"];
const EFFORTS = ["", "low", "medium", "high"];
const MODES = ["manual", "accept_edits", "plan", "bypass"];

const selectClass = "h-8 rounded-md border border-input bg-background px-2 text-xs text-foreground";

function messageText(message: Message): string {
  const content = message.content as Record<string, unknown> | null;
  if (content && typeof content.text === "string") return content.text;
  return JSON.stringify(content);
}

function describeError(error: unknown): string {
  if (error instanceof IpcError) return `${error.code}: ${error.message}`;
  return error instanceof Error ? error.message : String(error);
}

/** Developer view of the agent harness: raw stream, permissions and controls. Not user-facing. */
export function AgentDebug() {
  const workspaces = useWorkspaces();
  const [workspaceId, setWorkspaceId] = useState("");
  const [sessions, setSessions] = useState<Session[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [model, setModel] = useState("haiku");
  const [effort, setEffort] = useState("");
  const [mode, setMode] = useState("manual");
  const [error, setError] = useState<string | null>(null);

  const guard = useCallback(async <T,>(work: () => Promise<T>): Promise<T | undefined> => {
    try {
      setError(null);
      return await work();
    } catch (e) {
      setError(describeError(e));
      return undefined;
    }
  }, []);

  const refresh = useCallback(
    async (id: string) => {
      const list = await guard(() => ipc.sessionList(id));
      if (list) setSessions(list);
    },
    [guard],
  );

  useEffect(() => {
    if (!workspaceId && workspaces.data?.length) setWorkspaceId(workspaces.data[0].id);
  }, [workspaceId, workspaces.data]);

  useEffect(() => {
    if (!workspaceId) return;
    setSelectedId(null);
    void refresh(workspaceId);
  }, [workspaceId, refresh]);

  const create = async () => {
    const session = await guard(() =>
      ipc.sessionCreate(workspaceId, {
        model: model || null,
        effort: effort || null,
        permission_mode: mode,
      }),
    );
    if (session) {
      await refresh(workspaceId);
      setSelectedId(session.id);
    }
  };

  return (
    <div className="mx-auto flex max-w-4xl flex-col gap-4 px-6 py-8">
      <h1 className="text-xl font-semibold">Agent debug</h1>
      {error ? (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      ) : null}

      <section className="flex flex-wrap items-center gap-2" aria-label="New session">
        <select
          aria-label="Workspace"
          className={selectClass}
          value={workspaceId}
          onChange={(e) => setWorkspaceId(e.target.value)}
        >
          {(workspaces.data ?? []).map((w) => (
            <option key={w.id} value={w.id}>
              {w.title ?? w.city}
            </option>
          ))}
        </select>
        <select
          aria-label="Model"
          className={selectClass}
          value={model}
          onChange={(e) => setModel(e.target.value)}
        >
          {MODELS.map((m) => (
            <option key={m} value={m}>
              {m || "default model"}
            </option>
          ))}
        </select>
        <select
          aria-label="Effort"
          className={selectClass}
          value={effort}
          onChange={(e) => setEffort(e.target.value)}
        >
          {EFFORTS.map((m) => (
            <option key={m} value={m}>
              {m || "default effort"}
            </option>
          ))}
        </select>
        <select
          aria-label="Permission mode"
          className={selectClass}
          value={mode}
          onChange={(e) => setMode(e.target.value)}
        >
          {MODES.map((m) => (
            <option key={m} value={m}>
              {m}
            </option>
          ))}
        </select>
        <Button size="sm" onClick={create} disabled={!workspaceId}>
          New session
        </Button>
      </section>

      <ul className="flex flex-wrap gap-2" aria-label="Sessions">
        {sessions.map((s) => (
          <li key={s.id}>
            <Button
              size="sm"
              variant={s.id === selectedId ? "secondary" : "outline"}
              onClick={() => setSelectedId(s.id)}
            >
              {s.title ?? s.id.slice(-6)} · {s.status}
            </Button>
          </li>
        ))}
      </ul>

      {selectedId ? (
        <SessionPane
          key={selectedId}
          id={selectedId}
          onChanged={() => void refresh(workspaceId)}
          onClosed={() => {
            setSelectedId(null);
            void refresh(workspaceId);
          }}
        />
      ) : null}
    </div>
  );
}

type PaneProps = { id: string; onChanged: () => void; onClosed: () => void };

function SessionPane({ id, onChanged, onClosed }: PaneProps) {
  const [session, setSession] = useState<Session | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [raw, setRaw] = useState<string[]>([]);
  const [live, setLive] = useState<{ text: string; thinking: string }>({ text: "", thinking: "" });
  const [pending, setPending] = useState<PermissionRequest[]>([]);
  const [rateLimit, setRateLimit] = useState<RateLimit | null>(null);
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);
  const changed = useRef(onChanged);
  changed.current = onChanged;

  const run = useCallback(async (work: () => Promise<unknown>) => {
    try {
      setError(null);
      await work();
    } catch (e) {
      setError(describeError(e));
    }
  }, []);

  useEffect(() => {
    let active = true;
    const channel = new Channel<StreamItem>();
    channel.onmessage = (item) => {
      if (!active) return;
      switch (item.type) {
        case "raw":
          setRaw((lines) => [...lines.slice(-299), `${item.kind} ${item.line}`]);
          break;
        case "message":
          setMessages((list) => (list.some((m) => m.id === item.id) ? list : [...list, item]));
          setLive({ text: "", thinking: "" });
          break;
        case "delta":
          setLive((l) =>
            item.kind === "thinking"
              ? { ...l, thinking: l.thinking + item.text }
              : { ...l, text: l.text + item.text },
          );
          break;
        case "status":
          setSession((s) =>
            s ? { ...s, status: item.status, running: item.running, last_error: item.error } : s,
          );
          if (item.error) setError(item.error);
          changed.current();
          break;
        case "permission":
          setPending((list) =>
            list.some((p) => p.request_id === item.request_id) ? list : [...list, item],
          );
          break;
        case "permission_resolved":
          setPending((list) => list.filter((p) => p.request_id !== item.request_id));
          break;
        case "result":
          ipc
            .sessionGet(id)
            .then((s) => active && setSession(s))
            .catch(() => {});
          break;
        case "rate_limit":
          setRateLimit(item);
          break;
        default:
          break;
      }
    };
    (async () => {
      try {
        const snapshot = await ipc.sessionAttach(id, channel);
        if (!active) return;
        setSession(snapshot.session);
        setPending(snapshot.pending);
        const history = await ipc.sessionMessages(id);
        if (active) setMessages(history);
        const limit = await ipc.claudeRateLimit();
        if (active) setRateLimit(limit);
      } catch (e) {
        if (active) setError(describeError(e));
      }
    })();
    return () => {
      active = false;
      void ipc.sessionDetach(id).catch(() => {});
    };
  }, [id]);

  const send = () => {
    const text = draft.trim();
    if (!text) return;
    setDraft("");
    void run(() => ipc.sessionSend(id, text));
  };

  const reply = (request: PermissionRequest, decision: PermissionDecision) =>
    run(() => ipc.permissionReply(id, request.request_id, decision));

  if (!session) {
    return error ? (
      <p role="alert" className="text-sm text-destructive">
        {error}
      </p>
    ) : null;
  }

  return (
    <section
      className="flex flex-col gap-3 rounded-lg border border-border p-3"
      aria-label="Session"
    >
      <header className="flex flex-wrap items-center gap-2 text-xs">
        <Badge data-testid="status">{session.status}</Badge>
        <span>{session.running ? "process running" : "no process"}</span>
        <span>
          {session.model ?? "default model"} · {session.permission_mode}
        </span>
        <span>
          {session.turns} turns · ${session.total_cost_usd.toFixed(4)}
        </span>
        {rateLimit ? (
          <span>
            rate limit {rateLimit.status} ({rateLimit.window})
          </span>
        ) : null}
      </header>
      {error ? (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      ) : null}

      <div className="flex flex-wrap gap-2">
        <Button size="sm" variant="outline" onClick={() => run(() => ipc.sessionResume(id))}>
          Resume
        </Button>
        <Button size="sm" variant="outline" onClick={() => run(() => ipc.sessionInterrupt(id))}>
          Interrupt
        </Button>
        <Button size="sm" variant="outline" onClick={() => run(() => ipc.sessionStop(id))}>
          Stop
        </Button>
        <Button
          size="sm"
          variant="outline"
          onClick={() =>
            run(async () => {
              await ipc.sessionClose(id);
              onClosed();
            })
          }
        >
          Close
        </Button>
      </div>

      <ol className="flex flex-col gap-1 text-sm" aria-label="Messages">
        {messages.map((m) => (
          <li key={m.id} className="font-mono text-xs">
            <span className="text-muted-foreground">
              {m.role}/{m.kind}
            </span>{" "}
            {messageText(m)}
          </li>
        ))}
        {live.thinking ? (
          <li className="font-mono text-xs italic text-muted-foreground">{live.thinking}</li>
        ) : null}
        {live.text ? (
          <li className="font-mono text-xs" data-testid="live">
            {live.text}
          </li>
        ) : null}
      </ol>

      {pending.map((request) => (
        <fieldset
          key={request.request_id}
          aria-label={`Permission for ${request.tool_name}`}
          className="flex flex-col gap-2 rounded-md border border-warning p-2 text-xs"
        >
          <p className="font-medium">{request.display_name ?? request.tool_name} wants to run</p>
          <pre className="overflow-x-auto font-mono">{JSON.stringify(request.input, null, 2)}</pre>
          {request.blocked_path ? <p>Outside the workspace: {request.blocked_path}</p> : null}
          <div className="flex gap-2">
            <Button size="sm" onClick={() => reply(request, { decision: "allow" })}>
              Allow
            </Button>
            <Button
              size="sm"
              variant="outline"
              onClick={() => reply(request, { decision: "allow_session" })}
            >
              Allow for session
            </Button>
            <Button
              size="sm"
              variant="outline"
              onClick={() => reply(request, { decision: "deny", message: null })}
            >
              Deny
            </Button>
          </div>
        </fieldset>
      ))}

      <form
        className="flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          send();
        }}
      >
        <Input
          aria-label="Message"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          placeholder="Message the agent"
        />
        <Button type="submit" size="sm">
          Send
        </Button>
      </form>

      <details>
        <summary className="cursor-pointer text-xs text-muted-foreground">
          Raw stream ({raw.length})
        </summary>
        <pre
          role="log"
          aria-label="Raw stream"
          className="max-h-64 overflow-auto whitespace-pre-wrap font-mono text-[11px]"
        >
          {raw.join("\n")}
        </pre>
      </details>
    </section>
  );
}
