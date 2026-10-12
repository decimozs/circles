import { act, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useUi } from "@/stores/ui";
import { backend } from "@/test/backend";
import { HOUR, NOW, project, workspace } from "@/test/fixtures";
import { defaultBackend, renderApp } from "@/test/render";

function seed() {
  defaultBackend();
  backend
    .on("project_list", [project(), project({ id: "p2", name: "portfolio" })])
    .on("workspace_list", [
      workspace({ id: "a", city: "Algiers", last_opened_at: NOW - 5 * HOUR }),
      workspace({
        id: "b",
        city: "Lima",
        title: "Fix login",
        project_id: "p2",
        last_opened_at: NOW - HOUR,
      }),
      workspace({ id: "c", city: "Oslo", state: "archived" }),
    ]);
}

async function openPalette(route = "/") {
  const app = renderApp(route);
  await screen.findByRole("main");
  await app.user.keyboard("{Meta>}k{/Meta}");
  const input = await screen.findByPlaceholderText("Type a command or search...");
  return { ...app, input };
}

const dialog = () => screen.getByRole("dialog");

describe("Command palette", () => {
  it("opens with Cmd+K and closes with Escape", async () => {
    seed();
    const { user } = await openPalette();
    expect(useUi.getState().paletteOpen).toBe(true);
    for (const tab of ["All", "Workspaces", "Projects", "Actions", "Settings"]) {
      expect(screen.getByRole("tab", { name: tab })).toBeInTheDocument();
    }
    await user.keyboard("{Escape}");
    await waitFor(() => expect(useUi.getState().paletteOpen).toBe(false));
  });

  it("toggles with Cmd+K and opens from the sidebar Search item", async () => {
    seed();
    const { user } = await openPalette();
    await user.keyboard("{Meta>}k{/Meta}");
    await waitFor(() => expect(useUi.getState().paletteOpen).toBe(false));

    await user.click(screen.getByRole("button", { name: "Search" }));
    expect(await screen.findByPlaceholderText("Type a command or search...")).toBeInTheDocument();
  });

  it("lists recent workspaces newest first, without archived ones, plus actions with shortcuts", async () => {
    seed();
    await openPalette();
    const recent = await screen.findByRole("group", { name: "Recent workspaces" });
    const names = within(recent)
      .getAllByRole("option")
      .map((o) => o.textContent ?? "");
    expect(names[0]).toContain("Fix login");
    expect(names[1]).toContain("Algiers");
    expect(names.join(" ")).not.toContain("Oslo");

    const actions = screen.getByRole("group", { name: "Actions" });
    const create = within(actions).getByRole("option", { name: /Create workspace/ });
    expect(create).toHaveTextContent("⌘N");
    expect(within(actions).getByRole("option", { name: /Add project/ })).toHaveTextContent("⌘⌥A");
    expect(within(actions).getByRole("option", { name: /Toggle theme/ })).toHaveTextContent("⌘⌥T");
    expect(within(actions).getByRole("option", { name: /Toggle left sidebar/ })).toHaveTextContent(
      "⌘B",
    );
    expect(
      within(actions).getByRole("option", { name: /Toggle right sidebar/ }),
    ).toBeInTheDocument();
    expect(within(actions).getByRole("option", { name: /Toggle zen mode/ })).toHaveTextContent(
      "⌘.",
    );
    expect(within(actions).getByRole("option", { name: /Create section/ })).toBeInTheDocument();
  });

  it("searches across workspaces, projects, actions and settings", async () => {
    seed();
    const { user, input } = await openPalette();
    await user.type(input, "port");
    expect(await screen.findByRole("group", { name: "Projects" })).toHaveTextContent("portfolio");
    expect(screen.queryByRole("group", { name: "Recent workspaces" })).not.toBeInTheDocument();

    await user.clear(input);
    await user.type(input, "login");
    const workspaces = await screen.findByRole("group", { name: "Workspaces" });
    expect(workspaces).toHaveTextContent("Fix login");
    expect(workspaces).not.toHaveTextContent("Algiers");

    await user.clear(input);
    await user.type(input, "appearance");
    expect(await screen.findByRole("group", { name: "Settings" })).toHaveTextContent("Appearance");

    await user.clear(input);
    await user.type(input, "zzzzzz");
    expect(await screen.findByText("No results.")).toBeInTheDocument();
  });

  it("opens a workspace and closes", async () => {
    seed();
    const { user, router } = await openPalette();
    await user.click(await screen.findByRole("option", { name: /Algiers/ }));
    await waitFor(() => expect(router.state.location.pathname).toBe("/workspace/a"));
    await waitFor(() => expect(useUi.getState().paletteOpen).toBe(false));
  });

  it("filters by tab", async () => {
    seed();
    const { user, router } = await openPalette();
    await user.click(screen.getByRole("tab", { name: "Settings" }));
    expect(screen.getByRole("tab", { name: "Settings", selected: true })).toBeInTheDocument();
    expect(screen.queryByRole("group", { name: "Actions" })).not.toBeInTheDocument();
    await user.click(await screen.findByRole("option", { name: /Appearance/ }));
    await waitFor(() => expect(router.state.location.pathname).toBe("/settings/appearance"));

    await user.keyboard("{Meta>}k{/Meta}");
    await screen.findByPlaceholderText("Type a command or search...");
    await user.click(screen.getByRole("tab", { name: "Workspaces" }));
    const group = await screen.findByRole("group", { name: "Workspaces" });
    expect(within(group).getAllByRole("option")).toHaveLength(2);
    expect(screen.queryByRole("group", { name: "Settings" })).not.toBeInTheDocument();
  });

  it("starts a workspace in a project from the Projects tab", async () => {
    seed();
    const { user } = await openPalette();
    await user.click(screen.getByRole("tab", { name: "Projects" }));
    await user.click(await screen.findByRole("option", { name: /portfolio/ }));
    await waitFor(() => expect(useUi.getState().createOpen).toBe(true));
    expect(useUi.getState().createProjectId).toBe("p2");
  });

  it("runs actions", async () => {
    seed();
    const { user } = await openPalette();
    await user.click(await screen.findByRole("option", { name: /Create section/ }));
    expect(await screen.findByRole("heading", { name: "Create section" })).toBeInTheDocument();

    backend.on("section_create", { id: "s1", name: "Work", position: 0 });
    await user.type(screen.getByLabelText("Section name"), "Work");
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Create" }));
    await waitFor(() => expect(backend.callsTo("section_create")).toEqual([{ name: "Work" }]));
  });

  it("toggles the theme and the sidebar from the palette", async () => {
    seed();
    const { user } = await openPalette();
    const before = useUi.getState().leftOpen;
    await user.click(await screen.findByRole("option", { name: /Toggle left sidebar/ }));
    await waitFor(() => expect(useUi.getState().leftOpen).toBe(!before));
  });

  it("does not offer to archive when no workspace is open", async () => {
    seed();
    await openPalette();
    await screen.findByRole("group", { name: "Actions" });
    expect(screen.queryByRole("option", { name: /^Archive/ })).not.toBeInTheDocument();
  });

  it("offers to archive the open workspace", async () => {
    seed();
    backend.on("workspace_get", workspace({ id: "a", city: "Algiers" }));
    backend.on("workspace_archive", workspace({ id: "a", state: "archived" }));
    const { user } = renderApp("/workspace/a");
    await screen.findAllByText("Algiers");
    await user.keyboard("{Meta>}k{/Meta}");
    await user.click(await screen.findByRole("option", { name: /Archive Algiers/ }));
    await waitFor(() =>
      expect(backend.callsTo("workspace_archive").pop()).toEqual({ id: "a", commitDirty: false }),
    );
  });

  it("offers to commit and archive when there is uncommitted work", async () => {
    seed();
    backend.on("workspace_get", workspace({ id: "a", city: "Algiers" }));
    let attempts = 0;
    backend.on("workspace_archive", (args: Record<string, unknown>) => {
      attempts += 1;
      if (!args.commitDirty) {
        throw {
          code: "git",
          message: "Algiers has 2 uncommitted changes. Commit them, or archive with auto-commit.",
          details: { kind: "dirty_tree", path: "/p" },
        };
      }
      return workspace({ id: "a", state: "archived" });
    });
    const { user } = renderApp("/workspace/a");
    await screen.findAllByText("Algiers");
    await user.keyboard("{Meta>}k{/Meta}");
    await user.click(await screen.findByRole("option", { name: /Archive Algiers/ }));

    const action = await screen.findByRole("button", { name: "Commit and archive" });
    expect(screen.getByText(/2 uncommitted changes/)).toBeInTheDocument();
    await user.click(action);
    await waitFor(() =>
      expect(backend.callsTo("workspace_archive").pop()).toEqual({ id: "a", commitDirty: true }),
    );
    expect(attempts).toBe(2);
  });

  it("opens the Create dialog from the Create workspace action", async () => {
    seed();
    const { user } = await openPalette();
    await user.click(await screen.findByRole("option", { name: /Create workspace/ }));
    await waitFor(() => expect(useUi.getState().createOpen).toBe(true));
    act(() => useUi.getState().setCreateOpen(false));
    expect(dialog).toBeDefined();
  });
});
