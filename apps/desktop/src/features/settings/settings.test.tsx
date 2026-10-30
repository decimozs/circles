import { act, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useChat } from "@/stores/chat";
import { useCreatePrefs } from "@/stores/create-prefs";
import { backend, emitBackendEvent } from "@/test/backend";
import { appInfo, project } from "@/test/fixtures";
import { defaultBackend, renderApp, settingsBackend } from "@/test/render";

const last = () => backend.callsTo("settings_set").slice(-1)[0];

function open(route = "/settings/general", levels: Parameters<typeof settingsBackend>[0] = {}) {
  defaultBackend();
  const store = settingsBackend(levels);
  return { ...renderApp(route), store };
}

describe("settings pages", () => {
  it("lists the groups and the repositories, and Back leaves", async () => {
    const app = open();
    const nav = await screen.findByRole("navigation", { name: "Settings" });
    for (const name of ["Personal", "Agents & environment", "More", "Repositories"]) {
      expect(await within(nav).findByRole("heading", { name })).toBeInTheDocument();
    }
    expect(within(nav).getByRole("link", { name: "Default models" })).toBeInTheDocument();
    expect(within(nav).getByRole("link", { name: project().name })).toBeInTheDocument();
    await app.user.click(within(nav).getByRole("button", { name: "Back" }));
    await waitFor(() => expect(app.router.state.location.pathname).toBe("/"));
  });

  it("writes a changed setting to the user file", async () => {
    const app = open();
    const select = await screen.findByLabelText("Send messages with");
    await app.user.selectOptions(select, "mod_enter");
    await waitFor(() =>
      expect(backend.callsTo("settings_set")[0]).toMatchObject({
        scope: "user",
        path: ["general", "send_key"],
        value: "mod_enter",
      }),
    );
    // And the chat obeys it.
    await waitFor(() => expect(useChat.getState().sendKey).toBe("mod-enter"));
  });

  it("toggles a switch and resets it to the default", async () => {
    const app = open("/settings/general", { user: { general: { sound: false } } });
    const toggle = await screen.findByRole("switch", { name: "Completion sound" });
    expect(toggle).not.toBeChecked();
    await app.user.click(screen.getByRole("button", { name: "Reset Completion sound" }));
    await waitFor(() =>
      expect(last()).toMatchObject({
        path: ["general", "sound"],
        value: null,
      }),
    );
    await waitFor(() =>
      expect(screen.getByRole("switch", { name: "Completion sound" })).toBeChecked(),
    );
  });

  it("shows an error from the backend next to the row", async () => {
    const app = open();
    backend.fail("settings_set", { code: "settings", message: "claude_path must be a file" });
    await app.user.selectOptions(await screen.findByLabelText("Send messages with"), "enter");
    // A failing write leaves the control on what the file holds.
    expect(await screen.findByRole("alert")).toHaveTextContent("claude_path must be a file");
  });

  it("reflects an edit made outside the app", async () => {
    const { store } = open();
    await screen.findByLabelText("Follow-up behaviour");
    expect(screen.getByLabelText("Follow-up behaviour")).toHaveValue("steer");
    store.user = { general: { follow_up: "queue" } };
    act(() => emitBackendEvent("settings:changed", null));
    await waitFor(() => expect(screen.getByLabelText("Follow-up behaviour")).toHaveValue("queue"));
  });

  it("finds rows by title and description", async () => {
    const app = open();
    const box = await screen.findByLabelText("Search settings");
    await app.user.type(box, "privacy");
    const rows = await screen.findAllByTestId("setting-row");
    expect(rows).toHaveLength(1);
    expect(rows[0]).toHaveTextContent("Full disk access");
    await app.user.clear(box);
    await app.user.type(box, "queue");
    expect((await screen.findAllByTestId("setting-row")).length).toBeGreaterThan(0);
    expect(screen.getByText("Follow-up behaviour")).toBeInTheDocument();
    await app.user.clear(box);
    await app.user.type(box, "zzzz");
    expect(await screen.findByText(/Nothing matches/)).toBeInTheDocument();
  });

  it("focuses the search with Cmd+F", async () => {
    const app = open();
    await screen.findByLabelText("Search settings");
    await app.user.keyboard("{Meta>}f{/Meta}");
    expect(screen.getByLabelText("Search settings")).toHaveFocus();
  });

  it("applies the theme, font size and density from the file", async () => {
    open("/settings/appearance", {
      user: { appearance: { theme: "dark", font_size: "large", density: "compact" } },
    });
    await waitFor(() => expect(document.documentElement).toHaveClass("dark"));
    expect(document.documentElement.style.fontSize).toBe("18px");
    expect(document.documentElement.dataset.density).toBe("compact");
  });

  it("writes the theme chosen in the toggle back to the file", async () => {
    const app = open("/");
    await app.user.click(
      await screen.findByRole("button", { name: new RegExp(appInfo().machine_name) }),
    );
    await app.user.click(await screen.findByRole("button", { name: "Dark" }));
    await waitFor(() =>
      expect(last()).toMatchObject({
        path: ["appearance", "theme"],
        value: "dark",
      }),
    );
  });

  it("changes the theme from the Appearance page", async () => {
    const app = open("/settings/appearance");
    await app.user.selectOptions(await screen.findByLabelText("Theme"), "dark");
    await waitFor(() => expect(document.documentElement).toHaveClass("dark"));
    expect(last()).toMatchObject({ path: ["appearance", "theme"], value: "dark" });
  });

  it("warns when the permission mode is bypass", async () => {
    open("/settings/agents", { user: { agents: { permission_mode: "bypass" } } });
    expect(await screen.findByRole("alert")).toHaveTextContent(/without asking/);
  });

  it("saves extra arguments one per line", async () => {
    const app = open("/settings/agents");
    const box = await screen.findByLabelText("Extra arguments");
    await app.user.type(box, "--verbose{Enter}--debug");
    await app.user.tab();
    await waitFor(() =>
      expect(last()).toMatchObject({
        path: ["agents", "extra_args"],
        value: ["--verbose", "--debug"],
      }),
    );
  });

  it("checks the Claude version", async () => {
    backend.on("claude_check", {
      found: true,
      path: "/bin/claude",
      version: "2.1.0",
      problem: null,
    });
    const app = open("/settings/agents");
    backend.on("claude_check", {
      found: true,
      path: "/bin/claude",
      version: "2.1.0",
      problem: null,
    });
    await app.user.click(await screen.findByRole("button", { name: "Check" }));
    expect(await screen.findByTestId("claude-version")).toHaveTextContent("2.1.0");
  });

  it("masks environment variable values until shown", async () => {
    const app = open("/settings/environment", {
      user: { environment_variables: { API_KEY: "s3cret" } },
    });
    expect(await screen.findByText("API_KEY")).toBeInTheDocument();
    expect(screen.queryByText("s3cret")).not.toBeInTheDocument();
    await app.user.click(screen.getByRole("button", { name: "Show" }));
    expect(screen.getByText("s3cret")).toBeInTheDocument();
  });

  it("adds an environment variable", async () => {
    const app = open("/settings/environment");
    await app.user.type(await screen.findByLabelText("Variable name"), "TOKEN");
    await app.user.type(screen.getByLabelText("Variable value"), "abc");
    await app.user.click(screen.getByRole("button", { name: "Add" }));
    await waitFor(() =>
      expect(last()).toMatchObject({
        path: ["environment_variables", "TOKEN"],
        value: "abc",
      }),
    );
  });

  it("only resets app data after typing reset", async () => {
    backend.on("app_reset_data", null);
    const app = open("/settings/advanced");
    backend.on("app_reset_data", null);
    await app.user.click(await screen.findByRole("button", { name: "Reset…" }));
    const confirm = screen.getByRole("button", { name: "Reset app data" });
    expect(confirm).toBeDisabled();
    await app.user.type(screen.getByLabelText("Type reset to confirm"), "reset");
    expect(confirm).toBeEnabled();
    await app.user.click(confirm);
    await waitFor(() =>
      expect(backend.callsTo("app_reset_data")[0]).toMatchObject({ confirm: "reset" }),
    );
  });

  it("opens the settings file of a level", async () => {
    backend.on("settings_open_file", "/data/settings.toml");
    const app = open();
    backend.on("settings_open_file", "/data/settings.toml");
    await app.user.click(await screen.findByRole("button", { name: /Open settings.toml/ }));
    await app.user.click(await screen.findByRole("menuitem", { name: /User/ }));
    await waitFor(() =>
      expect(backend.callsTo("settings_open_file")[0]).toMatchObject({ scope: "user" }),
    );
  });
});

