import type { Message } from "@circles/bindings/Message";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useChat } from "@/stores/chat";
import { useSide } from "@/stores/side";
import { backend, emitBackendEvent, type FakeChannel } from "@/test/backend";
import { session, workspace } from "@/test/fixtures";
import { chatBackend, defaultBackend, renderApp, settingsBackend } from "@/test/render";
import { highlight, LONG_PASTE } from "./composer";
import { VIRTUALISE_FROM } from "./message-list";
import { workLabel } from "./work-block";

let seq = 0;
function msg(
  role: Message["role"],
  kind: Message["kind"],
  content: Record<string, unknown>,
): Message {
  seq += 1;
  return { id: `m${seq}`, session_id: "s1", seq, role, kind, content, created_at: seq * 1000 };
}

function open(history: Message[] = [], existing = [session()]) {
  seq = 0;
  defaultBackend();
  chatBackend(existing);
  backend
    .on("workspace_get", workspace({ id: "w1", city: "Algiers" }))
    .on("session_messages", history);
  return renderApp("/workspace/w1");
}

function openWith(general: Record<string, string>) {
  const app = open();
  settingsBackend({ user: { general } });
  return app;
}

async function pane(app: ReturnType<typeof open>) {
  await screen.findByRole("region", { name: "Chat" });
  await waitFor(() => expect(backend.callsTo("session_attach").length).toBeGreaterThan(0));
  const calls = backend.callsTo("session_attach");
  const channel = calls[calls.length - 1].onEvent as FakeChannel;
  await screen.findByLabelText("Message");
  return { app, channel };
}

const chatTabs = () => within(screen.getByRole("tablist", { name: "Chats" })).getAllByRole("tab");
const box = () => screen.getByLabelText("Message") as HTMLTextAreaElement;

beforeEach(() => {
  useChat.setState(useChat.getInitialState(), true);
});

