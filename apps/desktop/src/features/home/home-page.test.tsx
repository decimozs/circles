import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useUi } from "@/stores/ui";
import { backend } from "@/test/backend";
import { HOUR, project, workspace } from "@/test/fixtures";
import { defaultBackend, renderApp } from "@/test/render";

const DAY = 24 * HOUR;

function seed(workspaces = [workspace()]) {
  defaultBackend();
  backend.on("workspace_list", workspaces);
}

describe("Home", () => {
  it("shows the machine name and workspaces grouped by recency with counts", async () => {
    const now = Date.now();
    seed([
      workspace({ id: "a", city: "Algiers", updated_at: now - 1000 }),
      workspace({ id: "b", city: "Lima", title: "Fix login", updated_at: now - 2 * DAY }),
      workspace({ id: "c", city: "Oslo", updated_at: now - 30 * DAY }),
    ]);
    renderApp("/");

    expect(await screen.findByRole("heading", { name: /Test Mac/ })).toBeInTheDocument();
    const today = await screen.findByRole("region", { name: "Today" });
    expect(within(today).getByText("Algiers")).toBeInTheDocument();
    expect(within(today).getByText("1")).toBeInTheDocument();
    const week = screen.getByRole("region", { name: "Last 7 days" });
    expect(within(week).getByText("Fix login")).toBeInTheDocument();
    expect(
      within(screen.getByRole("region", { name: "Older" })).getByText("Oslo"),
    ).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Yesterday" })).not.toBeInTheDocument();
  });

  it("filters by search across title, city, branch and project", async () => {
    seed([
      workspace({ id: "a", city: "Algiers", branch: "me/aaa11" }),
      workspace({ id: "b", city: "Lima", title: "Fix login", branch: "me/bbb22" }),
    ]);
    const { user } = renderApp("/");
    const home = await screen.findByRole("region", { name: "Today" });
    expect(within(home).getAllByRole("button")).toHaveLength(2);

    const search = screen.getByRole("textbox", { name: "Search workspaces" });
    const main = within(screen.getByRole("main"));
    await user.type(search, "login");
    expect(
      within(screen.getByRole("region", { name: "Today" })).getAllByRole("button"),
    ).toHaveLength(1);
    expect(main.getByText("Fix login")).toBeInTheDocument();
    expect(main.queryByText("Algiers")).not.toBeInTheDocument();

    await user.clear(search);
    await user.type(search, "bbb22");
    expect(main.getByText("Fix login")).toBeInTheDocument();

    await user.clear(search);
    await user.type(search, "nothing like this");
    expect(await screen.findByText("No workspaces match your search.")).toBeInTheDocument();
  });

  it("hides archived workspaces until asked", async () => {
    seed([
      workspace({ id: "a", city: "Algiers" }),
      workspace({ id: "b", city: "Lima", state: "archived" }),
    ]);
    const { user } = renderApp("/");
    const today = await screen.findByRole("region", { name: "Today" });
    expect(within(today).queryByText("Lima")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /Hiding archived/ }));
    await user.click(await screen.findByRole("menuitemradio", { name: "Show archived" }));
    expect(await screen.findByText("Lima")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Showing archived/ })).toBeInTheDocument();
  });

  it("opens a workspace when its row is clicked", async () => {
    seed([workspace({ id: "w9", city: "Lima" })]);
    const { user, router } = renderApp("/");
    const today = await screen.findByRole("region", { name: "Today" });
    await user.click(within(today).getByRole("button", { name: /Lima/ }));
    await waitFor(() => expect(router.state.location.pathname).toBe("/workspace/w9"));
  });

  it("opens the Create dialog from the header button", async () => {
    seed();
    const { user } = renderApp("/");
    await user.click(await screen.findByRole("button", { name: "Create workspace" }));
    expect(useUi.getState().createOpen).toBe(true);
    expect(await screen.findByPlaceholderText("What do you want to work on?")).toBeInTheDocument();
  });

  it("guides a first launch with no projects and opens Add project on the right tab", async () => {
    defaultBackend();
    backend.on("project_list", []).on("workspace_list", []);
    const { user } = renderApp("/");

    expect(await screen.findByText("Add your first project")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Clone from GitHub/ }));
    expect(useUi.getState().addProjectTab).toBe("github");
    expect(await screen.findByRole("tab", { name: "GitHub", selected: true })).toBeInTheDocument();
  });

  it("invites the first workspace when projects exist but workspaces do not", async () => {
    defaultBackend();
    backend.on("project_list", [project()]).on("workspace_list", []);
    renderApp("/");
    expect(await screen.findByText("No workspaces yet.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Create your first workspace" })).toBeInTheDocument();
  });
});
