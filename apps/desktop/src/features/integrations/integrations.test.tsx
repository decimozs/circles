import { screen, waitFor, within } from "@testing-library/react";
import axe from "axe-core";
import { describe, expect, it } from "vitest";
import { backend } from "@/test/backend";
import { composer, project } from "@/test/fixtures";
import { defaultBackend, renderApp, settingsBackend } from "@/test/render";

const NOT = {
  connected: false,
  site: null,
  email: null,
  account_name: null,
  needs_reconnect: false,
  method: null,
  choose_site: [],
};
const YES = {
  connected: true,
  site: "team.atlassian.net",
  email: "me@x.com",
  account_name: "Marlon",
  needs_reconnect: false,
  method: "token",
  choose_site: [],
};
const SIGNED_IN = { ...YES, email: null, method: "oauth" };

function openSettings(status: Record<string, unknown> = NOT) {
  defaultBackend();
  settingsBackend({});
  backend.on("jira_status", status);
  return renderApp("/settings/integrations");
}

describe("settings, integrations", () => {
  it("holds only the connection: the Jira card, and no other setting", async () => {
    openSettings();
    const card = await screen.findByRole("region", { name: "Jira" });
    expect(within(card).getByTestId("jira-state")).toHaveTextContent("Not connected");
    expect(within(card).getByRole("button", { name: "Connect" })).toBeInTheDocument();
    expect(screen.queryByRole("switch", { name: "Let chats use Jira" })).not.toBeInTheDocument();
  });

  it("has no Integrations section in the sidebar until something is connected", async () => {
    openSettings();
    const nav = await screen.findByRole("navigation", { name: "Settings" });
    await within(nav).findByRole("link", { name: "Integrations" });
    expect(within(nav).queryByRole("heading", { name: "Integrations" })).not.toBeInTheDocument();
    expect(within(nav).queryByRole("link", { name: "Jira" })).not.toBeInTheDocument();
  });

  it("lists Jira in its own section once connected, with its own settings", async () => {
    const app = openSettings(YES);
    const nav = await screen.findByRole("navigation", { name: "Settings" });
    expect(await within(nav).findByRole("heading", { name: "Integrations" })).toBeInTheDocument();
    await app.user.click(await within(nav).findByRole("link", { name: "Jira" }));
    await waitFor(() => expect(app.router.state.location.pathname).toBe("/settings/jira"));
    expect(await screen.findByRole("switch", { name: "Let chats use Jira" })).toBeChecked();
    expect(screen.getByTestId("jira-account")).toHaveTextContent("Marlon on team.atlassian.net");
    // The connection itself stays on the other page.
    expect(screen.queryByRole("button", { name: "Disconnect" })).not.toBeInTheDocument();
    await app.user.click(screen.getByRole("link", { name: "Reconnect or disconnect" }));
    await waitFor(() => expect(app.router.state.location.pathname).toBe("/settings/integrations"));
  });

  it("says Jira is not connected when its page is opened without a connection", async () => {
    defaultBackend();
    settingsBackend({});
    backend.on("jira_status", NOT);
    renderApp("/settings/jira");
    expect(await screen.findByText(/Jira is not connected/)).toBeInTheDocument();
    expect(screen.queryByRole("switch", { name: "Let chats use Jira" })).not.toBeInTheDocument();
  });

  it("connects: the token goes to the backend once and is gone from the screen", async () => {
    backend.on("jira_connect", YES);
    const app = openSettings();
    const card = await screen.findByRole("region", { name: "Jira" });
    await app.user.click(within(card).getByRole("button", { name: "Connect" }));
    const dialog = await screen.findByRole("dialog", { name: "Connect Jira" });
    await app.user.click(within(dialog).getByRole("button", { name: "Use an API token instead" }));
    const connect = within(dialog).getByRole("button", { name: "Connect" });
    expect(connect).toBeDisabled();
    await app.user.type(within(dialog).getByLabelText("Jira site"), "team.atlassian.net");
    await app.user.type(within(dialog).getByLabelText("Atlassian email"), "me@x.com");
    const token = within(dialog).getByLabelText("API token");
    expect(token).toHaveAttribute("type", "password");
    await app.user.type(token, "SECRET-TOKEN");
    backend.on("jira_status", YES);
    await app.user.click(connect);
    await waitFor(() =>
      expect(backend.callsTo("jira_connect")[0]).toEqual({
        site: "team.atlassian.net",
        email: "me@x.com",
        token: "SECRET-TOKEN",
      }),
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(document.body.innerHTML).not.toContain("SECRET-TOKEN");
    await waitFor(() => expect(screen.getByTestId("jira-state")).toHaveTextContent("Connected"));
  });

  it("says plainly why a connection failed and keeps the dialog open", async () => {
    backend.fail("jira_connect", {
      code: "invalid",
      message: "Jira did not accept the email and token.",
    });
    const app = openSettings();
    await app.user.click(await screen.findByRole("button", { name: "Connect" }));
    const dialog = await screen.findByRole("dialog", { name: "Connect Jira" });
    await app.user.click(within(dialog).getByRole("button", { name: "Use an API token instead" }));
    await app.user.type(within(dialog).getByLabelText("Jira site"), "team.atlassian.net");
    await app.user.type(within(dialog).getByLabelText("Atlassian email"), "me@x.com");
    await app.user.type(within(dialog).getByLabelText("API token"), "bad");
    await app.user.click(within(dialog).getByRole("button", { name: "Connect" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("did not accept");
  });

  it("signs in with Atlassian: one button, then a wait, then connected", async () => {
    let finish: (value: unknown) => void = () => {};
    backend.on("jira_cancel_sign_in", null);
    backend.on(
      "jira_sign_in",
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    const app = openSettings();
    await app.user.click(await screen.findByRole("button", { name: "Connect" }));
    const dialog = await screen.findByRole("dialog", { name: "Connect Jira" });
    // No fields: the browser does the asking.
    expect(within(dialog).queryByLabelText("API token")).not.toBeInTheDocument();
    await app.user.click(within(dialog).getByRole("button", { name: "Sign in with Atlassian" }));
    // While it waits there is only the loader and Cancel.
    expect(await within(dialog).findByText("Waiting for the browser…")).toBeInTheDocument();
    expect(
      within(dialog).queryByRole("button", { name: "Sign in with Atlassian" }),
    ).not.toBeInTheDocument();
    expect(
      within(dialog).queryByRole("button", { name: "Use an API token instead" }),
    ).not.toBeInTheDocument();
    await app.user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(backend.callsTo("jira_cancel_sign_in")).toHaveLength(1));
    backend.on("jira_status", SIGNED_IN);
    finish(SIGNED_IN);
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    await waitFor(() => expect(screen.getByTestId("jira-state")).toHaveTextContent("Connected"));
    expect(screen.getByTestId("jira-details")).toHaveTextContent(
      "Marlon · team.atlassian.net · Atlassian sign-in",
    );
  });

  it("shows why a sign-in failed and offers the token instead", async () => {
    backend.fail("jira_sign_in", { code: "invalid", message: "Sign-in was refused: nope" });
    const app = openSettings();
    await app.user.click(await screen.findByRole("button", { name: "Connect" }));
    const dialog = await screen.findByRole("dialog", { name: "Connect Jira" });
    await app.user.click(within(dialog).getByRole("button", { name: "Sign in with Atlassian" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("refused");
    await app.user.click(within(dialog).getByRole("button", { name: "Use an API token instead" }));
    expect(within(dialog).getByLabelText("API token")).toBeInTheDocument();
    await app.user.click(
      within(dialog).getByRole("button", { name: "Sign in with Atlassian instead" }),
    );
    expect(
      within(dialog).getByRole("button", { name: "Sign in with Atlassian" }),
    ).toBeInTheDocument();
  });

  it("asks which site when the account reaches several", async () => {
    const sites = [
      { cloud_id: "a", name: "Alpha", url: "https://alpha.atlassian.net" },
      { cloud_id: "b", name: "Beta", url: "https://beta.atlassian.net" },
    ];
    backend.on("jira_sign_in", { ...NOT, choose_site: sites });
    backend.on("jira_choose_site", { ...SIGNED_IN, site: "beta.atlassian.net" });
    const app = openSettings();
    await app.user.click(await screen.findByRole("button", { name: "Connect" }));
    const dialog = await screen.findByRole("dialog", { name: "Connect Jira" });
    await app.user.click(within(dialog).getByRole("button", { name: "Sign in with Atlassian" }));
    await within(dialog).findByText("Which Jira site?");
    await app.user.click(within(dialog).getByRole("radio", { name: /Beta/ }));
    await app.user.click(within(dialog).getByRole("button", { name: "Use this site" }));
    await waitFor(() => expect(backend.callsTo("jira_choose_site")[0]).toEqual({ cloudId: "b" }));
  });

  it("reconnecting a signed-in connection signs in again, with no token form", async () => {
    const app = openSettings(SIGNED_IN);
    await app.user.click(await screen.findByRole("button", { name: "Reconnect" }));
    const dialog = await screen.findByRole("dialog", { name: "Reconnect Jira" });
    expect(
      within(dialog).getByRole("button", { name: "Sign in with Atlassian" }),
    ).toBeInTheDocument();
    expect(within(dialog).queryByLabelText("API token")).not.toBeInTheDocument();
  });

  it("reconnects with the site and email filled in and an empty token", async () => {
    const app = openSettings(YES);
    await app.user.click(await screen.findByRole("button", { name: "Reconnect" }));
    const dialog = await screen.findByRole("dialog", { name: "Reconnect Jira" });
    expect(within(dialog).getByLabelText("Jira site")).toHaveValue("team.atlassian.net");
    expect(within(dialog).getByLabelText("Atlassian email")).toHaveValue("me@x.com");
    expect(within(dialog).getByLabelText("API token")).toHaveValue("");
  });

  it("asks before it disconnects", async () => {
    backend.on("jira_disconnect", NOT);
    const app = openSettings(YES);
    await app.user.click(await screen.findByRole("button", { name: "Disconnect" }));
    const dialog = await screen.findByRole("dialog", { name: "Disconnect Jira?" });
    expect(backend.callsTo("jira_disconnect")).toHaveLength(0);
    await app.user.click(within(dialog).getByRole("button", { name: "Disconnect" }));
    await waitFor(() => expect(backend.callsTo("jira_disconnect")).toHaveLength(1));
  });

  it("tells when the token was turned down", async () => {
    openSettings({ ...YES, needs_reconnect: true });
    await waitFor(() =>
      expect(screen.getByTestId("jira-state")).toHaveTextContent("Reconnect needed"),
    );
  });

  it("has no accessibility violations", async () => {
    openSettings(YES);
    await waitFor(() => expect(screen.getByTestId("jira-state")).toHaveTextContent("Connected"));
    const result = await axe.run(document.body, {
      rules: { "color-contrast": { enabled: false }, region: { enabled: false } },
    });
    expect(result.violations.map((v) => v.id)).toEqual([]);
  });
});

describe("the New Composer dialog", () => {
  async function openDialog(status: Record<string, unknown>) {
    defaultBackend();
    backend
      .on("composer_list", [composer()])
      .on("project_list", [
        project({ id: "p1", name: "alpha" }),
        project({ id: "p2", name: "beta" }),
      ])
      .on("jira_status", status)
      .on("composer_create", composer());
    const app = renderApp("/composers");
    await app.user.click(await screen.findByRole("button", { name: /New Composer/ }));
    return { app, dialog: await screen.findByRole("dialog", { name: "New Composer" }) };
  }

  it("works on every repository unless told otherwise", async () => {
    const { app, dialog } = await openDialog(NOT);
    await app.user.type(within(dialog).getByLabelText("Name"), "Checker");
    await app.user.click(within(dialog).getByRole("button", { name: "Create Composer" }));
    await waitFor(() => expect(backend.callsTo("composer_create")).toHaveLength(1));
    const input = (backend.callsTo("composer_create")[0] as { input: Record<string, unknown> })
      .input;
    expect(input.projects).toBeNull();
    expect(input.integrations).toEqual([]);
  });

  it("limits it to the repositories that are picked, and needs at least one", async () => {
    const { app, dialog } = await openDialog(NOT);
    await app.user.type(within(dialog).getByLabelText("Name"), "Checker");
    await app.user.selectOptions(
      within(dialog).getByLabelText("Repositories it may work on"),
      "only",
    );
    expect(within(dialog).getByRole("button", { name: "Create Composer" })).toBeDisabled();
    await app.user.click(within(dialog).getByRole("combobox", { name: "Search repositories" }));
    // A search, not a list of boxes: both are offered, and typing narrows them.
    expect(await within(dialog).findByRole("option", { name: "alpha" })).toBeInTheDocument();
    await app.user.type(
      within(dialog).getByRole("combobox", { name: "Search repositories" }),
      "bet",
    );
    expect(within(dialog).queryByRole("option", { name: "alpha" })).not.toBeInTheDocument();
    await app.user.click(within(dialog).getByRole("option", { name: "beta" }));
    expect(within(dialog).getByRole("button", { name: "Remove beta" })).toBeInTheDocument();
    await app.user.click(within(dialog).getByRole("button", { name: "Create Composer" }));
    await waitFor(() => expect(backend.callsTo("composer_create")).toHaveLength(1));
    const input = (backend.callsTo("composer_create")[0] as { input: Record<string, unknown> })
      .input;
    expect(input.projects).toEqual(["p2"]);
  });

  it("previews the instructions as Markdown and goes back to writing", async () => {
    const { app, dialog } = await openDialog(NOT);
    await app.user.type(
      within(dialog).getByLabelText("Instructions"),
      "# Checker{Enter}{Enter}- run /code-check",
    );
    await app.user.click(within(dialog).getByRole("button", { name: "Preview" }));
    const preview = within(dialog).getByRole("region", { name: "Instructions preview" });
    expect(within(preview).getByRole("heading", { name: "Checker" })).toBeInTheDocument();
    expect(within(preview).getByRole("listitem")).toHaveTextContent("run /code-check");
    expect(within(dialog).queryByLabelText("Instructions")).not.toBeInTheDocument();
    await app.user.click(within(dialog).getByRole("button", { name: "Write" }));
    expect(within(dialog).getByLabelText("Instructions")).toHaveValue(
      "# Checker\n\n- run /code-check",
    );
  });

  it("colours the skills it knows, like a command in the chat input", async () => {
    defaultBackend();
    backend
      .on("composer_list", [composer()])
      .on("project_list", [project()])
      .on("jira_status", NOT)
      .on("composer_skills", [{ name: "code-check", description: "Check code", source: "skill" }]);
    const app = renderApp("/composers");
    await app.user.click(await screen.findByRole("button", { name: /New Composer/ }));
    const dialog = await screen.findByRole("dialog", { name: "New Composer" });
    await app.user.type(
      within(dialog).getByLabelText("Instructions"),
      "run /code-check and /nothing",
    );
    // The picker still works beside the colour: "/" lists the skills, and a choice is inserted.
    await app.user.type(within(dialog).getByLabelText("Instructions"), " /co");
    const list = await within(dialog).findByRole("listbox", { name: "Skills" });
    await app.user.click(within(list).getByRole("option", { name: /code-check/ }));
    expect(within(dialog).getByLabelText("Instructions")).toHaveValue(
      "run /code-check and /nothing /code-check ",
    );
    const marks = within(dialog).getByTestId("skill-marks");
    await waitFor(() =>
      expect([...marks.querySelectorAll("[data-skill]")].map((m) => m.textContent)).toEqual([
        "/code-check",
        "/code-check",
      ]),
    );
  });

  it("says there is nothing to preview when the instructions are empty", async () => {
    const { app, dialog } = await openDialog(NOT);
    await app.user.click(within(dialog).getByRole("button", { name: "Preview" }));
    expect(within(dialog).getByText("Nothing to preview yet.")).toBeInTheDocument();
  });

  it("searches a long list of repositories, with tags that can be removed", async () => {
    defaultBackend();
    const many = Array.from({ length: 40 }, (_, i) => project({ id: `p${i}`, name: `repo-${i}` }));
    backend.on("composer_list", [composer()]).on("project_list", many).on("jira_status", NOT);
    const app = renderApp("/composers");
    await app.user.click(await screen.findByRole("button", { name: /New Composer/ }));
    const dialog = await screen.findByRole("dialog", { name: "New Composer" });
    await app.user.selectOptions(
      within(dialog).getByLabelText("Repositories it may work on"),
      "only",
    );
    const search = within(dialog).getByRole("combobox", { name: "Search repositories" });
    await app.user.click(search);
    // At most a handful show at once, however many there are.
    const list = await within(dialog).findByRole("listbox", { name: "Repositories" });
    expect(within(list).getAllByRole("option").length).toBeLessThanOrEqual(8);
    await app.user.type(search, "repo-3");
    await app.user.keyboard("{ArrowDown}{Enter}");
    const tags = within(dialog).getByRole("list", { name: "Picked repositories" });
    expect(within(tags).getAllByRole("listitem")).toHaveLength(1);
    // A repository that is picked is not offered again.
    await app.user.type(search, "repo-3");
    const left = within(dialog)
      .queryAllByRole("option")
      .map((o) => o.textContent);
    expect(left).not.toContain(
      within(tags).getAllByRole("listitem")[0].textContent?.replace("", ""),
    );
    await app.user.click(within(tags).getByRole("button", { name: /^Remove / }));
    expect(
      within(dialog).queryByRole("list", { name: "Picked repositories" }),
    ).not.toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Create Composer" })).toBeDisabled();
  });

  it("cannot switch Jira on until it is connected, and points to the settings", async () => {
    const { app, dialog } = await openDialog(NOT);
    expect(within(dialog).getByRole("switch", { name: "Use Jira" })).toHaveAttribute(
      "aria-disabled",
      "true",
    );
    await app.user.click(
      within(dialog).getByRole("button", { name: /Open Settings, Integrations/ }),
    );
    await waitFor(() => expect(app.router.state.location.pathname).toBe("/settings/integrations"));
  });

  it("switches Jira on for this Composer when it is connected", async () => {
    const { app, dialog } = await openDialog(YES);
    await app.user.type(within(dialog).getByLabelText("Name"), "Checker");
    const toggle = within(dialog).getByRole("switch", { name: "Use Jira" });
    expect(toggle).not.toBeChecked();
    await app.user.click(toggle);
    await app.user.click(within(dialog).getByRole("button", { name: "Create Composer" }));
    await waitFor(() => expect(backend.callsTo("composer_create")).toHaveLength(1));
    const input = (backend.callsTo("composer_create")[0] as { input: Record<string, unknown> })
      .input;
    expect(input.integrations).toEqual(["jira"]);
  });
});

describe("settings, smart titles", () => {
  function openSmart(status = { embedded_available: true, provider: "embedded" }) {
    defaultBackend();
    settingsBackend({});
    backend.on("smart_status", status);
    return renderApp("/settings/smart");
  }

  it("has its own page in the sidebar, with the switches and the model choice", async () => {
    openSmart();
    const nav = await screen.findByRole("navigation", { name: "Settings" });
    expect(within(nav).getByRole("link", { name: "Smart titles" })).toBeInTheDocument();
    expect(await screen.findByRole("switch", { name: "Name new chats" })).toBeChecked();
    // Suggestions are off until switched on.
    expect(screen.getByRole("switch", { name: "Suggest the next message" })).not.toBeChecked();
    expect(screen.getByLabelText("Model")).toHaveValue("embedded");
    // The example for a new Composer is on by default, and has its own switch.
    expect(
      screen.getByRole("switch", { name: "Show an example of what to ask a Composer" }),
    ).toBeChecked();
  });

  it("says where the text goes and which model is inside", async () => {
    openSmart();
    expect(
      await screen.findByText(/runs on this Mac and sends nothing anywhere/),
    ).toBeInTheDocument();
    expect(screen.getByText(/SmolLM2-360M-Instruct.*Apache License 2.0/)).toBeInTheDocument();
  });

  it("tries the model and shows what it wrote and how long it took", async () => {
    backend.on("smart_test", {
      title: "Fix failing checkout test",
      suggestion: "Open a pull request for this fix",
      millis: 183,
      provider: "embedded",
    });
    const app = openSmart();
    await app.user.click(await screen.findByRole("button", { name: "Try it" }));
    const result = await screen.findByTestId("smart-result");
    expect(result).toHaveTextContent("Fix failing checkout test");
    expect(result).toHaveTextContent("Open a pull request for this fix");
    expect(result).toHaveTextContent("183 ms");
  });

  it("warns when the model file is not in this copy of Circles", async () => {
    openSmart({ embedded_available: false, provider: "embedded" });
    expect(await screen.findByRole("alert")).toHaveTextContent("model file was not found");
  });

  it("writes the choice to the settings file", async () => {
    const app = openSmart();
    await app.user.click(await screen.findByRole("switch", { name: "Suggest the next message" }));
    await waitFor(() =>
      expect(backend.callsTo("settings_set")[0]).toMatchObject({
        path: ["smart", "suggestions"],
        value: true,
      }),
    );
  });
});