describe("repository settings", () => {
  const route = `/settings/repo/${project().id}`;

  it("writes to this Mac by default and to the repository when shared", async () => {
    const app = open(route);
    const setup = await screen.findByLabelText("Setup script");
    await app.user.type(setup, "pnpm install");
    await app.user.tab();
    await waitFor(() =>
      expect(last()).toMatchObject({
        scope: "local",
        path: ["scripts", "setup"],
        value: "pnpm install",
      }),
    );
    await app.user.click(screen.getByRole("button", { name: "Shared" }));
    await app.user.type(screen.getByLabelText("Archive script"), "rm -rf tmp");
    await app.user.tab();
    await waitFor(() =>
      expect(last()).toMatchObject({
        scope: "project",
        path: ["scripts", "archive"],
      }),
    );
  });

  it("learns the repository and saves only the proposals that are kept", async () => {
    backend.on("project_learn_conventions", {
      branch_pattern: "{type}/{ticket}-{slug}",
      pr: "Start the title with the ticket.",
      commits: "Conventional commits.",
    });
    const app = open(route);
    backend.on("project_learn_conventions", {
      branch_pattern: "{type}/{ticket}-{slug}",
      pr: "Start the title with the ticket.",
      commits: "Conventional commits.",
    });
    await app.user.click(await screen.findByRole("button", { name: "Learn" }));
    const dialog = await screen.findByRole("dialog", { name: "What Claude learned" });
    expect(within(dialog).getByText("{type}/{ticket}-{slug}")).toBeInTheDocument();
    await app.user.click(within(dialog).getByLabelText("Save Commit instructions"));
    await app.user.click(within(dialog).getByRole("button", { name: "Save 2" }));
    await waitFor(() => expect(backend.callsTo("settings_set")).toHaveLength(2));
    expect(backend.callsTo("settings_set")).toEqual([
      expect.objectContaining({
        scope: "local",
        path: ["git", "branch_pattern"],
        value: "{type}/{ticket}-{slug}",
      }),
      expect.objectContaining({
        scope: "local",
        path: ["instructions", "pr"],
        value: "Start the title with the ticket.",
      }),
    ]);
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("says when the repository shows nothing to keep, and why learning failed", async () => {
    backend.on("project_learn_conventions", { branch_pattern: null, pr: null, commits: null });
    const app = open(route);
    backend.on("project_learn_conventions", { branch_pattern: null, pr: null, commits: null });
    await app.user.click(await screen.findByRole("button", { name: "Learn" }));
    expect(await screen.findByText(/does not show a clear convention/)).toBeInTheDocument();
    expect(screen.queryByRole("dialog", { name: "What Claude learned" })).not.toBeInTheDocument();
    backend.fail("project_learn_conventions", {
      code: "invalid",
      message: "There is nothing to learn from yet.",
    });
    await app.user.click(screen.getByRole("button", { name: "Learn" }));
    expect(await screen.findByText(/nothing to learn from yet/)).toBeInTheDocument();
  });

  it("changes the base branch", async () => {
    backend.on("project_set_base_branch", project());
    const app = open(route);
    backend.on("project_set_base_branch", project());
    const box = await screen.findByLabelText("Base branch");
    await app.user.clear(box);
    await app.user.type(box, "develop");
    await app.user.tab();
    await waitFor(() =>
      expect(backend.callsTo("project_set_base_branch")[0]).toMatchObject({ branch: "develop" }),
    );
  });

  it("asks before removing the project", async () => {
    backend.on("project_remove", null);
    const app = open(route);
    backend.on("project_remove", null);
    await app.user.click(await screen.findByRole("button", { name: "Remove…" }));
    expect(backend.callsTo("project_remove")).toHaveLength(0);
    const dialog = await screen.findByRole("dialog");
    const remove = within(dialog).getByRole("button", { name: /^Remove/ });
    expect(remove).toBeDisabled();
    await app.user.type(within(dialog).getByLabelText("Project name"), project().name);
    await app.user.click(remove);
    await waitFor(() => expect(backend.callsTo("project_remove")).toHaveLength(1));
  });
});

describe("checkpoint retention", () => {
  it("is written as a number", async () => {
    const app = open("/settings/advanced");
    await app.user.selectOptions(await screen.findByLabelText("Keep checkpoints for"), "90");
    await waitFor(() =>
      expect(last()).toMatchObject({ path: ["advanced", "checkpoint_days"], value: 90 }),
    );
  });
});

describe("default model", () => {
  it("is applied once, and a later pick in the dialog is kept", async () => {
    open("/settings/models", { user: { models: { model: "haiku", effort: "low" } } });
    await waitFor(() => expect(useCreatePrefs.getState().model).toBe("haiku"));
    expect(useCreatePrefs.getState().effort).toBe("low");
    useCreatePrefs.getState().set({ model: "sonnet" });
    act(() => emitBackendEvent("settings:changed", null));
    await new Promise((r) => setTimeout(r, 50));
    expect(useCreatePrefs.getState().model).toBe("sonnet");
  });
});