describe("chat messages", () => {
  it("shows the conversation with markdown, grouped tool calls and a summary", async () => {
    const { app } = await pane(
      open([
        msg("user", "text", { text: "make it so" }),
        msg("assistant", "text", { text: "Here is **bold** and `code`" }),
        msg("assistant", "tool_call", {
          id: "a",
          name: "Read",
          input: { file_path: "/home/circles/workspaces/roster/algiers/a.ts" },
        }),
        msg("user", "tool_result", { tool_use_id: "a", content: "file body" }),
        msg("assistant", "tool_call", {
          id: "b",
          name: "Edit",
          input: { file_path: "/x/b.ts", old_string: "old line", new_string: "new line" },
        }),
        msg("user", "tool_result", { tool_use_id: "b", content: "ok" }),
        msg("assistant", "text", { text: "Done." }),
      ]),
    );
    expect(await screen.findByText("make it so")).toBeInTheDocument();
    expect(screen.getByText("bold").tagName).toBe("STRONG");
    expect(screen.getByText("code").tagName).toBe("CODE");

    const group = screen.getByRole("button", { name: /2 tool calls/ });
    expect(group).toHaveAttribute("aria-expanded", "false");
    await app.user.click(group);
    await app.user.click(screen.getByRole("button", { name: /Edit/ }));
    expect(screen.getByText("- old line")).toBeInTheDocument();
    expect(screen.getByText("+ new line")).toBeInTheDocument();
    // The summary row lists the changed file.
    expect(screen.getByText("b.ts")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Copy reply" })).toBeInTheDocument();
  });

  it("floats the message box over the conversation and leaves room for it at the end", async () => {
    await pane(
      open([msg("user", "text", { text: "hello" }), msg("assistant", "text", { text: "hi" })]),
    );
    expect(await screen.findByText("hello")).toBeInTheDocument();
    const dock = screen.getByTestId("chat-dock");
    expect(dock).toHaveClass("absolute", "bottom-0");
    // Clicks beside the box reach the messages under it.
    expect(dock).toHaveClass("pointer-events-none");
    const box = within(dock).getByRole("textbox");
    expect(box.closest(".glass-panel")).not.toBeNull();
    // The list's end is padded by the height the pane measured for the dock.
    const content = screen.getByTestId("messages").firstElementChild;
    expect((content as HTMLElement).className).toContain("var(--dock-height");
  });

  it("renders unknown tools generically", async () => {
    const { app } = await pane(
      open([
        msg("user", "text", { text: "go" }),
        msg("assistant", "tool_call", {
          id: "m",
          name: "mcp__db__query",
          input: { sql: "select 1" },
        }),
        msg("user", "tool_result", { tool_use_id: "m", content: "1" }),
      ]),
    );
    await app.user.click(await screen.findByRole("button", { name: /1 tool call/ }));
    await app.user.click(screen.getByRole("button", { name: /db · query/ }));
    expect(screen.getByText(/"sql": "select 1"/)).toBeInTheDocument();
  });

  it("shows thinking and tool calls as one block, opened to see the steps", async () => {
    const { app } = await pane(
      open([
        msg("user", "text", { text: "go" }),
        msg("assistant", "thinking", { text: "first idea" }),
        msg("assistant", "tool_call", {
          id: "a",
          name: "Bash",
          input: { command: "ls", description: "List files" },
        }),
        msg("user", "tool_result", { tool_use_id: "a", content: "x" }),
        msg("assistant", "thinking", { text: "second idea" }),
        msg("assistant", "tool_call", {
          id: "b",
          name: "Bash",
          input: { command: "pwd", description: "Where am I" },
        }),
        msg("user", "tool_result", { tool_use_id: "b", content: "y" }),
      ]),
    );
    const block = await screen.findByRole("button", { name: /2 tool calls/ });
    // One block, and nothing else with its own thinking line.
    expect(screen.getByTestId("messages").querySelectorAll("[aria-expanded]")).toHaveLength(1);
    expect(screen.queryByText("first idea")).not.toBeInTheDocument();
    await app.user.click(block);
    expect(screen.getByText("first idea")).toBeInTheDocument();
    expect(screen.getByText("second idea")).toBeInTheDocument();
    expect(screen.getByText("List files")).toBeInTheDocument();
    expect(screen.getByText("Where am I")).toBeInTheDocument();
  });

  it("picks a name for the block and keeps it", () => {
    expect(workLabel("work-m2")).toBe(workLabel("work-m2"));
    const names = new Set(
      ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"].map((k) => workLabel(`work-${k}`)),
    );
    expect(names.size).toBeGreaterThan(1);
  });

  it("shows streamed text as it arrives and replaces it with the stored message", async () => {
    const { channel } = await pane(open());
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
    });
    expect(await screen.findByText("Hello")).toBeInTheDocument();
    act(() => {
      channel.emit({ type: "message", ...msg("assistant", "text", { text: "Hello" }) });
    });
    expect(screen.getAllByText("Hello")).toHaveLength(1);
  });

  it("shows a closed thinking block that opens on click", async () => {
    const history = [
      msg("user", "text", { text: "hm" }),
      msg("assistant", "thinking", { text: "pondering" }),
    ];
    const { app } = await pane(open(history));
    expect(screen.queryByText("pondering")).not.toBeInTheDocument();
    await app.user.click(
      await screen.findByRole("button", { name: workLabel(`work-${history[1].id}`) }),
    );
    expect(screen.getByText("pondering")).toBeInTheDocument();
  });
  it("virtualises a very long conversation", async () => {
    // jsdom has no layout, so give elements a size the virtualiser can work with.
    const height = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetHeight");
    const width = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetWidth");
    Object.defineProperty(HTMLElement.prototype, "offsetHeight", {
      configurable: true,
      get: () => 100,
    });
    Object.defineProperty(HTMLElement.prototype, "offsetWidth", {
      configurable: true,
      get: () => 800,
    });
    try {
      const history: Message[] = [];
      for (let i = 0; i < VIRTUALISE_FROM + 200; i++) {
        history.push(msg("assistant", "text", { text: `line ${i}` }));
      }
      await pane(open(history));
      await waitFor(() =>
        expect(document.querySelectorAll("[data-row-key]").length).toBeGreaterThan(0),
      );
      expect(document.querySelectorAll("[data-row-key]").length).toBeLessThan(history.length);
    } finally {
      if (height) Object.defineProperty(HTMLElement.prototype, "offsetHeight", height);
      if (width) Object.defineProperty(HTMLElement.prototype, "offsetWidth", width);
    }
  });
});

describe("permission prompts", () => {
  it("shows a card for a blocked tool and answers it", async () => {
    const { app, channel } = await pane(open());
    act(() => {
      channel.emit({ type: "status", status: "blocked", running: true, error: null });
      channel.emit({
        type: "permission",
        request_id: "r1",
        session_id: "s1",
        tool_name: "Bash",
        display_name: null,
        input: { command: "rm -rf build" },
        tool_use_id: null,
        description: null,
        blocked_path: null,
        created_at: 1,
      });
    });
    const card = await screen.findByRole("group", { name: "Permission for Bash" });
    expect(card).toHaveTextContent("rm -rf build");
    await app.user.click(within(card).getByRole("button", { name: "Allow for session" }));
    expect(backend.callsTo("permission_reply")[0]).toMatchObject({
      sessionId: "s1",
      requestId: "r1",
      decision: { decision: "allow_session" },
    });
    act(() => channel.emit({ type: "permission_resolved", request_id: "r1" }));
    expect(screen.queryByRole("group", { name: "Permission for Bash" })).not.toBeInTheDocument();
  });
});

