import { open as pickFiles } from "@tauri-apps/plugin-dialog";
import { act, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useCreatePrefs } from "@/stores/create-prefs";
import { useUi } from "@/stores/ui";
import { backend, emitBackendEvent } from "@/test/backend";
import { branch, project, pullRequest, workspace } from "@/test/fixtures";
import { defaultBackend, renderApp } from "@/test/render";

const PROMPT = "What do you want to work on?";

function seed() {
  defaultBackend();
  backend.on("workspace_create", (args: Record<string, unknown>) =>
    workspace({ id: "new1", project_id: String(args.projectId), city: "Kyoto" }),
  );
}

async function openDialog(route = "/") {
  const app = renderApp(route);
  await screen.findByRole("heading", { name: /Test Mac/ }).catch(() => undefined);
  act(() => useUi.getState().openCreate());
  const textarea = await screen.findByPlaceholderText(PROMPT);
  return { ...app, textarea };
}

function lastCreate() {
  const calls = backend.callsTo("workspace_create");
  return calls[calls.length - 1] as { projectId: string; options: Record<string, unknown> };
}

describe("Create dialog", () => {
  it("opens focused on the prompt, with the project selected", async () => {
    seed();
    const { textarea } = await openDialog();
    await waitFor(() => expect(textarea).toHaveFocus());
    expect(await screen.findByRole("button", { name: /roster/ })).toBeInTheDocument();
    expect(screen.getByText("New branch from main")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Opus/ })).toHaveTextContent("High");
  });

  it("opens preselected on the project whose + was pressed", async () => {
    seed();
    backend.on("project_list", [project(), project({ id: "p2", name: "portfolio" })]);
    const { user } = renderApp("/");
    await user.click(await screen.findByRole("button", { name: "New workspace in portfolio" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByRole("button", { name: /portfolio/ })).toBeInTheDocument();
  });

  it("keeps Create disabled until there is a prompt, and offers creating without one", async () => {
    seed();
    const { user, textarea } = await openDialog();
    const create = screen.getByRole("button", { name: "Create" });
    expect(create).toBeDisabled();
    expect(screen.getByRole("button", { name: "Create without a prompt" })).toBeEnabled();

    await user.type(textarea, "Add a dark mode toggle");
    expect(create).toBeEnabled();
    expect(
      screen.queryByRole("button", { name: "Create without a prompt" }),
    ).not.toBeInTheDocument();
  });

  it("creates with Enter, sends the options, closes and opens the workspace", async () => {
    seed();
    const { user, router, textarea } = await openDialog();
    await user.type(textarea, "Add a dark mode toggle{Enter}");

    await waitFor(() => expect(router.state.location.pathname).toBe("/workspace/new1"));
    expect(lastCreate()).toEqual({
      projectId: "p1",
      options: {
        source: { kind: "new_branch", base_branch: null },
        prompt: "Add a dark mode toggle",
        model: "opus",
        effort: "high",
        attachments: [],
      },
    });
    expect(useUi.getState().createOpen).toBe(false);
    await waitFor(() => expect(screen.queryByPlaceholderText(PROMPT)).not.toBeInTheDocument());
  });

  it("uses Shift+Enter for a new line instead of submitting", async () => {
    seed();
    const { user, textarea } = await openDialog();
    await user.type(textarea, "line one{Shift>}{Enter}{/Shift}line two");
    expect(textarea).toHaveValue("line one\nline two");
    expect(backend.callsTo("workspace_create")).toHaveLength(0);
  });

  it("creates without a prompt from the secondary action", async () => {
    seed();
    const { user, router } = await openDialog();
    await user.click(screen.getByRole("button", { name: "Create without a prompt" }));
    await waitFor(() => expect(router.state.location.pathname).toBe("/workspace/new1"));
    expect(lastCreate().options.prompt).toBeNull();
  });

  it("sends the chosen model and effort, and remembers them", async () => {
    seed();
    const { user, textarea } = await openDialog();
    await user.click(screen.getByRole("button", { name: /Opus/ }));
    await user.click(await screen.findByRole("menuitemradio", { name: "Sonnet 5.5" }));
    // The menu stays open after picking a radio item, so model and effort are set in one go.
    await user.click(await screen.findByRole("menuitemradio", { name: "Max" }));
    await waitFor(() => expect(useCreatePrefs.getState().effort).toBe("max"));
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("menu")).not.toBeInTheDocument());
    expect(screen.getByRole("button", { name: /Sonnet/ })).toHaveTextContent("Max");
    await user.type(textarea, "Refactor the router{Enter}");

    await waitFor(() => expect(backend.callsTo("workspace_create")).toHaveLength(1));
    expect(lastCreate().options).toMatchObject({ model: "sonnet", effort: "max" });
    expect(useCreatePrefs.getState()).toMatchObject({
      model: "sonnet",
      effort: "max",
      projectId: "p1",
    });
  });

  it("lets the base branch be overridden", async () => {
    seed();
    backend.on("project_branches", [branch(), branch({ name: "develop" })]);
    const { user, textarea } = await openDialog();
    await user.click(screen.getByRole("button", { name: "More options" }));
    await user.click(await screen.findByRole("menuitem", { name: /Change the base branch/ }));
    await user.type(await screen.findByLabelText(/Base branch/), "develop");
    expect(screen.getByText("New branch from develop")).toBeInTheDocument();

    await user.type(textarea, "Backport the fix{Enter}");
    await waitFor(() => expect(backend.callsTo("workspace_create")).toHaveLength(1));
    expect(lastCreate().options.source).toEqual({ kind: "new_branch", base_branch: "develop" });
  });

  it("creates from an existing branch and blocks branches that are in use", async () => {
    seed();
    backend.on("project_branches", [
      branch({ name: "main", checked_out_at: "/code/roster" }),
      branch({ name: "feature/login", local: true, remote: false }),
      branch({ name: "remote-only", local: false, remote: true }),
    ]);
    const { user, textarea } = await openDialog();
    await user.click(screen.getByRole("button", { name: "More options" }));
    await user.click(await screen.findByRole("menuitem", { name: /Use an existing branch/ }));

    const used = await screen.findByRole("button", { name: /^main/ });
    expect(used).toBeDisabled();
    expect(within(used).getByText("in use")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /remote-only/ })).toHaveTextContent("origin");

    await user.click(screen.getByRole("button", { name: /feature\/login/ }));
    expect(screen.getByText("Branch feature/login")).toBeInTheDocument();
    await user.type(textarea, "Finish the login work{Enter}");

    await waitFor(() => expect(backend.callsTo("workspace_create")).toHaveLength(1));
    expect(lastCreate().options.source).toEqual({ kind: "existing_branch", name: "feature/login" });
  });

  it("filters branches", async () => {
    seed();
    backend.on("project_branches", [
      branch({ name: "feature/login" }),
      branch({ name: "fix/typo" }),
    ]);
    const { user } = await openDialog();
    await user.click(screen.getByRole("button", { name: "More options" }));
    await user.click(await screen.findByRole("menuitem", { name: /Use an existing branch/ }));
    await user.type(await screen.findByLabelText("Filter branches"), "typo");
    expect(screen.queryByRole("button", { name: /feature\/login/ })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /fix\/typo/ })).toBeInTheDocument();
  });

  it("creates from a pull request chosen from the list or typed by number", async () => {
    seed();
    backend.on("github_list_prs", [
      pullRequest(),
      pullRequest({ number: 9, title: "From a fork", is_cross_repository: true, author: "octo" }),
    ]);
    const { user, textarea } = await openDialog();
    await user.click(screen.getByRole("button", { name: "More options" }));
    await user.click(await screen.findByRole("menuitem", { name: /Use a pull request/ }));

    expect(await screen.findByRole("button", { name: /Fix login/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /From a fork/ })).toHaveTextContent("fork");
    await user.click(screen.getByRole("button", { name: /Fix login/ }));
    expect(screen.getByText("Pull request #12")).toBeInTheDocument();
    await user.type(textarea, "Review this PR{Enter}");

    await waitFor(() => expect(backend.callsTo("workspace_create")).toHaveLength(1));
    expect(lastCreate().options.source).toEqual({ kind: "pull_request", number: 12 });
  });

  it("accepts a pull request number typed by hand and ignores non-digits", async () => {
    seed();
    backend.on("github_list_prs", []);
    const { user, textarea } = await openDialog();
    await user.click(screen.getByRole("button", { name: "More options" }));
    await user.click(await screen.findByRole("menuitem", { name: /Use a pull request/ }));
    const number = await screen.findByLabelText("Pull request number");
    await user.type(number, "4x2");
    expect(number).toHaveValue("42");
    await user.type(textarea, "Take a look{Enter}");
    await waitFor(() => expect(backend.callsTo("workspace_create")).toHaveLength(1));
    expect(lastCreate().options.source).toEqual({ kind: "pull_request", number: 42 });
  });

  it("explains how to fix a missing GitHub CLI when listing pull requests", async () => {
    seed();
    backend.fail("github_list_prs", {
      code: "gh",
      message: "The GitHub CLI is not signed in. Run `gh auth login` in a terminal.",
      details: { kind: "not_authenticated", fix: "gh auth login" },
    });
    const { user } = await openDialog();
    await user.click(screen.getByRole("button", { name: "More options" }));
    await user.click(await screen.findByRole("menuitem", { name: /Use a pull request/ }));
    expect(await screen.findByText(/not signed in/)).toBeInTheDocument();
    expect(screen.getByText("gh auth login")).toBeInTheDocument();
  });

  it("switches project", async () => {
    seed();
    backend.on("project_list", [project(), project({ id: "p2", name: "portfolio" })]);
    const { user, textarea } = await openDialog();
    const dialog = screen.getByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: /roster/ }));
    await user.click(await screen.findByRole("menuitemradio", { name: /portfolio/ }));
    await user.type(textarea, "Update the hero{Enter}");
    await waitFor(() => expect(backend.callsTo("workspace_create")).toHaveLength(1));
    expect(lastCreate().projectId).toBe("p2");
  });

  it("remembers the last project for next time", async () => {
    seed();
    backend.on("project_list", [project(), project({ id: "p2", name: "portfolio" })]);
    const { user, textarea } = await openDialog();
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: /roster/ }));
    await user.click(await screen.findByRole("menuitemradio", { name: /portfolio/ }));
    await user.type(textarea, "Hello{Enter}");
    await waitFor(() => expect(useCreatePrefs.getState().projectId).toBe("p2"));

    act(() => useUi.getState().openCreate());
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByRole("button", { name: /portfolio/ })).toBeInTheDocument();
  });

  it("attaches files and lets them be removed", async () => {
    seed();
    vi.mocked(pickFiles).mockResolvedValueOnce(["/tmp/design.png", "/tmp/spec.pdf"]);
    const { user, textarea } = await openDialog();
    await user.click(screen.getByRole("button", { name: "Attach files" }));
    expect(await screen.findByText("design.png")).toBeInTheDocument();
    expect(screen.getByText("spec.pdf")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Remove design.png" }));
    expect(screen.queryByText("design.png")).not.toBeInTheDocument();
    await user.type(textarea, "Match the spec{Enter}");
    await waitFor(() => expect(backend.callsTo("workspace_create")).toHaveLength(1));
    expect(lastCreate().options.attachments).toEqual(["/tmp/spec.pdf"]);
  });

  it("shows progress while the workspace is being created", async () => {
    seed();
    let finish: (w: unknown) => void = () => {};
    backend.on(
      "workspace_create",
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    const { user, textarea } = await openDialog();
    await user.type(textarea, "Ship it{Enter}");

    expect(await screen.findByRole("status")).toHaveTextContent("Creating your workspace");
    expect(screen.getByRole("status")).toHaveTextContent("Preparing the workspace");
    act(() =>
      emitBackendEvent("workspace:progress", {
        workspace_id: "w",
        project_id: "p1",
        stage: "fetching",
      }),
    );
    expect(await screen.findByText("Fetching from origin")).toBeInTheDocument();
    act(() =>
      emitBackendEvent("workspace:progress", {
        workspace_id: "w",
        project_id: "p1",
        stage: "copying_files",
      }),
    );
    expect(await screen.findByText("Copying files")).toBeInTheDocument();
    // The form is replaced by the loading view while the workspace is created.
    expect(screen.queryByLabelText("What do you want to work on?")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Create/ })).not.toBeInTheDocument();

    // Progress for another project is ignored.
    act(() =>
      emitBackendEvent("workspace:progress", {
        workspace_id: "x",
        project_id: "other",
        stage: "finishing",
      }),
    );
    expect(screen.queryByText("Finishing up")).not.toBeInTheDocument();

    await act(async () => finish(workspace({ id: "new1" })));
  });

  it("shows an inline error and lets the user try again", async () => {
    seed();
    let attempts = 0;
    backend.on("workspace_create", () => {
      attempts += 1;
      if (attempts === 1) {
        throw {
          code: "git",
          message: "The branch main is already checked out at /code/roster.",
          details: { kind: "branch_checked_out", path: "/code/roster" },
        };
      }
      return workspace({ id: "new1" });
    });
    const { user, router, textarea } = await openDialog();
    await user.type(textarea, "Try it{Enter}");

    expect(await screen.findByRole("alert")).toHaveTextContent("already checked out");
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(textarea).toHaveValue("Try it");

    await user.click(screen.getByRole("button", { name: /Try again/ }));
    await waitFor(() => expect(router.state.location.pathname).toBe("/workspace/new1"));
    expect(attempts).toBe(2);
  });

  it("asks for a project first when there are none", async () => {
    defaultBackend();
    backend.on("project_list", []).on("workspace_list", []);
    const { user } = renderApp("/");
    await screen.findByText("Add your first project");
    act(() => useUi.getState().openCreate());
    await screen.findByPlaceholderText(PROMPT);
    expect(screen.getByRole("button", { name: /Create without a prompt/ })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: /Choose a project/ }));
    await user.click(await screen.findByRole("menuitem", { name: /Add a project first/ }));
    expect(useUi.getState().createOpen).toBe(false);
    expect(useUi.getState().addProjectOpen).toBe(true);
  });

  it("starts fresh each time it opens", async () => {
    seed();
    const { user, textarea } = await openDialog();
    await user.type(textarea, "half written");
    act(() => useUi.getState().setCreateOpen(false));
    await waitFor(() => expect(screen.queryByPlaceholderText(PROMPT)).not.toBeInTheDocument());
    act(() => useUi.getState().openCreate());
    expect(await screen.findByPlaceholderText(PROMPT)).toHaveValue("");
  });
});
