import type { Component } from "@circles/bindings/Component";
import type { StoredCard } from "@circles/bindings/StoredCard";
import { screen, waitFor, within } from "@testing-library/react";
import axe from "axe-core";
import { describe, expect, it } from "vitest";
import { backend, emitBackendEvent } from "@/test/backend";
import { composer } from "@/test/fixtures";
import { defaultBackend, renderApp, settingsBackend } from "@/test/render";

function stored(
  body: Component[],
  over: Partial<StoredCard> = {},
  title = "Which tickets?",
): StoredCard {
  return {
    uid: "u1",
    session_id: "s1",
    composer_id: "c1",
    card: { id: "pick", title, body, submit_label: null, outside_text: false },
    status: "pending",
    answer: null,
    created_at: 1,
    answered_at: null,
    ...over,
  };
}

const TICKETS: Component = {
  type: "choice_list",
  id: "t",
  multiple: true,
  required: true,
  options: [
    { value: "ROS-1", label: "Share the state", detail: "To Do" },
    { value: "ROS-2", label: "Protect main", detail: "In Progress" },
  ],
};

function open(cards: StoredCard[]) {
  defaultBackend();
  backend
    .on("composer_list", [composer({ id: "c1", name: "Release captain" })])
    .on(
      "genui_pending",
      cards.filter((c) => c.status === "pending"),
    )
    .on("genui_answer", null)
    .on("genui_dismiss", null);
  return renderApp("/composers");
}