describe("composer", () => {
  it("sends with Enter, keeps Shift+Enter for new lines and names the chat", async () => {
    const { app } = await pane(open());
    await app.user.type(box(), "first line{Shift>}{Enter}{/Shift}second");
    expect(box().value).toBe("first line\nsecond");
    await app.user.keyboard("{Enter}");
    await waitFor(() => expect(backend.callsTo("session_send")).toHaveLength(1));
    expect(backend.callsTo("session_send")[0]).toMatchObject({
      id: "s1",
      text: "first line\nsecond",
    });
    expect(box().value).toBe("");
    // Its first words at once. The model's title is asked for behind the scenes, with the text.
    await waitFor(() =>
      expect(backend.callsTo("session_auto_title")[0]).toMatchObject({
        sessionId: "s1",
        title: "first line",
        text: "first line\nsecond",
      }),
    );
    expect(backend.callsTo("session_rename")).toHaveLength(0);
  });

  it("shows a suggested next message as the placeholder and takes it with Tab", async () => {
    const { app, channel } = await pane(open());
    backend.on("smart_suggest", "Open a pull request for this fix");
    act(() => channel.emit({ type: "status", status: "working", running: true, error: null }));
    act(() => {
      channel.emit({ type: "message", ...msg("user", "text", { text: "fix the checkout test" }) });
      channel.emit({ type: "message", ...msg("assistant", "text", { text: "Done, it passes." }) });
    });
    act(() => channel.emit({ type: "status", status: "idle", running: true, error: null }));
    // The model is asked with the last exchange.
    await waitFor(() =>
      expect(backend.callsTo("smart_suggest")[0]).toEqual({
        asked: "fix the checkout test",
        answer: "Done, it passes.",
      }),
    );
    await waitFor(() => expect(box().placeholder).toContain("Open a pull request for this fix"));
    // Nothing is typed for the person until they press Tab, and nothing is sent.
    expect(box().value).toBe("");
    await app.user.click(box());
    await app.user.keyboard("{Tab}");
    expect(box().value).toBe("Open a pull request for this fix");
    expect(backend.callsTo("session_send")).toHaveLength(0);
  });

  it("does not take the suggestion when something is already typed", async () => {
    const { app, channel } = await pane(open());
    backend.on("smart_suggest", "Open a pull request for this fix");
    act(() => channel.emit({ type: "status", status: "working", running: true, error: null }));
    act(() => {
      channel.emit({ type: "message", ...msg("user", "text", { text: "fix it" }) });
      channel.emit({ type: "message", ...msg("assistant", "text", { text: "Done." }) });
    });
    act(() => channel.emit({ type: "status", status: "idle", running: true, error: null }));
    await waitFor(() => expect(box().placeholder).toContain("Open a pull request"));
    await app.user.type(box(), "my own words");
    expect(box().value).toBe("my own words");
  });

  it("does not make an empty chat in a workspace a Composer made", async () => {
    const rendered = open([], []);
    backend.on("composer_workspace_links", [
      { workspace_id: "w1", composer_id: "c1", composer_name: "Task splitter", session_id: "r1" },
    ]);
    await screen.findByRole("region", { name: "Chat" });
    await waitFor(() =>
      expect(backend.callsTo("composer_workspace_links").length).toBeGreaterThan(0),
    );
    expect(backend.callsTo("session_create")).toHaveLength(0);
    void rendered;
  });

  it("makes the first chat of an ordinary workspace", async () => {
    open([], []);
    await screen.findByRole("region", { name: "Chat" });
    await waitFor(() => expect(backend.callsTo("session_create")).toHaveLength(1));
  });

  describe("cards from the agent", () => {
    const card = {
      uid: "u9",
      session_id: "s1",
      composer_id: null,
      card: {
        id: "pick",
        title: "Which branch?",
        body: [
          {
            type: "choice_list",
            id: "b",
            multiple: false,
            required: true,
            options: [
              { value: "main", label: "main", detail: null },
              { value: "develop", label: "develop", detail: null },
            ],
          },
        ],
        submit_label: null,
        outside_text: false,
      },
      status: "pending",
      answer: null,
      created_at: 1,
      answered_at: null,
    };

    it("shows a waiting card above the box and sends the answer", async () => {
      const rendered = open();
      backend.on("genui_cards", [card]).on("genui_answer", null);
      const { app } = await pane(rendered);
      const panel = await screen.findByRole("region", { name: "Cards from the agent" });
      const waiting = within(panel).getByRole("region", { name: "Which branch?" });
      await app.user.click(within(waiting).getByRole("radio", { name: "develop" }));
      await app.user.click(within(waiting).getByRole("button", { name: "Send" }));
      await waitFor(() =>
        expect(backend.callsTo("genui_answer")[0]).toEqual({
          uid: "u9",
          values: { b: ["develop"] },
        }),
      );
      // Not a message of the chat.
      expect(backend.callsTo("session_send")).toHaveLength(0);
    });

    it("keeps an answered card in view, with what was chosen", async () => {
      const rendered = open();
      backend.on("genui_cards", [{ ...card, status: "answered", answer: { b: ["main"] } }]);
      await pane(rendered);
      const done = await screen.findByRole("region", { name: "Which branch?" });
      expect(done).toHaveTextContent("Answered");
      expect(within(done).getByRole("radio", { name: "main" })).toBeChecked();
      expect(within(done).getByRole("radio", { name: "main" })).toBeDisabled();
      expect(within(done).queryByRole("button", { name: "Send" })).not.toBeInTheDocument();
    });
  });

  describe("side questions", () => {
    beforeEach(() => useSide.setState({ bySession: {} }));

    it("lists /btw among the commands", async () => {
      const rendered = open();
      backend.on("slash_commands", [
        {
          name: "btw",
          description: "Ask a side question. The chat is not changed",
          source: "builtin",
        },
      ]);
      const { app } = await pane(rendered);
      await app.user.type(box(), "/bt");
      const list = await screen.findByRole("listbox", { name: "Commands" });
      expect(within(list).getByRole("option", { name: /\/btw/ })).toBeInTheDocument();
    });

    it("answers in a pane above the box, and the chat never hears of it", async () => {
      const { app } = await pane(open());
      backend.on("session_side_question", null);
      await app.user.type(box(), "/btw what does this do?{Enter}");
      await waitFor(() =>
        expect(backend.callsTo("session_side_question")[0]).toEqual({
          sessionId: "s1",
          question: "what does this do?",
        }),
      );
      // Not sent as a message, and the draft is left empty.
      expect(backend.callsTo("session_send")).toHaveLength(0);
      expect(box().value).toBe("");

      const panel = await screen.findByRole("region", { name: "Side question" });
      expect(panel).toHaveTextContent("what does this do?");
      expect(panel).toHaveTextContent("Thinking");

      // The answer streams in.
      act(() => {
        emitBackendEvent("side:event", {
          session_id: "s1",
          kind: "started",
          text: "",
          with_context: true,
        });
        emitBackendEvent("side:event", {
          session_id: "s1",
          kind: "delta",
          text: "It parses ",
          with_context: true,
        });
      });
      await waitFor(() => expect(panel).toHaveTextContent("It parses"));
      expect(within(panel).getByRole("button", { name: "Stop" })).toBeInTheDocument();
      act(() =>
        emitBackendEvent("side:event", {
          session_id: "s1",
          kind: "done",
          text: "It parses the settings file.",
          with_context: true,
        }),
      );
      await waitFor(() => expect(panel).toHaveTextContent("It parses the settings file."));
      expect(within(panel).queryByRole("button", { name: "Stop" })).not.toBeInTheDocument();
      expect(panel).toHaveTextContent("Not part of the chat");
    });

    it("stops with Stop, and closes with the close button", async () => {
      const { app } = await pane(open());
      backend.on("session_side_question", null).on("session_side_stop", null);
      await app.user.type(box(), "/btw anything{Enter}");
      const panel = await screen.findByRole("region", { name: "Side question" });
      await app.user.click(within(panel).getByRole("button", { name: "Stop" }));
      expect(backend.callsTo("session_side_stop")).toEqual([{ sessionId: "s1" }]);
      await app.user.click(within(panel).getByRole("button", { name: "Close side question" }));
      expect(screen.queryByRole("region", { name: "Side question" })).not.toBeInTheDocument();
    });

    it("brings the answer into the box as a quote, only when asked to", async () => {
      const { app } = await pane(open());
      backend.on("session_side_question", null);
      await app.user.type(box(), "/btw why?{Enter}");
      const panel = await screen.findByRole("region", { name: "Side question" });
      act(() =>
        emitBackendEvent("side:event", {
          session_id: "s1",
          kind: "done",
          text: "Because of the cache.",
          with_context: true,
        }),
      );
      await waitFor(() => expect(panel).toHaveTextContent("Because of the cache."));
      await app.user.click(within(panel).getByRole("button", { name: "Add to chat" }));
      expect(box().value).toBe("> why?\n\nBecause of the cache.");
      expect(screen.queryByRole("region", { name: "Side question" })).not.toBeInTheDocument();
      // Nothing was sent. The person can still edit it first.
      expect(backend.callsTo("session_send")).toHaveLength(0);
    });

    it("shows why it failed", async () => {
      const { app } = await pane(open());
      backend.on("session_side_question", null);
      await app.user.type(box(), "/btw hello{Enter}");
      await screen.findByRole("region", { name: "Side question" });
      act(() =>
        emitBackendEvent("side:event", {
          session_id: "s1",
          kind: "failed",
          text: "The answer took too long.",
          with_context: true,
        }),
      );
      expect(await screen.findByRole("alert")).toHaveTextContent("took too long");
    });

    it("says when the chat has not started, so there was nothing to use", async () => {
      const { app } = await pane(open());
      backend.on("session_side_question", null);
      await app.user.type(box(), "/btw hi{Enter}");
      const panel = await screen.findByRole("region", { name: "Side question" });
      act(() =>
        emitBackendEvent("side:event", {
          session_id: "s1",
          kind: "started",
          text: "",
          with_context: false,
        }),
      );
      await waitFor(() => expect(panel).toHaveTextContent("has not started"));
    });

    it("Cmd+Shift+B opens a side question in the box", async () => {
      const { app } = await pane(open());
      await app.user.click(box());
      await app.user.keyboard("{Meta>}{Shift>}b{/Shift}{/Meta}");
      expect(box().value).toBe("/btw ");
    });

    it("keeps an empty /btw out of the chat too", async () => {
      const { app } = await pane(open());
      backend.on("session_side_question", null);
      await app.user.type(box(), "/btw{Enter}");
      // No question: it is a plain message, which is how the command is told to be used.
      expect(backend.callsTo("session_side_question")).toHaveLength(0);
    });
  });

  it("can be set to send with Cmd+Enter", async () => {
    const { app } = await pane(openWith({ send_key: "mod_enter" }));
    await app.user.type(box(), "hello{Enter}");
    expect(backend.callsTo("session_send")).toHaveLength(0);
    await app.user.keyboard("{Meta>}{Enter}{/Meta}");
    await waitFor(() => expect(backend.callsTo("session_send")).toHaveLength(1));
  });

  it("offers Stop while Claude works, and steers with Enter", async () => {
    const { app, channel } = await pane(open());
    act(() => channel.emit({ type: "status", status: "working", running: true, error: null }));
    await app.user.click(await screen.findByRole("button", { name: "Stop" }));
    expect(backend.callsTo("session_interrupt")).toEqual([{ id: "s1" }]);
    await app.user.type(box(), "actually do this instead{Enter}");
    await waitFor(() => expect(backend.callsTo("session_send")).toHaveLength(1));
    expect(useChat.getState().queues.s1 ?? []).toHaveLength(0);
  });

  it("queues with Cmd+Enter while working, and sends the queue when the turn ends", async () => {
    const { app, channel } = await pane(open());
    act(() => channel.emit({ type: "status", status: "working", running: true, error: null }));
    await screen.findByRole("button", { name: "Stop" });
    await app.user.type(box(), "then run the tests");
    await app.user.keyboard("{Meta>}{Enter}{/Meta}");
    const queued = await screen.findByRole("list", { name: "Queued messages" });
    expect(queued).toHaveTextContent("then run the tests");
    expect(backend.callsTo("session_send")).toHaveLength(0);

    act(() => channel.emit({ type: "status", status: "idle", running: true, error: null }));
    await waitFor(() => expect(backend.callsTo("session_send")).toHaveLength(1));
    expect(backend.callsTo("session_send")[0]).toMatchObject({ text: "then run the tests" });
    await waitFor(() =>
      expect(screen.queryByRole("list", { name: "Queued messages" })).not.toBeInTheDocument(),
    );
  });

  it("edits and cancels queued messages", async () => {
    const { app, channel } = await pane(openWith({ follow_up: "queue" }));
    act(() => channel.emit({ type: "status", status: "working", running: true, error: null }));
    await screen.findByRole("button", { name: "Stop" });
    await app.user.type(box(), "one{Enter}");
    await app.user.type(box(), "two{Enter}");
    await app.user.click(await screen.findByRole("button", { name: "one" }));
    const input = screen.getByLabelText("Edit queued message");
    await app.user.clear(input);
    await app.user.type(input, "uno{Enter}");
    expect(await screen.findByRole("button", { name: "uno" })).toBeInTheDocument();
    await app.user.click(screen.getAllByRole("button", { name: "Cancel queued message" })[1]);
    expect(screen.queryByRole("button", { name: "two" })).not.toBeInTheDocument();
  });

  it("suggests files after @ and inserts the choice", async () => {
    const rendered = open();
    backendFiles();
    const { app } = await pane(rendered);
    await app.user.type(box(), "look at @but");
    const list = await screen.findByRole("listbox", { name: "Files" });
    expect(within(list).getByRole("option", { name: /src\/button\.tsx/ })).toBeInTheDocument();
    await app.user.keyboard("{Enter}");
    expect(box().value).toBe("look at @src/button.tsx ");
    expect(backend.callsTo("session_send")).toHaveLength(0);
    expect(backend.callsTo("workspace_search_files")[0]).toMatchObject({ workspaceId: "w1" });
  });

  it("suggests slash commands and keeps typing free of them otherwise", async () => {
    const rendered = open();
    backend.on("slash_commands", [
      { name: "review", description: "Review the diff", source: "project" },
      { name: "compact", description: "Summarise", source: "builtin" },
    ]);
    const { app } = await pane(rendered);
    await app.user.type(box(), "/rev");
    const list = await screen.findByRole("listbox", { name: "Commands" });
    expect(within(list).getAllByRole("option")).toHaveLength(1);
    await app.user.keyboard("{Tab}");
    expect(box().value).toBe("/review ");
    await app.user.clear(box());
    await app.user.type(box(), "no /slash here");
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
  });

  it("attaches a file and sends it with the message", async () => {
    backend.on("attachment_save", {
      name: "shot.png",
      path: "/w/.context/attachments/ab-shot.png",
      size: 3,
    });
    const { app } = await pane(open());
    const file = new File(["abc"], "shot.png", { type: "image/png" });
    await app.user.upload(screen.getByLabelText("Attach files"), file);
    expect(await screen.findByText("shot.png")).toBeInTheDocument();
    expect(backend.callsTo("attachment_save")[0]).toMatchObject({
      workspaceId: "w1",
      name: "shot.png",
      dataBase64: "YWJj",
    });
    await app.user.type(box(), "see this{Enter}");
    await waitFor(() => expect(backend.callsTo("session_send")).toHaveLength(1));
    expect(backend.callsTo("session_send")[0].attachments).toEqual([
      { name: "shot.png", path: "/w/.context/attachments/ab-shot.png", size: 3 },
    ]);
  });

  it("turns a very long paste into an attachment", async () => {
    backend.on("attachment_save", {
      name: "pasted-text.txt",
      path: "/w/.context/attachments/ab-pasted-text.txt",
      size: 9,
    });
    const { app } = await pane(open());
    box().focus();
    fireEvent.paste(box(), {
      clipboardData: { files: [], getData: () => "x".repeat(LONG_PASTE + 1) },
    });
    expect(await screen.findByText("pasted-text.txt")).toBeInTheDocument();
    expect(box().value).toBe("");
    expect(app.user).toBeDefined();
  });

  it("pastes short text normally", async () => {
    await pane(open());
    box().focus();
    fireEvent.paste(box(), { clipboardData: { files: [], getData: () => "short" } });
    expect(backend.callsTo("attachment_save")).toHaveLength(0);
  });

  it("changes model, effort and plan mode for the chat", async () => {
    const { app } = await pane(open());
    await app.user.selectOptions(screen.getByLabelText("Model"), "sonnet");
    expect(backend.callsTo("session_update")[0]).toMatchObject({ id: "s1", model: "sonnet" });
    await app.user.click(screen.getByRole("button", { name: "Effort" }));
    const slider = await screen.findByRole("slider", { name: "Effort level" });
    expect(slider).toHaveAttribute("aria-valuetext", "High");
    expect(screen.getByText("Recommended")).toBeInTheDocument();
    fireEvent.change(slider, { target: { value: "1" } });
    expect(backend.callsTo("session_update")[1]).toMatchObject({ id: "s1", effort: "medium" });
    await app.user.keyboard("{Escape}");
    expect(screen.queryByRole("button", { name: "Plan" })).not.toBeInTheDocument();
    await app.user.selectOptions(screen.getByLabelText("Permission mode"), "plan");
    expect(backend.callsTo("session_update")[2]).toMatchObject({ permissionMode: "plan" });
  });

  it("switches an open chat to bypass or any other permission mode", async () => {
    const { app } = await pane(open());
    const select = screen.getByLabelText("Permission mode");
    expect(select).toHaveValue("manual");
    await app.user.selectOptions(select, "bypass");
    expect(backend.callsTo("session_update").slice(-1)[0]).toMatchObject({
      id: "s1",
      permissionMode: "bypass",
    });
    await app.user.selectOptions(select, "accept_edits");
    expect(backend.callsTo("session_update").slice(-1)[0]).toMatchObject({
      permissionMode: "accept_edits",
    });
  });

  it("keeps a draft per chat", async () => {
    const { app } = await pane(open());
    await app.user.type(box(), "unfinished");
    expect(useChat.getState().drafts.s1).toBe("unfinished");
  });

  it("starts from a suggestion chip", async () => {
    const { app } = await pane(open());
    await app.user.click(await screen.findByRole("button", { name: /Find and fix a bug/ }));
    expect(box().value).toBe("Find and fix a bug");
  });
});

