import type { Session } from "@circles/bindings/Session";
import { act, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { backend, FakeChannel } from "@/test/backend";
import { defaultBackend, renderApp } from "@/test/render";

function session(overrides: Partial<Session> = {}): Session {
  return {
    id: "s1",
    workspace_id: "w1",
    claude_session_id: null,
    title: null,
    status: "idle",
    model: "haiku",
    effort: null,
    permission_mode: "manual",
    position: 0,
    total_cost_usd: 0,
    turns: 0,
    last_error: null,
    running: false,
    created_at: 1,
    updated_at: 1,
    working_since: null,
    ...overrides,
  };
}

const message = (seq: number, role: string, kind: string, text: string) => ({
  id: `m${seq}`,
  session_id: "s1",
  seq,
  role,
  kind,
  content: { text },
  created_at: 1,
});

function setup(sessions: Session[] = [session()]) {
  defaultBackend();
  backend
    .on("session_list", sessions)
    .on("session_get", sessions[0] ?? session())
    .on("session_attach", { session: sessions[0] ?? session(), pending: [] })
    .on("session_detach", null)
    .on("session_messages", [])
    .on("claude_rate_limit", null)
    .on("session_send", message(1, "user", "text", "hi"))
    .on("session_interrupt", null)
    .on("session_stop", null)
    .on("session_resume", session({ running: true }))
    .on("session_close", null)
    .on("permission_reply", null)
    .on("session_create", session({ id: "s2" }));
  return renderApp("/debug/agent");
}

async function open(app: ReturnType<typeof setup>) {
  await app.user.click(await screen.findByRole("button", { name: /idle/ }));
  await screen.findByRole("region", { name: "Session" });
  return FakeChannel.last;
}

describe("agent debug panel", () => {
  it("lists a workspace's sessions and shows its status", async () => {
    const app = setup();
    await open(app);
    expect(screen.getByTestId("status")).toHaveTextContent("idle");
    expect(backend.callsTo("session_attach")).toHaveLength(1);
  });

  it("creates a session with the chosen model, effort and mode", async () => {
    const app = setup();
    await screen.findByRole("list", { name: "Sessions" });
    await app.user.selectOptions(screen.getByLabelText("Model"), "opus");
    await app.user.selectOptions(screen.getByLabelText("Effort"), "high");
    await app.user.selectOptions(screen.getByLabelText("Permission mode"), "plan");
    await app.user.click(screen.getByRole("button", { name: "New session" }));
    await waitFor(() => expect(backend.callsTo("session_create")).toHaveLength(1));
    expect(backend.callsTo("session_create")[0]).toMatchObject({
      workspaceId: "w1",
      options: { model: "opus", effort: "high", permission_mode: "plan" },
    });
  });

  it("sends a message and shows streamed text, messages and the raw log", async () => {
    const app = setup();
    const channel = await open(app);
    await app.user.type(screen.getByLabelText("Message"), "hello{Enter}");
    await waitFor(() =>
      expect(backend.callsTo("session_send")[0]).toMatchObject({ id: "s1", text: "hello" }),
    );

    act(() => {
      channel.emit({ type: "status", status: "working", running: true, error: null });
      channel.emit({
        type: "delta",
        block_index: 0,
        kind: "text",
        text: "Hel",
        parent_tool_use_id: null,
      });
      channel.emit({
        type: "delta",
        block_index: 0,
        kind: "text",
        text: "lo",
        parent_tool_use_id: null,
      });
      channel.emit({ type: "raw", seq: 1, kind: "assistant", line: "{}" });
    });
    expect(screen.getByTestId("status")).toHaveTextContent("working");
    expect(screen.getByTestId("live")).toHaveTextContent("Hello");
    expect(screen.getByLabelText("Raw stream")).toHaveTextContent("assistant {}");

    act(() => {
      channel.emit({ type: "message", ...message(2, "assistant", "text", "Hello there") });
    });
    expect(screen.queryByTestId("live")).not.toBeInTheDocument();
    expect(screen.getByRole("list", { name: "Messages" })).toHaveTextContent("Hello there");
  });

  it("shows cost, turns and the rate limit after a result", async () => {
    const app = setup();
    const channel = await open(app);
    backend.on("session_get", session({ turns: 2, total_cost_usd: 0.0123 }));
    act(() => {
      channel.emit({
        type: "rate_limit",
        status: "allowed",
        window: "five_hour",
        resets_at: null,
        five_hour_utilization: null,
        seven_day_utilization: null,
        using_overage: false,
      });
      channel.emit({
        type: "result",
        is_error: false,
        subtype: "success",
        cost_usd: 0.01,
        turns: 1,
        duration_ms: 10,
      });
    });
    expect(await screen.findByText(/2 turns · \$0\.0123/)).toBeInTheDocument();
    expect(screen.getByText(/rate limit allowed \(five_hour\)/)).toBeInTheDocument();
  });

  it("answers permission requests with allow, allow for session and deny", async () => {
    const app = setup();
    const channel = await open(app);
    const request = (id: string) => ({
      type: "permission",
      request_id: id,
      session_id: "s1",
      tool_name: "Bash",
      display_name: null,
      input: { command: "ls" },
      tool_use_id: null,
      description: null,
      blocked_path: null,
      created_at: 1,
    });
    act(() => channel.emit(request("r1")));
    const card = await screen.findByRole("group", { name: "Permission for Bash" });
    expect(card).toHaveTextContent('"command": "ls"');
    await app.user.click(screen.getByRole("button", { name: "Allow for session" }));
    expect(backend.callsTo("permission_reply")[0]).toMatchObject({
      sessionId: "s1",
      requestId: "r1",
      decision: { decision: "allow_session" },
    });

    act(() => channel.emit({ type: "permission_resolved", request_id: "r1" }));
    expect(screen.queryByRole("group", { name: "Permission for Bash" })).not.toBeInTheDocument();

    act(() => channel.emit(request("r2")));
    await app.user.click(await screen.findByRole("button", { name: "Deny" }));
    expect(backend.callsTo("permission_reply")[1]).toMatchObject({
      requestId: "r2",
      decision: { decision: "deny", message: null },
    });
  });

  it("interrupts, stops, resumes and closes", async () => {
    const app = setup();
    await open(app);
    await app.user.click(screen.getByRole("button", { name: "Interrupt" }));
    await app.user.click(screen.getByRole("button", { name: "Stop" }));
    await app.user.click(screen.getByRole("button", { name: "Resume" }));
    await app.user.click(screen.getByRole("button", { name: "Close" }));
    await waitFor(() => expect(backend.callsTo("session_close")).toHaveLength(1));
    expect(backend.callsTo("session_interrupt")).toHaveLength(1);
    expect(backend.callsTo("session_stop")).toHaveLength(1);
    expect(backend.callsTo("session_resume")).toHaveLength(1);
    await waitFor(() =>
      expect(screen.queryByRole("region", { name: "Session" })).not.toBeInTheDocument(),
    );
  });

  it("shows a missing Claude Code error", async () => {
    const app = setup();
    await open(app);
    backend.fail("session_send", {
      code: "claude_missing",
      message: "Claude Code is not installed.",
    });
    await app.user.type(screen.getByLabelText("Message"), "hi{Enter}");
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "claude_missing: Claude Code is not installed.",
    );
  });

  it("detaches when the session is left", async () => {
    const app = setup();
    await open(app);
    app.router.navigate("/debug");
    await waitFor(() => expect(backend.callsTo("session_detach")).toHaveLength(1));
  });
});
