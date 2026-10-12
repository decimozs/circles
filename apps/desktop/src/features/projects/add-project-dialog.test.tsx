import { open as pickFolder } from "@tauri-apps/plugin-dialog";
import { act, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useUi } from "@/stores/ui";
import { backend } from "@/test/backend";
import { project } from "@/test/fixtures";
import { defaultBackend, renderApp } from "@/test/render";

async function openDialog(tab?: "local" | "github" | "quick") {
  const app = renderApp("/");
  await screen.findByRole("main");
  act(() => useUi.getState().setAddProjectOpen(true, tab));
  const dialog = await screen.findByRole("dialog", { name: "Add project" });
  return { ...app, dialog };
}

const added = (name: string) => project({ id: `p-${name}`, name, path: `/code/${name}` });

describe("Add project dialog", () => {
  it("opens on the tab it was asked for", async () => {
    defaultBackend();
    backend.on("github_list_repos", []);
    await openDialog("github");
    expect(await screen.findByRole("tab", { name: "GitHub", selected: true })).toBeInTheDocument();
  });

  it("adds a folder picked in the native dialog", async () => {
    defaultBackend();
    backend.on("project_add_local", added("picked"));
    vi.mocked(pickFolder).mockResolvedValueOnce("/code/picked");
    const { user, dialog } = await openDialog();
    await user.click(within(dialog).getByRole("button", { name: "Choose folder" }));
    await waitFor(() =>
      expect(backend.callsTo("project_add_local")).toEqual([{ path: "/code/picked" }]),
    );
    await waitFor(() => expect(useUi.getState().addProjectOpen).toBe(false));
    expect(await screen.findByText("Added picked")).toBeInTheDocument();
  });

  it("adds a path typed by hand", async () => {
    defaultBackend();
    backend.on("project_add_local", added("typed"));
    const { user, dialog } = await openDialog();
    await user.type(within(dialog).getByLabelText("Repository path"), "/code/typed");
    await user.click(within(dialog).getByRole("button", { name: "Add" }));
    await waitFor(() =>
      expect(backend.callsTo("project_add_local")).toEqual([{ path: "/code/typed" }]),
    );
  });

  it("keeps the dialog open and explains a folder that cannot be added", async () => {
    defaultBackend();
    backend.fail("project_add_local", {
      code: "git",
      message: "This folder is not a git repository.",
      details: { kind: "not_a_repo" },
    });
    const { user, dialog } = await openDialog();
    await user.type(within(dialog).getByLabelText("Repository path"), "/tmp/plain");
    await user.click(within(dialog).getByRole("button", { name: "Add" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("not a git repository");
    expect(useUi.getState().addProjectOpen).toBe(true);
  });

  it("clones a repository chosen from GitHub", async () => {
    defaultBackend();
    backend
      .on("github_list_repos", [
        {
          name_with_owner: "me/roster",
          name: "roster",
          description: "AI co-op",
          is_private: true,
          url: "u",
          owner: "me",
          avatar_url: "a",
        },
        {
          name_with_owner: "me/portfolio",
          name: "portfolio",
          description: null,
          is_private: false,
          url: "u",
          owner: "me",
          avatar_url: "a",
        },
      ])
      .on("project_add_github", added("portfolio"));
    const { user, dialog } = await openDialog("github");
    await user.type(await within(dialog).findByLabelText("Filter your repositories"), "port");
    expect(within(dialog).queryByText("me/roster")).not.toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: /me\/portfolio/ }));
    await waitFor(() =>
      expect(backend.callsTo("project_add_github")).toEqual([{ reference: "me/portfolio" }]),
    );
  });

  it("clones a repository typed as owner/name or a URL", async () => {
    defaultBackend();
    backend.on("github_list_repos", []).on("project_add_github", added("hello"));
    const { user, dialog } = await openDialog("github");
    await user.type(within(dialog).getByLabelText("Repository"), "https://github.com/octo/hello");
    await user.click(within(dialog).getByRole("button", { name: "Clone" }));
    await waitFor(() =>
      expect(backend.callsTo("project_add_github")).toEqual([
        { reference: "https://github.com/octo/hello" },
      ]),
    );
  });

  it("shows the command that fixes a missing GitHub sign-in", async () => {
    defaultBackend();
    backend.fail("github_list_repos", {
      code: "gh",
      message: "The GitHub CLI is not signed in. Run `gh auth login` in a terminal.",
      details: { kind: "not_authenticated", fix: "gh auth login" },
    });
    const { dialog } = await openDialog("github");
    expect(await within(dialog).findByText(/not signed in/)).toBeInTheDocument();
    expect(within(dialog).getByText("gh auth login")).toBeInTheDocument();
  });

  it("creates a project from the Quick start tab", async () => {
    defaultBackend();
    backend.on("project_quick_start", added("my-app"));
    const { user, dialog } = await openDialog("quick");
    await user.type(within(dialog).getByLabelText("Project name"), "my-app");
    await user.click(within(dialog).getByRole("button", { name: "Create project" }));
    await waitFor(() =>
      expect(backend.callsTo("project_quick_start")).toEqual([{ name: "my-app" }]),
    );
  });
});
