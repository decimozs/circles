import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { backend } from "@/test/backend";
import { workspace } from "@/test/fixtures";
import { defaultBackend, renderApp } from "@/test/render";

function seed(overrides = {}) {
  defaultBackend();
  const ws = workspace({ id: "w1", city: "Algiers", ...overrides });
  backend.on("workspace_get", ws).on("workspace_list", [ws]);
  return ws;
}

async function openMenu(user: ReturnType<typeof renderApp>["user"]) {
  await user.click(await screen.findByRole("button", { name: "Actions for Algiers" }));
}

describe("Workspace page", () => {
  it("shows the workspace details and the lines that start the chat", async () => {
    seed();
    backend.on("workspace_events", [
      {
        id: "1",
        workspace_id: "w1",
        kind: "created",
        message: "You're in a new copy of roster called algiers",
        created_at: 1,
      },
      {
        id: "2",
        workspace_id: "w1",
        kind: "info",
        message: "Branched me/abc12 from origin/main",
        created_at: 2,
      },
      {
        id: "3",
        workspace_id: "w1",
        kind: "warning",
        message: "Could not update from origin",
        created_at: 3,
      },
    ]);
    const app = renderApp("/workspace/w1");
    expect(await screen.findByRole("heading", { name: "Algiers" })).toBeInTheDocument();
    expect(
      await screen.findByText("You're in a new copy of roster called algiers"),
    ).toBeInTheDocument();
    expect(screen.getByText("Branched me/abc12 from origin/main")).toBeInTheDocument();
    await openMenu(app.user);
    await app.user.click(await screen.findByRole("menuitem", { name: "Workspace details" }));
    const details = await screen.findByRole("dialog");
    expect(within(details).getByText("me/abc12")).toBeInTheDocument();
    expect(within(details).getByText("roster")).toBeInTheDocument();
    expect(backend.callsTo("workspace_open")).toEqual([{ id: "w1" }]);
  });

  it("copies the pull request link, or is off when there is none", async () => {
    seed();
    const app = renderApp("/workspace/w1");
    await openMenu(app.user);
    expect(
      await screen.findByRole("menuitem", {
        name: /Copy pull request link \(no pull request yet\)/,
      }),
    ).toHaveAttribute("aria-disabled", "true");
  });

  it("copies the pull request link when the workspace has one", async () => {
    seed({ pr_number: 7, pr_url: "https://github.com/a/b/pull/7" });
    const app = renderApp("/workspace/w1");
    // user-event installs its own clipboard when the app is rendered, so spy on that one.
    const writeText = vi.spyOn(navigator.clipboard, "writeText");
    await openMenu(app.user);
    await app.user.click(await screen.findByRole("menuitem", { name: "Copy pull request link" }));
    expect(writeText).toHaveBeenCalledWith("https://github.com/a/b/pull/7");
  });

  it("prefers the title over the city", async () => {
    seed({ title: "#12 Fix login" });
    renderApp("/workspace/w1");
    expect(await screen.findByRole("heading", { name: "#12 Fix login" })).toBeInTheDocument();
  });

  it("archives a clean workspace", async () => {
    seed();
    backend.on("workspace_archive", workspace({ id: "w1", state: "archived" }));
    const { user } = renderApp("/workspace/w1");
    await openMenu(user);
    await user.click(await screen.findByRole("menuitem", { name: "Archive" }));
    await waitFor(() =>
      expect(backend.callsTo("workspace_archive")).toEqual([{ id: "w1", commitDirty: false }]),
    );
  });

  it("asks before archiving uncommitted work and commits it on request", async () => {
    seed();
    backend.on("workspace_archive", (args: Record<string, unknown>) => {
      if (!args.commitDirty) {
        throw {
          code: "git",
          message: "Algiers has 1 uncommitted change. Commit them, or archive with auto-commit.",
          details: { kind: "dirty_tree", path: "/p" },
        };
      }
      return workspace({ id: "w1", state: "archived" });
    });
    const { user } = renderApp("/workspace/w1");
    await openMenu(user);
    await user.click(await screen.findByRole("menuitem", { name: "Archive" }));

    const dialog = await screen.findByRole("dialog", { name: "Uncommitted changes" });
    expect(within(dialog).getByText(/1 uncommitted change/)).toBeInTheDocument();
    expect(within(dialog).getByText(/WIP: archived from Circles/)).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Commit and archive" }));
    await waitFor(() =>
      expect(backend.callsTo("workspace_archive")).toEqual([
        { id: "w1", commitDirty: false },
        { id: "w1", commitDirty: true },
      ]),
    );
  });

  it("can cancel the commit prompt without archiving", async () => {
    seed();
    backend.fail("workspace_archive", {
      code: "git",
      message: "dirty",
      details: { kind: "dirty_tree" },
    });
    const { user } = renderApp("/workspace/w1");
    await openMenu(user);
    await user.click(await screen.findByRole("menuitem", { name: "Archive" }));
    const dialog = await screen.findByRole("dialog", { name: "Uncommitted changes" });
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(backend.callsTo("workspace_archive")).toHaveLength(1);
  });

  it("renames the branch and shows a problem inline", async () => {
    seed();
    let attempts = 0;
    backend.on("workspace_rename_branch", (args: Record<string, unknown>) => {
      attempts += 1;
      if (attempts === 1)
        throw {
          code: "invalid",
          message: '"bad..name" is not a valid branch name.',
          details: null,
        };
      return workspace({ id: "w1", branch: String(args.name) });
    });
    const { user } = renderApp("/workspace/w1");
    await openMenu(user);
    await user.click(await screen.findByRole("menuitem", { name: "Rename branch" }));
    const input = await screen.findByLabelText("Branch name");
    expect(input).toHaveValue("me/abc12");

    await user.clear(input);
    await user.type(input, "bad..name");
    await user.click(screen.getByRole("button", { name: "Rename" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("not a valid branch name");

    await user.clear(input);
    await user.type(input, "me/fix-login");
    await user.click(screen.getByRole("button", { name: "Rename" }));
    await waitFor(() =>
      expect(backend.callsTo("workspace_rename_branch").pop()).toEqual({
        id: "w1",
        name: "me/fix-login",
      }),
    );
    await waitFor(() => expect(screen.queryByLabelText("Branch name")).not.toBeInTheDocument());
  });

  it("pins and unpins", async () => {
    seed();
    backend.on("workspace_pin", workspace({ id: "w1", pinned: true }));
    const { user } = renderApp("/workspace/w1");
    await openMenu(user);
    await user.click(await screen.findByRole("menuitem", { name: "Pin" }));
    await waitFor(() =>
      expect(backend.callsTo("workspace_pin")).toEqual([{ id: "w1", pinned: true }]),
    );
  });

  it("shows the workspace name and state in the top bar, without the project", async () => {
    seed();
    renderApp("/workspace/w1");
    const heading = await screen.findByRole("heading", { name: "Algiers" });
    expect(heading).toBeInTheDocument();
    expect(screen.queryByText("Ready")).not.toBeInTheDocument();
    expect(screen.getByRole("banner")).not.toHaveTextContent("roster");
  });

  it("keeps the top bar icon still and shows no state text while the workspace is working", async () => {
    seed({ state: "working" });
    renderApp("/workspace/w1");
    await screen.findByRole("heading", { name: "Algiers" });
    expect(screen.queryByText("Working")).not.toBeInTheDocument();
    const banner = screen.getByRole("banner");
    expect(banner.querySelector(".animate-spin")).toBeNull();
  });

  it("has the actions menu on the sidebar row too", async () => {
    seed();
    backend.on("workspace_pin", workspace({ id: "w1", pinned: true }));
    const { user } = renderApp("/");
    await user.click(await screen.findByRole("button", { name: "Menu for Algiers" }));
    await user.click(await screen.findByRole("menuitem", { name: "Pin" }));
    await waitFor(() =>
      expect(backend.callsTo("workspace_pin")).toEqual([{ id: "w1", pinned: true }]),
    );
  });

  it("offers Restore for an archived workspace and no Archive", async () => {
    seed({ state: "archived" });
    backend.on("workspace_restore", workspace({ id: "w1" }));
    const { user } = renderApp("/workspace/w1");
    expect(
      await screen.findByText(/Archived\. The branch me\/abc12 was kept\./),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Restore" }));
    await waitFor(() => expect(backend.callsTo("workspace_restore")).toEqual([{ id: "w1" }]));

    await openMenu(user);
    expect(await screen.findByRole("menuitem", { name: "Restore" })).toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: "Archive" })).not.toBeInTheDocument();
  });

  it("shows why an errored workspace failed", async () => {
    seed({ state: "errored", error: "The worktree folder is missing." });
    renderApp("/workspace/w1");
    expect(await screen.findByText("The worktree folder is missing.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Restore" })).toBeInTheDocument();
  });

  it("deletes an archived workspace, optionally with its branch, and goes home", async () => {
    seed({ state: "archived" });
    backend.on("workspace_delete", null);
    const { user, router } = renderApp("/workspace/w1");
    await openMenu(user);
    await user.click(await screen.findByRole("menuitem", { name: "Delete" }));
    const dialog = await screen.findByRole("dialog", { name: /Delete Algiers/ });
    await user.click(within(dialog).getByRole("switch"));
    await user.click(within(dialog).getByRole("button", { name: "Delete" }));
    await waitFor(() =>
      expect(backend.callsTo("workspace_delete")).toEqual([{ id: "w1", deleteBranch: true }]),
    );
    await waitFor(() => expect(router.state.location.pathname).toBe("/"));
  });

  it("cannot delete a live workspace", async () => {
    seed();
    const { user } = renderApp("/workspace/w1");
    await openMenu(user);
    expect(await screen.findByRole("menuitem", { name: "Archive" })).toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: "Delete" })).not.toBeInTheDocument();
  });

  it("leaves a workspace that no longer exists for Home, which offers to add a project", async () => {
    defaultBackend();
    backend.on("project_list", []).on("workspace_list", []);
    backend.fail("workspace_get", { code: "not_found", message: "not found: workspace nope" });
    renderApp("/workspace/nope");
    expect(await screen.findByText("Add your first project")).toBeInTheDocument();
    expect(screen.queryByText(/not found: workspace/)).not.toBeInTheDocument();
  });

  it("reports a workspace that cannot be read for another reason", async () => {
    defaultBackend();
    backend.fail("workspace_get", { code: "internal", message: "disk is gone" });
    renderApp("/workspace/nope");
    expect(await screen.findByRole("alert")).toHaveTextContent("disk is gone");
  });
});