function backendFiles() {
  backend.on("workspace_search_files", [
    { path: "src/button.tsx", is_dir: false },
    { path: "src/bun.ts", is_dir: false },
  ]);
}

describe("conversation map", () => {
  function longHistory(count: number) {
    const history: Message[] = [];
    for (let i = 0; i < count; i++) {
      history.push(msg("user", "text", { text: `question ${i}` }));
      history.push(msg("assistant", "text", { text: `answer ${i}` }));
    }
    return history;
  }

  it("is left out of a short conversation", async () => {
    await pane(
      open([msg("user", "text", { text: "hi" }), msg("assistant", "text", { text: "yo" })]),
    );
    await screen.findByText("hi");
    expect(screen.queryByRole("navigation", { name: "Conversation map" })).not.toBeInTheDocument();
  });

  it("has one tick for each ask, not for replies, and marks the last while following the chat", async () => {
    await pane(open(longHistory(6)));
    const map = await screen.findByRole("navigation", { name: "Conversation map" });
    const ticks = within(map).getAllByRole("button");
    expect(ticks).toHaveLength(6);
    expect(ticks[ticks.length - 1]).toHaveAttribute("aria-current", "location");
    expect(within(map).getAllByRole("button", { current: "location" })).toHaveLength(1);
  });

  it("scrolls to a message when its tick is clicked", async () => {
    const scrolled = vi.spyOn(Element.prototype, "scrollIntoView");
    const { app } = await pane(open(longHistory(6)));
    const map = await screen.findByRole("navigation", { name: "Conversation map" });
    await app.user.click(within(map).getByRole("button", { name: "Go to question 1" }));
    expect(scrolled).toHaveBeenCalled();
    scrolled.mockRestore();
  });

  it("is left out until there are two asks", async () => {
    await pane(
      open([
        msg("user", "text", { text: "only ask" }),
        msg("assistant", "text", { text: "a" }),
        msg("assistant", "text", { text: "b" }),
        msg("assistant", "text", { text: "c" }),
        msg("assistant", "text", { text: "d" }),
        msg("assistant", "text", { text: "e" }),
      ]),
    );
    await screen.findByText("only ask");
    expect(screen.queryByRole("navigation", { name: "Conversation map" })).not.toBeInTheDocument();
  });

  it("thins out the ticks in a very long conversation", async () => {
    await pane(open(longHistory(120)));
    const map = await screen.findByRole("navigation", { name: "Conversation map" });
    expect(within(map).getAllByRole("button").length).toBeLessThanOrEqual(70);
  });
});