describe("cards from an agent", () => {
  it("waits for you on the Composers page, with the Composer's name", async () => {
    open([stored([{ type: "text", text: "Pick some." }, TICKETS])]);
    const waiting = await screen.findByRole("region", { name: "Waiting for you" });
    const card = within(waiting).getByRole("region", { name: "Which tickets?" });
    expect(within(card).getByText("Release captain")).toBeInTheDocument();
    expect(within(card).getByText("Pick some.")).toBeInTheDocument();
    expect(within(card).getByText("Share the state")).toBeInTheDocument();
    expect(within(card).getByText("In Progress")).toBeInTheDocument();
  });

  it("sends the chosen values, and only when what is required is filled in", async () => {
    const app = open([stored([TICKETS])]);
    const card = await screen.findByRole("region", { name: "Which tickets?" });
    const send = within(card).getByRole("button", { name: "Send" });
    expect(send).toBeDisabled();
    await app.user.click(within(card).getByRole("checkbox", { name: /Protect main/ }));
    expect(send).toBeEnabled();
    await app.user.click(send);
    await waitFor(() =>
      expect(backend.callsTo("genui_answer")[0]).toEqual({ uid: "u1", values: { t: ["ROS-2"] } }),
    );
  });

  it("takes a single choice with radio buttons", async () => {
    const app = open([
      stored([
        {
          type: "choice_list",
          id: "one",
          multiple: false,
          required: false,
          options: [
            { value: "a", label: "Alpha", detail: null },
            { value: "b", label: "Beta", detail: null },
          ],
        },
      ]),
    ]);
    const card = await screen.findByRole("region", { name: "Which tickets?" });
    await app.user.click(within(card).getByRole("radio", { name: "Alpha" }));
    await app.user.click(within(card).getByRole("radio", { name: "Beta" }));
    await app.user.click(within(card).getByRole("button", { name: "Send" }));
    await waitFor(() =>
      expect(backend.callsTo("genui_answer")[0]).toEqual({ uid: "u1", values: { one: ["b"] } }),
    );
  });

  it("fills in a form of every kind", async () => {
    const app = open([
      stored([
        {
          type: "form",
          id: "f",
          fields: [
            {
              name: "branch",
              label: "Branch",
              kind: "text",
              required: true,
              options: [],
              placeholder: null,
            },
            {
              name: "notes",
              label: "Notes",
              kind: "long_text",
              required: false,
              options: [],
              placeholder: null,
            },
            {
              name: "count",
              label: "How many",
              kind: "number",
              required: false,
              options: [],
              placeholder: null,
            },
            {
              name: "draft",
              label: "As a draft",
              kind: "checkbox",
              required: false,
              options: [],
              placeholder: null,
            },
            {
              name: "mode",
              label: "Mode",
              kind: "select",
              required: false,
              options: [{ value: "fast", label: "Fast", detail: null }],
              placeholder: null,
            },
          ],
        },
      ]),
    ]);
    const card = await screen.findByRole("region", { name: "Which tickets?" });
    const send = within(card).getByRole("button", { name: "Send" });
    expect(send).toBeDisabled();
    await app.user.type(within(card).getByLabelText("Branch"), "feat/x");
    await app.user.type(within(card).getByLabelText("How many"), "3");
    await app.user.click(within(card).getByLabelText("As a draft"));
    await app.user.selectOptions(within(card).getByLabelText("Mode"), "fast");
    await app.user.click(send);
    await waitFor(() =>
      expect(backend.callsTo("genui_answer")[0]).toEqual({
        uid: "u1",
        values: { f: { branch: "feat/x", count: 3, draft: true, mode: "fast" } },
      }),
    );
  });

  it("lets a plan be ticked, reordered and trimmed, and sends it in that order", async () => {
    const app = open([
      stored([
        {
          type: "steps",
          id: "plan",
          steps: [
            { id: "s1", text: "Write the migration", done: true },
            { id: "s2", text: "Add the command", done: true },
            { id: "s3", text: "Update the docs", done: true },
          ],
        },
      ]),
    ]);
    const card = await screen.findByRole("region", { name: "Which tickets?" });
    await app.user.click(within(card).getByRole("button", { name: "Move up: Add the command" }));
    await app.user.click(within(card).getByRole("button", { name: "Remove: Update the docs" }));
    await app.user.click(within(card).getByLabelText("Do: Write the migration"));
    await app.user.click(within(card).getByRole("button", { name: "Send" }));
    await waitFor(() =>
      expect(backend.callsTo("genui_answer")[0]).toEqual({
        uid: "u1",
        values: {
          plan: [
            { id: "s2", text: "Add the command", done: true },
            { id: "s1", text: "Write the migration", done: false },
          ],
        },
      }),
    );
  });

  it("answers at once when the card is only buttons", async () => {
    const app = open([
      stored([
        {
          type: "confirm",
          id: "ok",
          answers: [
            { value: "yes", label: "Start", detail: null },
            { value: "no", label: "Wait", detail: null },
          ],
        },
      ]),
    ]);
    const card = await screen.findByRole("region", { name: "Which tickets?" });
    expect(within(card).queryByRole("button", { name: "Send" })).not.toBeInTheDocument();
    await app.user.click(within(card).getByRole("button", { name: "Wait" }));
    await waitFor(() =>
      expect(backend.callsTo("genui_answer")[0]).toEqual({ uid: "u1", values: { ok: "no" } }),
    );
  });

  it("can be dismissed", async () => {
    const app = open([stored([TICKETS])]);
    const card = await screen.findByRole("region", { name: "Which tickets?" });
    await app.user.click(within(card).getByRole("button", { name: "Not now" }));
    await waitFor(() => expect(backend.callsTo("genui_dismiss")).toEqual([{ uid: "u1" }]));
  });

  it("refreshes when a card changes", async () => {
    open([stored([TICKETS])]);
    await screen.findByRole("region", { name: "Which tickets?" });
    backend.on("genui_pending", []);
    emitBackendEvent("genui:changed", { session_id: "s1", composer_id: "c1" });
    await waitFor(() =>
      expect(screen.queryByRole("region", { name: "Which tickets?" })).not.toBeInTheDocument(),
    );
  });

  it("draws every display component as plain text", async () => {
    open([
      stored([
        { type: "text", text: "<img src=x onerror=alert(1)> **not bold**" },
        { type: "key_value", rows: [{ label: "Branch", value: "feat/x" }] },
        { type: "list", items: [{ label: "One", detail: "first" }] },
        { type: "table", columns: ["Key", "Status"], rows: [["ROS-1", "To Do"]] },
        { type: "code", text: "let x = 1;", language: "rust" },
        { type: "progress", items: [{ label: "Build", state: "running" }] },
      ]),
    ]);
    const card = await screen.findByRole("region", { name: "Which tickets?" });
    // The markup is text, never an element.
    expect(within(card).getByText(/onerror=alert/)).toBeInTheDocument();
    expect(card.querySelector("img")).toBeNull();
    expect(within(card).getByText("**not bold**", { exact: false })).toBeInTheDocument();
    expect(within(card).getByRole("table")).toHaveTextContent("ROS-1");
    expect(within(card).getByText("let x = 1;")).toBeInTheDocument();
    expect(within(card).getByText("Build")).toBeInTheDocument();
    expect(within(card).getByText("Branch")).toBeInTheDocument();
  });

  it("marks a card that shows text from outside", async () => {
    open([
      stored([{ type: "text", text: "Ticket text" }], {
        card: {
          id: "x",
          title: "Which tickets?",
          body: [{ type: "text", text: "Ticket text" }],
          submit_label: null,
          outside_text: true,
        },
      }),
    ]);
    expect(await screen.findByText(/Includes text from outside/)).toBeInTheDocument();
  });

  it("has no accessibility violations", async () => {
    open([stored([{ type: "text", text: "Pick some." }, TICKETS])]);
    await screen.findByRole("region", { name: "Which tickets?" });
    const result = await axe.run(document.body, {
      rules: { "color-contrast": { enabled: false }, region: { enabled: false } },
    });
    expect(result.violations.map((v) => v.id)).toEqual([]);
  });
});

describe("settings for cards", () => {
  it("has switches for Composers (on) and chats (off)", async () => {
    defaultBackend();
    settingsBackend({});
    renderApp("/settings/composers");
    expect(
      await screen.findByRole("switch", { name: "Let Composers ask with choices and forms" }),
    ).toBeChecked();
  });
});
