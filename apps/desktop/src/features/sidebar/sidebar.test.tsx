import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useUi } from "@/stores/ui";
import { backend } from "@/test/backend";
import { HOUR, NOW, project, workspace } from "@/test/fixtures";
import { defaultBackend, renderApp } from "@/test/render";

function seed() {
  defaultBackend();
  backend
    .on("project_list", [
      project({ workspace_count: 2 }),
      project({ id: "p2", name: "portfolio", workspace_count: 0 }),
    ])
    .on("section_list", [{ id: "s1", name: "Side projects", position: 0 }])
    .on("workspace_list", [
      workspace({ id: "a", city: "Algiers" }),
      workspace({ id: "b", city: "Lima", title: "Fix login", state: "working" }),
      workspace({ id: "c", city: "Oslo", pinned: true, updated_at: NOW - 3 * HOUR }),
      workspace({ id: "d", city: "Kyoto", section_id: "s1" }),
      workspace({ id: "e", city: "Nice", state: "errored" }),
      workspace({ id: "f", city: "Gone", state: "archived" }),
    ]);
}

const sidebar = () =>
  screen.getByRole("navigation", { name: "Primary" }).parentElement as HTMLElement;

describe("Sidebar", () => {
  it("shows when each workspace was last used, in minutes, hours or days", async () => {
    defaultBackend();
    backend.on("workspace_list", [
      workspace({ id: "a", city: "Algiers", last_opened_at: Date.now() - 3 * 60_000 - 5_000 }),
      workspace({ id: "b", city: "Lima", last_opened_at: null, updated_at: Date.now() - 3 * HOUR }),
    ]);
    renderApp("/");
    const algiers = await screen.findByRole("link", { name: /Algiers/ });
    expect(within(algiers).getByTitle("Last used")).toHaveTextContent("3min");
    const lima = await screen.findByRole("link", { name: /Lima/ });
    expect(within(lima).getByTitle("Last used")).toHaveTextContent("3h");
  });

  it("shows the machine name and the real projects with their workspace counts", async () => {
    seed();
    renderApp("/");
    expect(await within(sidebar()).findByText("Test Mac")).toBeInTheDocument();
    const mine = await screen.findByRole("region", { name: "My workspaces" });
    expect(await within(mine).findByRole("button", { name: /^roster/ })).toHaveTextContent("2");
    expect(within(mine).getByRole("button", { name: /^portfolio/ })).toHaveTextContent("0");
  });

  it("invites you to add a repository, instead of a heading, when there are no projects", async () => {
    defaultBackend();
    backend.on("project_list", []).on("workspace_list", []);
    renderApp("/");
    const area = await screen.findByRole("region", { name: "My workspaces" });
    expect(
      await within(area).findByRole("heading", { name: "Add a repository to get started." }),
    ).toBeInTheDocument();
    expect(within(area).getByRole("button", { name: "Add project" })).toBeInTheDocument();
    // The heading "My workspaces" is not there with nothing to put under it.
    expect(within(area).queryByRole("heading", { name: "My workspaces" })).not.toBeInTheDocument();
  });

  it("lists workspaces under their project, by title when there is one, without archived ones", async () => {
    seed();
    renderApp("/");
    const mine = await screen.findByRole("region", { name: "My workspaces" });
    expect(await within(mine).findByRole("link", { name: /Algiers/ })).toBeInTheDocument();
    expect(within(mine).getByRole("link", { name: /Fix login/ })).toBeInTheDocument();
    expect(within(mine).queryByText("Gone")).not.toBeInTheDocument();
    expect(within(mine).queryByText("Lima")).not.toBeInTheDocument();
  });

  it("moves pinned workspaces to Pinned, with their age, and sectioned ones to their section", async () => {
    seed();
    renderApp("/");
    const pinned = await screen.findByRole("region", { name: "Pinned" });
    expect(within(pinned).getByRole("link", { name: /Oslo/ })).toHaveTextContent("3h");
    const mine = screen.getByRole("region", { name: "My workspaces" });
    expect(within(mine).queryByText("Oslo")).not.toBeInTheDocument();

    const section = screen.getByRole("region", { name: "Side projects" });
    expect(within(section).getByRole("link", { name: /Kyoto/ })).toBeInTheDocument();
    expect(within(mine).queryByText("Kyoto")).not.toBeInTheDocument();
  });

  it("marks working and errored workspaces", async () => {
    seed();
    renderApp("/");
    const mine = await screen.findByRole("region", { name: "My workspaces" });
    await within(mine).findByRole("link", { name: /Fix login/ });
    expect(
      within(within(mine).getByRole("link", { name: /Fix login/ })).getByLabelText("Working"),
    ).toBeInTheDocument();
    expect(
      within(within(mine).getByRole("link", { name: /Nice/ })).getByLabelText("Error"),
    ).toBeInTheDocument();
    expect(
      within(within(mine).getByRole("link", { name: /Algiers/ })).getByLabelText("Ready"),
    ).toBeInTheDocument();
  });

  it("shows where each branch stands in its icon: open, draft, merged, closed, commits", async () => {
    defaultBackend();
    const state = (id: string, over: object = {}) => ({
      workspace_id: id,
      pr_number: null,
      pr_state: null,
      is_draft: false,
      ahead: 0,
      ...over,
    });
    backend
      .on("workspace_list", [
        workspace({ id: "a", city: "Algiers" }),
        workspace({ id: "b", city: "Lima" }),
        workspace({ id: "c", city: "Oslo" }),
        workspace({ id: "d", city: "Kyoto" }),
        workspace({ id: "e", city: "Nice" }),
        workspace({ id: "f", city: "Rome" }),
      ])
      .on("workspace_branch_states", [
        state("a", { pr_number: 1, pr_state: "open" }),
        state("b", { pr_number: 2, pr_state: "open", is_draft: true }),
        state("c", { pr_number: 3, pr_state: "merged" }),
        state("d", { pr_number: 4, pr_state: "closed" }),
        state("e", { ahead: 3 }),
        state("f"),
      ]);
    renderApp("/");
    const icon = async (city: RegExp, label: string) => {
      const row = await screen.findByRole("link", { name: city });
      await waitFor(() => expect(within(row).getByLabelText(label)).toBeInTheDocument());
      return within(row).getByLabelText(label);
    };
    expect((await icon(/Algiers/, "Open")).getAttribute("class")).toMatch(/text-success/);
    expect((await icon(/Lima/, "Draft")).getAttribute("class")).toMatch(/text-muted-foreground/);
    expect((await icon(/Oslo/, "Merged")).getAttribute("class")).toMatch(/text-info/);
    expect((await icon(/Kyoto/, "Closed")).getAttribute("class")).toMatch(/text-destructive/);
    expect((await icon(/Nice/, "3 commits")).getAttribute("class")).toMatch(/text-violet/);
    // No pull request and no commits: the plain icon, named by the workspace state.
    expect((await icon(/Rome/, "Ready")).getAttribute("class")).toMatch(/text-muted-foreground/);
  });

  it("opens a workspace from its row", async () => {
    seed();
    const { user, router } = renderApp("/");
    await user.click(await screen.findByRole("link", { name: /Algiers/ }));
    await waitFor(() => expect(router.state.location.pathname).toBe("/workspace/a"));
  });

  it("opens Create from its nav item and from a project's plus button", async () => {
    seed();
    const { user } = renderApp("/");
    await screen.findByRole("region", { name: "My workspaces" });
    await user.click(within(sidebar()).getByRole("button", { name: "Create" }));
    expect(useUi.getState().createOpen).toBe(true);
    expect(useUi.getState().createProjectId).toBeNull();
    await user.keyboard("{Escape}");
    await waitFor(() => expect(useUi.getState().createOpen).toBe(false));

    await user.click(screen.getByRole("button", { name: "New workspace in portfolio" }));
    expect(useUi.getState().createProjectId).toBe("p2");
  });

  it("collapses a project group", async () => {
    seed();
    const { user } = renderApp("/");
    await screen.findByRole("link", { name: /Algiers/ });
    await user.click(screen.getByRole("button", { name: /^roster/ }));
    expect(screen.queryByRole("link", { name: /Algiers/ })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /^roster/ }));
    expect(await screen.findByRole("link", { name: /Algiers/ })).toBeInTheDocument();
  });

  it("removes a project after confirming, without touching the repository", async () => {
    defaultBackend();
    backend
      .on("project_list", [project({ id: "p2", name: "portfolio", workspace_count: 0 })])
      .on("workspace_list", [])
      .on("project_remove", null);
    const { user } = renderApp("/");
    await user.click(await screen.findByRole("button", { name: "Actions for portfolio" }));
    await user.click(await screen.findByRole("menuitem", { name: "Remove project" }));

    const dialog = await screen.findByRole("dialog", { name: "Remove portfolio?" });
    expect(within(dialog).getByText(/Your repository is not touched/)).toBeInTheDocument();
    // The name has to be typed first. With no workspaces there is no choice to make.
    const remove = within(dialog).getByRole("button", { name: "Remove project" });
    expect(remove).toBeDisabled();
    expect(within(dialog).queryByRole("radio")).not.toBeInTheDocument();
    await user.type(within(dialog).getByLabelText("Project name"), "portfolio");
    await user.click(remove);
    await waitFor(() =>
      expect(backend.callsTo("project_remove")).toEqual([{ id: "p2", workspaces: null }]),
    );
    expect(await screen.findByText("Removed portfolio")).toBeInTheDocument();
  });

  it("removes a project that has workspaces, keeping them unless told to delete them", async () => {
    seed();
    backend.on("project_remove", null);
    const { user } = renderApp("/");
    await user.click(await screen.findByRole("button", { name: "Actions for roster" }));
    await user.click(await screen.findByRole("menuitem", { name: "Remove project" }));
    const dialog = await screen.findByRole("dialog", { name: "Remove roster?" });
    // The safe choice is the one that is picked.
    expect(within(dialog).getByRole("radio", { name: /Keep the workspaces/ })).toBeChecked();
    const remove = within(dialog).getByRole("button", { name: "Remove project" });
    await user.type(within(dialog).getByLabelText("Project name"), "roste");
    expect(remove).toBeDisabled();
    await user.type(within(dialog).getByLabelText("Project name"), "r");
    await user.click(remove);
    await waitFor(() =>
      expect(backend.callsTo("project_remove")).toEqual([{ id: "p1", workspaces: "keep" }]),
    );
  });

  it("deletes the workspaces too when that is chosen, and says so on the button", async () => {
    seed();
    backend.on("project_remove", null);
    const { user } = renderApp("/");
    await user.click(await screen.findByRole("button", { name: "Actions for roster" }));
    await user.click(await screen.findByRole("menuitem", { name: "Remove project" }));
    const dialog = await screen.findByRole("dialog", { name: "Remove roster?" });
    await user.click(within(dialog).getByRole("radio", { name: /Delete the workspaces too/ }));
    await user.type(within(dialog).getByLabelText("Project name"), "roster");
    await user.click(within(dialog).getByRole("button", { name: "Remove and delete workspaces" }));
    await waitFor(() =>
      expect(backend.callsTo("project_remove")).toEqual([{ id: "p1", workspaces: "delete" }]),
    );
  });

  it("warns when a project's folder is gone", async () => {
    defaultBackend();
    backend
      .on("project_list", [project({ name: "moved", path: "/code/moved", path_exists: false })])
      .on("workspace_list", []);
    renderApp("/");
    expect(await screen.findByLabelText("Folder not found")).toBeInTheDocument();
  });

  it("invites adding a project when there are none", async () => {
    defaultBackend();
    backend.on("project_list", []).on("workspace_list", []);
    const { user } = renderApp("/");
    expect(await screen.findByText("Add a repository to get started.")).toBeInTheDocument();
    const empty = screen.getByRole("region", { name: "My workspaces" });
    await user.click(within(empty).getByRole("button", { name: "Add project" }));
    expect(useUi.getState().addProjectOpen).toBe(true);
  });

  it("opens the shortcuts dialog from the footer", async () => {
    seed();
    const { user } = renderApp("/");
    await screen.findByRole("region", { name: "My workspaces" });
    await user.click(screen.getByRole("button", { name: "Keyboard shortcuts" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Toggle zen mode")).toBeInTheDocument();
    expect(within(dialog).getByText("⌘⌥A")).toBeInTheDocument();
  });
});

describe("Sidebar status line", () => {
  async function rowLine(state: "ready" | "working" | "blocked" | "errored", active: boolean) {
    defaultBackend();
    const ws = workspace({ id: "w1", city: "Algiers", state });
    backend.on("project_list", [project()]).on("workspace_list", [ws]).on("workspace_get", ws);
    renderApp(active ? "/workspace/w1" : "/");
    const row = (await screen.findByRole("link", { name: /Algiers/ })).closest(
      "div",
    ) as HTMLElement;
    return { row, line: within(row).queryByTestId("status-line") };
  }

  it("is green while the agent works, and the icon stays the branch icon", async () => {
    const { row, line } = await rowLine("working", false);
    expect(line).toHaveClass("bg-success");
    expect(row.querySelector(".animate-spin")).toBeNull();
  });

  it("is yellow when approval is needed and red when something failed", async () => {
    expect((await rowLine("blocked", false)).line).toHaveClass("bg-warning");
  });

  it("is red for a workspace in error", async () => {
    expect((await rowLine("errored", false)).line).toHaveClass("bg-destructive");
  });

  it("is white for the open workspace and absent for the others", async () => {
    expect((await rowLine("ready", true)).line).toHaveClass("bg-foreground");
  });

  it("is absent for a workspace nobody is looking at that has nothing going on", async () => {
    expect((await rowLine("ready", false)).line).toBeNull();
  });
});