describe("mention and command badges", () => {
  it("splits a message into plain text, files and commands", () => {
    expect(highlight("/review the diff in @src/a.ts and @b.ts please")).toEqual([
      { text: "/review", start: 0, kind: "command" },
      { text: " the diff in ", start: 7 },
      { text: "@src/a.ts", start: 20, kind: "mention" },
      { text: " and ", start: 29 },
      { text: "@b.ts", start: 34, kind: "mention" },
      { text: " please", start: 39 },
    ]);
  });

  it("ignores an @ inside a word and a slash that is not at the start", () => {
    expect(highlight("mail me@x.com or a/b and /not here")).toEqual([
      { text: "mail me@x.com or a/b and /not here", start: 0 },
    ]);
    expect(highlight("")).toEqual([]);
  });

  it("paints a badge behind what the user typed", async () => {
    const { app } = await pane(open());
    await app.user.type(box(), "/review @src/a.ts now");
    const marks = screen.getByTestId("composer-marks");
    expect(marks.querySelector('[data-kind="command"]')).toHaveTextContent("/review");
    expect(marks.querySelector('[data-kind="mention"]')).toHaveTextContent("@src/a.ts");
  });
});

describe("chat tabs", () => {
  it("opens a new chat, switches, renames and closes", async () => {
    const { app } = await pane(open());
    await app.user.click(screen.getByRole("button", { name: "New chat" }));
    await waitFor(() => expect(chatTabs()).toHaveLength(2));
    expect(backend.callsTo("session_create")[0]).toMatchObject({ workspaceId: "w1" });
    expect(chatTabs()[1]).toHaveAttribute("aria-selected", "true");

    await app.user.click(chatTabs()[0]);
    expect(chatTabs()[0]).toHaveAttribute("aria-selected", "true");

    await app.user.dblClick(chatTabs()[0]);
    const name = screen.getByLabelText("Chat name");
    await app.user.clear(name);
    await app.user.type(name, "Refactor{Enter}");
    expect(backend.callsTo("session_rename").slice(-1)[0]).toMatchObject({
      id: "s1",
      title: "Refactor",
    });

    await app.user.click(screen.getAllByRole("button", { name: /^Close/ })[0]);
    expect(backend.callsTo("session_close")).toEqual([{ id: "s1" }]);
  });

  it("reorders tabs by dragging", async () => {
    const { app } = await pane(
      open([], [session({ id: "s1" }), session({ id: "s2", title: "Second" })]),
    );
    await waitFor(() => expect(chatTabs()).toHaveLength(2));
    const tabs = chatTabs();
    fireEvent.dragStart(tabs[1]);
    fireEvent.dragOver(tabs[0]);
    fireEvent.drop(tabs[0]);
    await waitFor(() => expect(backend.callsTo("session_reorder")).toHaveLength(1));
    expect(backend.callsTo("session_reorder")[0]).toEqual({ workspaceId: "w1", ids: ["s2", "s1"] });
    expect(app.user).toBeDefined();
  });

  it("reopens a closed chat from the history", async () => {
    const closed = session({ id: "s9", title: "Old idea", status: "closed" });
    const { app } = await pane(open());
    backend.on("session_list", (args: Record<string, unknown>) =>
      args.includeClosed ? [session(), closed] : [session()],
    );
    await app.user.click(screen.getByRole("button", { name: "Chat history" }));
    await app.user.click(await screen.findByRole("button", { name: "Old idea" }));
    expect(backend.callsTo("session_reopen")).toEqual([{ id: "s9" }]);
  });

  it("shows working and blocked chats in their tabs", async () => {
    await pane(
      open(
        [],
        [
          session({ id: "s1", status: "working" }),
          session({ id: "s2", status: "blocked", title: "B" }),
        ],
      ),
    );
    expect(await screen.findByLabelText("Working")).toBeInTheDocument();
    expect(screen.getByLabelText("Needs approval")).toBeInTheDocument();
  });
});

