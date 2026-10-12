import type { Checkpoint } from "@circles/bindings/Checkpoint";
import type { Message } from "@circles/bindings/Message";
import type { RevertPreview } from "@circles/bindings/RevertPreview";
import { act, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { useChat } from "@/stores/chat";
import { backend, emitBackendEvent } from "@/test/backend";
import { session, workspace } from "@/test/fixtures";
import { chatBackend, defaultBackend, renderApp } from "@/test/render";

const message = (seq: number, role: Message["role"], text: string): Message => ({
  id: `m${seq}`,
  session_id: "s1",
  seq,
  role,
  kind: "text",
  content: { text, steered: false },
  created_at: seq * 1000,
});

const history = [
  message(1, "user", "first ask"),
  message(2, "assistant", "first answer"),
  message(3, "user", "second ask"),
  message(4, "assistant", "second answer"),
  message(5, "user", "third ask"),
  message(6, "assistant", "third answer"),
];

const checkpoint = (turn: number, extra: Partial<Checkpoint> = {}): Checkpoint => ({
  id: `c${turn}`,
  workspace_id: "w1",
  session_id: "s1",
  turn,
  claude_session_id: null,
  message_seq: turn * 2 - 1,
  created_at: turn,
  skipped: [],
  ...extra,
});

const preview = (turn: number, extra: Partial<RevertPreview> = {}): RevertPreview => ({
  checkpoint: checkpoint(turn),
  files: [
    { path: "two.txt", kind: "removed" },
    { path: "one.txt", kind: "modified" },
    { path: "gone.txt", kind: "restored" },
  ],
  later_turns: 3 - turn,
  other_chats_active: 0,
  running: false,
  ...extra,
});

function open(setup: () => void = () => {}) {
  defaultBackend();
  chatBackend([session({ id: "s1" })]);
  backend
    .on("workspace_get", workspace({ id: "w1", city: "Algiers" }))
    .on("session_messages", history)
    .on("checkpoint_list", [checkpoint(1), checkpoint(2), checkpoint(3)])
    .on("checkpoint_preview", (args: Record<string, unknown>) =>
      preview(Number(String(args.checkpointId).slice(1))),
    )
    .on("checkpoint_revert", { revert_id: "r1", files: [] })
    .on("checkpoint_undo_revert", [{ path: "two.txt", kind: "restored" }])
    .on("session_branch", session({ id: "s2", title: "Chat (branch)" }));
  setup();
  return renderApp("/workspace/w1");
}

const reverts = () => screen.findAllByRole("button", { name: "Revert to before this turn" });

beforeEach(() => {
  useChat.setState(useChat.getInitialState(), true);
});

describe("revert", () => {
  it("offers to go back to before each turn that has a checkpoint", async () => {
    open();
    expect(await reverts()).toHaveLength(3);
  });

  it("offers nothing for a turn without a checkpoint", async () => {
    open(() => backend.on("checkpoint_list", [checkpoint(1), checkpoint(3)]));
    await screen.findByText("third ask");
    await waitFor(() =>
      expect(screen.getAllByRole("button", { name: "Revert to before this turn" })).toHaveLength(2),
    );
  });

  it("offers nothing when the message of a turn was never linked", async () => {
    open(() => backend.on("checkpoint_list", [checkpoint(1, { message_seq: null })]));
    await screen.findByText("first ask");
    await waitFor(() => expect(backend.callsTo("checkpoint_list")).toHaveLength(1));
    expect(
      screen.queryByRole("button", { name: "Revert to before this turn" }),
    ).not.toBeInTheDocument();
  });

  it("shows what would change and what is deleted before it asks to confirm", async () => {
    const app = open();
    await app.user.click((await reverts())[1]);
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("Revert to before turn 2?");
    const files = await within(dialog).findByRole("list", { name: "Files that would change" });
    expect(files).toHaveTextContent("two.txt");
    expect(files).toHaveTextContent("is removed");
    expect(files).toHaveTextContent("goes back to how it was");
    expect(files).toHaveTextContent("comes back");
    expect(dialog).toHaveTextContent("3 files would change");
    expect(dialog).toHaveTextContent("deletes turn 2 and the 1 later turn from the chat");
    expect(backend.callsTo("checkpoint_preview")).toEqual([{ checkpointId: "c2" }]);
    expect(backend.callsTo("checkpoint_revert")).toHaveLength(0);
  });

  it("goes back when confirmed, and offers to undo it", async () => {
    const app = open();
    await app.user.click((await reverts())[1]);
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findByRole("list", { name: "Files that would change" });
    await app.user.click(within(dialog).getByRole("button", { name: "Revert" }));
    await waitFor(() =>
      expect(backend.callsTo("checkpoint_revert")).toEqual([{ checkpointId: "c2" }]),
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());

    const toast = await screen.findByText("Went back to before turn 2");
    expect(toast).toBeInTheDocument();
    await app.user.click(await screen.findByRole("button", { name: "Undo" }));
    await waitFor(() =>
      expect(backend.callsTo("checkpoint_undo_revert")).toEqual([
        { revertId: "r1", sessionId: "s1" },
      ]),
    );
  });

  it("starts the chat over from what is stored when it was reverted", async () => {
    open();
    await screen.findByText("third ask");
    const loads = backend.callsTo("session_messages").length;
    backend.on("session_messages", history.slice(0, 2));
    act(() => emitBackendEvent("session:reverted", { session_id: "s1", workspace_id: "w1" }));
    await waitFor(() => expect(screen.queryByText("third ask")).not.toBeInTheDocument());
    expect(screen.getByText("first ask")).toBeInTheDocument();
    expect(backend.callsTo("session_messages").length).toBeGreaterThan(loads);
  });

  it("cannot revert while a turn runs, and says why", async () => {
    const app = open(() => backend.on("checkpoint_preview", preview(2, { running: true })));
    await app.user.click((await reverts())[1]);
    const dialog = await screen.findByRole("dialog");
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "A turn is running in this chat",
    );
    expect(within(dialog).getByRole("button", { name: "Revert" })).toBeDisabled();
  });

  it("warns when other chats of the workspace are working", async () => {
    const app = open(() => backend.on("checkpoint_preview", preview(2, { other_chats_active: 2 })));
    await app.user.click((await reverts())[1]);
    const dialog = await screen.findByRole("dialog");
    expect(await within(dialog).findByRole("status")).toHaveTextContent(
      "2 other chats of this workspace are working",
    );
    expect(within(dialog).getByRole("button", { name: "Revert" })).toBeEnabled();
  });

  it("warns about large files that could not be saved", async () => {
    const app = open(() =>
      backend.on(
        "checkpoint_preview",
        preview(2, { checkpoint: checkpoint(2, { skipped: ["huge.bin"] }) }),
      ),
    );
    await app.user.click((await reverts())[1]);
    const dialog = await screen.findByRole("dialog");
    expect(await within(dialog).findByText(/huge\.bin/)).toBeInTheDocument();
    expect(dialog).toHaveTextContent("too big to save");
  });

  it("says when no file would change", async () => {
    const app = open(() => backend.on("checkpoint_preview", preview(3, { files: [] })));
    await app.user.click((await reverts())[2]);
    expect(await screen.findByText("No file would change.")).toBeInTheDocument();
  });

  it("shows why a revert failed and keeps the dialog open", async () => {
    const app = open(() =>
      backend.fail("checkpoint_revert", {
        code: "git",
        message: "The working tree could not be restored.",
      }),
    );
    await app.user.click((await reverts())[1]);
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findByRole("list", { name: "Files that would change" });
    await app.user.click(within(dialog).getByRole("button", { name: "Revert" }));
    expect(
      await within(dialog).findByText("The working tree could not be restored."),
    ).toBeInTheDocument();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("can be cancelled without changing anything", async () => {
    const app = open();
    await app.user.click((await reverts())[0]);
    const dialog = await screen.findByRole("dialog");
    await app.user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(backend.callsTo("checkpoint_revert")).toHaveLength(0);
  });
});

describe("undo", () => {
  const undoable = { id: "r9", turn: 2, created_at: 1, expires_at: 2 };

  it("shows a banner after a revert, and undoes it", async () => {
    const app = open(() => backend.on("checkpoint_last_revert", undoable));
    const banner = await screen.findByText(
      /Went back to before turn 2\. Everything after it is hidden\./,
    );
    expect(banner).toBeInTheDocument();
    await app.user.click(screen.getByRole("button", { name: "Undo revert" }));
    await waitFor(() =>
      expect(backend.callsTo("checkpoint_undo_revert")).toEqual([
        { revertId: "r9", sessionId: "s1" },
      ]),
    );
  });

  it("shows nothing when there is nothing to undo", async () => {
    open();
    await screen.findByText("third ask");
    expect(screen.queryByRole("button", { name: "Undo revert" })).not.toBeInTheDocument();
  });

  it("says so when it is too late", async () => {
    const app = open(() => {
      backend.on("checkpoint_last_revert", undoable);
      backend.fail("checkpoint_undo_revert", {
        code: "invalid",
        message: "It is too late to undo that revert.",
      });
    });
    await app.user.click(await screen.findByRole("button", { name: "Undo revert" }));
    expect(await screen.findByText("It is too late to undo that revert.")).toBeInTheDocument();
  });
});

describe("branching", () => {
  it("starts a new chat from the end of a turn and opens it", async () => {
    const app = open();
    const buttons = await screen.findAllByRole("button", {
      name: "Branch the conversation from here",
    });
    expect(buttons.length).toBeGreaterThanOrEqual(2);
    await app.user.click(buttons[0]);
    await waitFor(() =>
      expect(backend.callsTo("session_branch")).toEqual([{ sessionId: "s1", turn: 1 }]),
    );
    await waitFor(() => expect(useChat.getState().activeTab.w1).toBe("s2"));
  });

  it("is not offered for a turn without a checkpoint", async () => {
    open(() => backend.on("checkpoint_list", []));
    await screen.findByText("third ask");
    await waitFor(() => expect(backend.callsTo("checkpoint_list")).toHaveLength(1));
    expect(
      screen.queryByRole("button", { name: "Branch the conversation from here" }),
    ).not.toBeInTheDocument();
  });
});
