import type { SetupItem } from "@circles/bindings/SetupItem";
import { screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { backend } from "@/test/backend";
import { defaultBackend, renderApp } from "@/test/render";
import { useOnboarding } from "./store";

function row(over: Partial<SetupItem> & { id: string }): SetupItem {
  return {
    title: over.id,
    why: "Because.",
    state: "ready",
    required: false,
    detail: null,
    fix: null,
    action: null,
    ...over,
  };
}

const READY: SetupItem[] = [
  row({ id: "git", title: "git", required: true, detail: "git version 2.45" }),
  row({ id: "claude_installed", title: "Claude Code", required: true, detail: "2.1.0" }),
  row({ id: "claude_signed_in", title: "Signed in to Claude", required: true }),
  row({
    id: "gh",
    title: "GitHub CLI",
    state: "optional",
    detail: "The GitHub CLI is not signed in.",
    fix: "gh auth login",
  }),
  row({
    id: "notifications",
    title: "Notifications",
    state: "unknown",
    action: { id: "test_notification", label: "Send a test" },
  }),
  row({
    id: "folders",
    title: "Folders where your code lives",
    state: "unknown",
    action: { id: "probe_folders", label: "Allow access" },
  }),
];

describe("first run", () => {
  beforeEach(() => {
    defaultBackend();
    backend.on("setup_status", READY).on("diagnostics_report", "report");
    useOnboarding.setState({ done: false });
  });

  it("is skipped when projects already exist", async () => {
    renderApp("/");
    await waitFor(() => expect(useOnboarding.getState().done).toBe(true));
    await waitFor(() => expect(screen.queryByText("Welcome to Circles")).not.toBeInTheDocument());
  });

  it("shows what is needed and what is nice to have, and needs the notice acknowledged", async () => {
    backend.on("project_list", []);
    const app = renderApp("/");
    expect(await screen.findByText("Welcome to Circles")).toBeInTheDocument();
    const needed = await screen.findByRole("region", { name: "What Circles needs" });
    expect(within(needed).getByText("Claude Code")).toBeInTheDocument();
    const nice = screen.getByRole("region", { name: "Nice to have" });
    expect(within(nice).getByText("gh auth login")).toBeInTheDocument();
    expect(within(nice).getByText("Notifications")).toBeInTheDocument();

    const add = screen.getByRole("button", { name: "Add your first project" });
    expect(add).toBeDisabled();
    await app.user.click(screen.getByLabelText("I understand"));
    expect(add).toBeEnabled();
    await app.user.click(add);
    await waitFor(() => expect(useOnboarding.getState().done).toBe(true));
  });

  it("holds back until Claude Code is installed and signed in", async () => {
    backend.on("project_list", []);
    backend.on("setup_status", [
      READY[0],
      row({
        id: "claude_installed",
        title: "Claude Code",
        required: true,
        state: "needs_you",
        detail: "`claude` was not found.",
        fix: "curl -fsSL https://claude.ai/install.sh | bash",
      }),
      row({
        id: "claude_signed_in",
        title: "Signed in to Claude",
        required: true,
        state: "needs_you",
        detail: "Install Claude Code first.",
      }),
    ]);
    const app = renderApp("/");
    await app.user.click(await screen.findByLabelText("I understand"));
    expect(screen.getByRole("button", { name: "Add your first project" })).toBeDisabled();
    expect(screen.getByText(/claude\.ai\/install\.sh/)).toBeInTheDocument();
    expect(screen.getByText("2 things are still needed.")).toBeInTheDocument();
  });

  it("offers the install for git, and checks again when asked", async () => {
    backend.on("project_list", []);
    backend.on("setup_status", [
      row({
        id: "git",
        title: "git",
        required: true,
        state: "needs_you",
        detail: "git was not found.",
        fix: "xcode-select --install",
        action: { id: "install_git", label: "Install" },
      }),
      READY[1],
      READY[2],
    ]);
    backend.on("setup_action", {
      ok: true,
      message: "macOS opened its installer. Finish it, then check again.",
    });
    const app = renderApp("/");
    const git = await screen.findByTestId("setup-git");
    expect(within(git).getByText("xcode-select --install")).toBeInTheDocument();
    backend.on("setup_status", READY);
    await app.user.click(within(git).getByRole("button", { name: "Install" }));
    await waitFor(() => expect(backend.callsTo("setup_action")).toEqual([{ id: "install_git" }]));
    expect(await within(git).findByRole("status")).toHaveTextContent("macOS opened its installer");
    // After the action the list is read again.
    await waitFor(() => expect(screen.getByTestId("setup-git")).toHaveTextContent("git version"));
  });

  it("runs a permission test and shows what it found", async () => {
    backend.on("project_list", []);
    backend.on("setup_action", {
      ok: false,
      message: "Desktop: allowed. Documents: not allowed. Downloads: allowed.",
    });
    const app = renderApp("/");
    const folders = await screen.findByTestId("setup-folders");
    await app.user.click(within(folders).getByRole("button", { name: "Allow access" }));
    expect(await within(folders).findByRole("status")).toHaveTextContent("Documents: not allowed");
    expect(backend.callsTo("setup_action")).toEqual([{ id: "probe_folders" }]);
  });

  it("shows a failed action as a message and does not break", async () => {
    backend.on("project_list", []);
    backend.fail("setup_action", { code: "internal", message: "no terminal" });
    const app = renderApp("/");
    const note = await screen.findByTestId("setup-notifications");
    await app.user.click(within(note).getByRole("button", { name: "Send a test" }));
    expect(await within(note).findByRole("status")).toHaveTextContent("no terminal");
  });

  it("can be shown again from the settings", async () => {
    const app = renderApp("/settings/advanced");
    await waitFor(() => expect(useOnboarding.getState().done).toBe(true));
    await app.user.click(await screen.findByRole("button", { name: "Show again" }));
    expect(await screen.findByText("Welcome to Circles")).toBeInTheDocument();
  });
});