describe("first visit", () => {
  it("starts a chat and sends the prompt written in the Create dialog", async () => {
    seq = 0;
    defaultBackend();
    chatBackend([]);
    backend.on("workspace_get", workspace({ id: "w1" })).on("workspace_take_pending_prompt", {
      prompt: "Fix the login bug",
      model: "sonnet",
      effort: "low",
    });
    renderApp("/workspace/w1");
    await waitFor(() => expect(backend.callsTo("session_send")).toHaveLength(1));
    expect(backend.callsTo("session_create")[0]).toMatchObject({
      options: { model: "sonnet", effort: "low" },
    });
    expect(backend.callsTo("session_send")[0]).toMatchObject({ text: "Fix the login bug" });
    expect(backend.callsTo("workspace_take_pending_prompt")).toHaveLength(1);
    await waitFor(() => expect(chatTabs().length).toBeGreaterThan(0));
  });

  it("starts an empty chat when there is no prompt", async () => {
    seq = 0;
    defaultBackend();
    chatBackend([]);
    backend.on("workspace_get", workspace({ id: "w1" }));
    renderApp("/workspace/w1");
    await waitFor(() => expect(chatTabs().length).toBeGreaterThan(0));
    expect(backend.callsTo("session_send")).toHaveLength(0);
    expect(backend.callsTo("session_create")).toHaveLength(1);
  });

  it("says so, with a way in, when a Composer made the workspace and started no agent in it", async () => {
    defaultBackend();
    chatBackend([]);
    backend
      .on("workspace_get", workspace({ id: "w1", city: "Florence" }))
      .on("composer_workspace_links", [
        {
          workspace_id: "w1",
          composer_id: "c1",
          composer_name: "Task splitter",
          session_id: "r1",
        },
      ]);
    const app = renderApp("/workspace/w1");
    expect(await screen.findByText("No agent has started here yet")).toBeInTheDocument();
    // The Composer's workspace is not given a chat of its own behind its back.
    expect(backend.callsTo("session_create")).toHaveLength(0);
    await app.user.click(screen.getByRole("button", { name: "Start a chat" }));
    await waitFor(() => expect(backend.callsTo("session_create")).toHaveLength(1));
  });

  it("does not start a chat in an archived workspace", async () => {
    defaultBackend();
    chatBackend([]);
    backend.on("workspace_get", workspace({ id: "w1", state: "archived" }));
    renderApp("/workspace/w1");
    await screen.findByRole("status");
    expect(backend.callsTo("session_create")).toHaveLength(0);
  });
});

describe("unread marker", () => {
  it("is shown in the sidebar and cleared when the workspace is open", async () => {
    useChat.getState().markUnread("w1");
    defaultBackend();
    chatBackend([]);
    backend.on("workspace_get", workspace({ id: "w1" }));
    renderApp("/");
    expect(await screen.findByLabelText("Unread")).toBeInTheDocument();
    renderApp("/workspace/w1");
    await waitFor(() => expect(useChat.getState().unread.w1).toBeUndefined());
  });
});
