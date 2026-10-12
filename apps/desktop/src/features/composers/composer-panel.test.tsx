import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useChat } from "@/stores/chat";
import { backend } from "@/test/backend";
import {
  approval,
  composer,
  project,
  prStatus,
  readiness,
  runView,
  session,
  workspace,
} from "@/test/fixtures";
import { chatBackend, defaultBackend, renderApp } from "@/test/render";

function seed(extra?: () => void) {
  defaultBackend();
  backend
    .on("composer_list", [composer()])
    .on("composer_runs", [runView()])
    .on("project_list", [project()]);
  chatBackend([session({ id: "r1", workspace_id: "cw1", title: "Triage open PRs" })]);
  useChat.setState({ activeTab: {} });
  extra?.();
}

const open = async (tab: string) => {
  const app = renderApp("/composer/c1/run/r1");
  await app.user.click(await screen.findByRole("tab", { name: tab }));
  return app;
};

const fleetItem = (over: { chatStatus?: "working" | "idle" | "blocked"; cost?: number } = {}) => ({
  child: {
    id: "f1",
    composer_id: "c1",
    session_id: "r1",
    workspace_id: "w2",
    chat_id: "k1",
    created_at: 1,
  },
  workspace: workspace({
    id: "w2",
    city: "Oslo",
    title: "Add dark mode",
    branch: "me/abc12",
    state: "working",
  }),
  chat: session({
    id: "k1",
    workspace_id: "w2",
    title: "Triage open PRs",
    status: over.chatStatus ?? "working",
    total_cost_usd: over.cost ?? 0.42,
  }),
});

describe("Fleet", () => {
  it("shows what the run started, with the agent's state and cost", async () => {
    seed(() => backend.on("composer_fleet", [fleetItem()]));
    await open("Fleet");
    const list = await screen.findByRole("list", { name: "Fleet" });
    expect(within(list).getByText("Add dark mode")).toBeInTheDocument();
    expect(within(list).getByText(/me\/abc12/)).toBeInTheDocument();
    // The state is the dot. There is no line of text below the card, and no price.
    expect(within(list).getByRole("img", { name: "Working" })).toBeInTheDocument();
    expect(within(list).queryByText("Working")).not.toBeInTheDocument();
    expect(within(list).queryByText(/\$0\.42/)).not.toBeInTheDocument();
    await waitFor(() =>
      expect(backend.callsTo("composer_fleet").some((c) => c.sessionId === "r1")).toBe(true),
    );
  });

  it("says when a child waits for a permission and can stop a working one", async () => {
    seed(() => {
      backend
        .on("composer_fleet", [fleetItem({ chatStatus: "blocked" })])
        .on("composer_stop_child", null);
    });
    const app = await open("Fleet");
    expect(await screen.findByRole("img", { name: "Waiting" })).toBeInTheDocument();
    await app.user.click(screen.getByRole("button", { name: "Stop" }));
    await waitFor(() =>
      expect(backend.callsTo("composer_stop_child")[0]).toMatchObject({ chatId: "k1" }),
    );
  });

  it("offers no Stop for an agent that is done, and opens the workspace", async () => {
    seed(() => backend.on("composer_fleet", [fleetItem({ chatStatus: "idle" })]));
    const app = await open("Fleet");
    await screen.findByText("Add dark mode");
    expect(screen.queryByRole("button", { name: "Stop" })).not.toBeInTheDocument();
    await app.user.click(screen.getByRole("button", { name: "Open Add dark mode" }));
    await waitFor(() => expect(app.router.state.location.pathname).toBe("/workspace/w2"));
  });

  it("lists a workspace once, however many records the run made for it", async () => {
    const started = {
      ...fleetItem(),
      child: { ...fleetItem().child, id: "f0", chat_id: null },
      chat: null,
    };
    seed(() =>
      backend.on("composer_fleet", [started, fleetItem({ chatStatus: "idle", cost: 0.16 })]),
    );
    await open("Fleet");
    const list = await screen.findByRole("list", { name: "Fleet" });
    expect(within(list).getAllByText("Add dark mode")).toHaveLength(1);
    expect(within(list).queryByText("Idle")).not.toBeInTheDocument();
  });

  it("puts the Composer at the top, with its workspaces under it and a mark on the one waiting", async () => {
    seed(() => backend.on("composer_fleet", [fleetItem({ chatStatus: "blocked" })]));
    await open("Fleet");
    await screen.findByRole("list", { name: "Fleet" });
    expect(screen.getByText("1 workspace")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "Waiting" })).toBeInTheDocument();
  });

  it("shows the pull request number of a workspace", async () => {
    const item = fleetItem({ chatStatus: "idle" });
    item.workspace = { ...item.workspace, pr_number: 7 };
    seed(() => backend.on("composer_fleet", [item]));
    await open("Fleet");
    expect(await screen.findByText("#7")).toBeInTheDocument();
  });

  it("expands the fleet into a graph from a button in the footer", async () => {
    const item = fleetItem({ chatStatus: "working" });
    seed(() => backend.on("composer_fleet", [item]));
    const app = await open("Fleet");
    await app.user.click(await screen.findByRole("button", { name: "Expand fleet" }));
    const graph = await screen.findByRole("region", { name: "Fleet graph" });
    expect(within(graph).getByText("Release captain")).toBeInTheDocument();
    expect(within(graph).getByText("Add dark mode")).toBeInTheDocument();
    // An agent is named by its chat, with its state under the name.
    expect(within(graph).getByText("Triage open PRs")).toBeInTheDocument();
    expect(within(graph).getByText("Working")).toBeInTheDocument();
    await app.user.click(within(graph).getByRole("button", { name: "Open Add dark mode" }));
    await waitFor(() => expect(app.router.state.location.pathname).toBe("/workspace/w2"));
  });

  it("has no Expand button before anything was started", async () => {
    seed();
    await open("Fleet");
    await screen.findByText(/has not started any workspace or agent/);
    expect(screen.queryByRole("button", { name: "Expand fleet" })).not.toBeInTheDocument();
  });

  it("says where the branch stands, next to the pull request number, in its colour", async () => {
    const item = fleetItem({ chatStatus: "idle" });
    item.workspace = { ...item.workspace, pr_number: 2 };
    seed(() => {
      backend.on("composer_fleet", [item]);
      backend.on("pr_readiness", readiness({ pr: prStatus({ number: 2, state: "merged" }) }));
    });
    await open("Fleet");
    const badge = await screen.findByTestId("branch-status");
    expect(badge).toHaveTextContent("Merged");
    expect(badge.className).toMatch(/bg-info\/20/);
    expect(badge.previousElementSibling).toHaveTextContent("#2");
    // Below the branch name, in the card's own colour.
    expect(badge.parentElement?.previousElementSibling).toHaveTextContent("me/abc12");
    expect(screen.getByTestId("fleet-card").className).toMatch(/border-info\/30/);
  });

  it("colours a branch with only commits differently from a merged one, like the Create PR step", async () => {
    seed(() => {
      backend.on("composer_fleet", [fleetItem({ chatStatus: "idle" })]);
      backend.on("pr_readiness", readiness());
    });
    await open("Fleet");
    const card = await screen.findByTestId("fleet-card");
    await waitFor(() => expect(card.className).toMatch(/border-violet\/30/));
    expect(card.className).not.toMatch(/border-info/);
  });

  it("opens the workspace when the card is clicked, with no separate link icon", async () => {
    seed(() => backend.on("composer_fleet", [fleetItem({ chatStatus: "idle" })]));
    const app = await open("Fleet");
    const card = await screen.findByTestId("fleet-card");
    // One way in, and it covers the whole card.
    expect(within(card).getAllByRole("button")).toHaveLength(1);
    await app.user.click(within(card).getByRole("button", { name: "Open Add dark mode" }));
    await waitFor(() => expect(app.router.state.location.pathname).toBe("/workspace/w2"));
  });

  it("says a branch has commits when there is no pull request yet", async () => {
    const file = (path: string, added: number, removed: number) => ({
      path,
      old_path: null,
      status: "modified",
      added,
      removed,
      binary: false,
    });
    seed(() => {
      backend.on("composer_fleet", [fleetItem({ chatStatus: "idle" })]);
      backend.on("pr_readiness", readiness());
      backend.on("changes_list", [file("a.ts", 10, 4), file("b.ts", 5, 2)]);
    });
    await open("Fleet");
    // What it changed, like the Changes tab's footer, in place of a badge.
    const summary = await screen.findByTestId("branch-changes");
    expect(summary).toHaveTextContent("2 files");
    expect(summary).toHaveTextContent("+15");
    expect(summary).toHaveTextContent("−6");
    expect(screen.queryByTestId("branch-status")).not.toBeInTheDocument();
    // In the same column as the branch name, with no rule above it.
    expect(summary.className).not.toMatch(/border-t/);
    expect(summary.previousElementSibling).toHaveTextContent("me/abc12");
  });

  it("is empty before anything was started", async () => {
    seed();
    await open("Fleet");
    expect(await screen.findByText(/has not started any workspace or agent/)).toBeInTheDocument();
  });
});

describe("Approvals tab", () => {
  it("has the waiting ones to answer and the answered ones listed, with a count on the tab", async () => {
    seed(() => {
      backend
        .on("composer_approvals", (args: Record<string, unknown>) =>
          args.onlyPending
            ? [approval()]
            : [
                approval(),
                approval({ id: "a2", tool: "merge_pr", status: "allowed" }),
                approval({ id: "a3", tool: "create_pr", status: "denied" }),
              ],
        )
        .on("approval_answer", null);
    });
    const app = renderApp("/composer/c1/run/r1");
    const tab = await screen.findByRole("tab", { name: /Approvals/ });
    await waitFor(() => expect(tab).toHaveTextContent("1"));
    await app.user.click(tab);
    // The chat shows the waiting approval too, above its input. This one is in the right pane.
    const cards = await screen.findAllByRole("group", { name: "Approval for delete workspace" });
    const card = cards[cards.length - 1] as HTMLElement;
    const answered = screen.getByRole("list", { name: "Answered" });
    expect(within(answered).getByText("merge pr")).toBeInTheDocument();
    expect(within(answered).getByText("allowed")).toBeInTheDocument();
    expect(within(answered).getByText("denied")).toBeInTheDocument();
    await app.user.click(within(card).getByRole("button", { name: "Allow" }));
    await waitFor(() =>
      expect(backend.callsTo("approval_answer")[0]).toMatchObject({
        id: "a1",
        allow: true,
        forRun: false,
      }),
    );
  });

  it("says when nothing ever asked", async () => {
    seed();
    await open("Approvals");
    expect(await screen.findByText(/Nothing has asked for approval yet/)).toBeInTheDocument();
  });
});

describe("Steps tab", () => {
  const entry = (over: object) => ({
    id: "e1",
    at: Date.now() - 120_000,
    composer_id: "c1",
    session_id: "r1",
    tool: "create_workspace",
    tier: "write",
    input: "{}",
    outcome: "ok",
    summary: "made one",
    approval_id: null,
    ...over,
  });

  it("lists the run's calls with their outcome, newest first", async () => {
    seed(() => {
      backend.on("audit_list", [
        entry({
          id: "e2",
          tool: "delete_workspace",
          tier: "destructive",
          outcome: "denied",
          summary: "A person declined it.",
        }),
        entry({}),
      ]);
    });
    await open("Steps");
    const list = await screen.findByRole("list", { name: "Steps" });
    const rows = within(list).getAllByRole("listitem");
    expect(rows[0]).toHaveTextContent("delete workspace");
    expect(rows[0]).toHaveTextContent("denied");
    expect(rows[0]).toHaveTextContent("A person declined it.");
    expect(rows[1]).toHaveTextContent("create workspace");
    expect(rows[1]).toHaveTextContent("2min");
    expect(backend.callsTo("audit_list")[0]).toMatchObject({ composerId: "c1", sessionId: "r1" });
  });

  it("says when no tool was called", async () => {
    seed();
    await open("Steps");
    expect(await screen.findByText(/No tool has been called yet/)).toBeInTheDocument();
  });
});

/** The Details tab, then Settings, then one of the categories in its sidebar. */
const openSettings = async (section: string) => {
  const app = await open("Details");
  await app.user.click(await screen.findByRole("button", { name: "Composer settings" }));
  const nav = await screen.findByRole("navigation", { name: "Composer settings" });
  await app.user.click(within(nav).getByRole("button", { name: section }));
  return app;
};

describe("Details tab", () => {
  it("shows what each tool does on its own, from its setting or its mode", async () => {
    seed(() =>
      backend.on("composer_list", [
        composer({ autonomy: { merge_pr: "allow", delete_workspace: "off" } }),
      ]),
    );
    await openSettings("Autonomy");
    const autonomy = await screen.findByRole("region", { name: "Autonomy" });
    expect(await within(autonomy).findByRole("combobox", { name: "merge pr" })).toHaveValue(
      "allow",
    );
    expect(within(autonomy).getByRole("combobox", { name: "delete workspace" })).toHaveValue("off");
  });

  it("follows the mode for a tool with no setting of its own", async () => {
    seed(() => backend.on("composer_list", [composer({ mode: "plan" })]));
    await openSettings("Autonomy");
    const autonomy = await screen.findByRole("region", { name: "Autonomy" });
    expect(await within(autonomy).findByRole("combobox", { name: "create pr" })).toHaveValue("off");
  });

  it("shows no Cancel and Save until something changes, and one pair for every section", async () => {
    seed(() => backend.on("composer_update", composer()));
    const app = await openSettings("Autonomy");
    expect(screen.queryByRole("button", { name: "Save" })).not.toBeInTheDocument();
    const autonomy = await screen.findByRole("region", { name: "Autonomy" });
    await app.user.selectOptions(
      await within(autonomy).findByRole("combobox", { name: "merge pr" }),
      "allow",
    );
    // The same pair stays when another section is open.
    const nav = screen.getByRole("navigation", { name: "Composer settings" });
    await app.user.click(within(nav).getByRole("button", { name: "Limits" }));
    const agents = await screen.findByRole("spinbutton", { name: "Agents at once" });
    await app.user.tripleClick(agents);
    await app.user.keyboard("5");
    await app.user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(backend.callsTo("composer_update")).toHaveLength(1));
    const input = (backend.callsTo("composer_update")[0] as { input: Record<string, unknown> })
      .input;
    expect(input).toMatchObject({
      autonomy: { merge_pr: "allow" },
      limits: { max_children_at_once: 5 },
    });
  });

  it("drops the changes with Cancel", async () => {
    seed();
    const app = await openSettings("Notes");
    const box = await screen.findByRole("textbox", { name: "Notes" });
    await app.user.type(box, "Prefers small PRs.");
    await app.user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.getByRole("textbox", { name: "Notes" })).toHaveValue("");
    expect(screen.queryByRole("button", { name: "Save" })).not.toBeInTheDocument();
    expect(backend.callsTo("composer_update")).toHaveLength(0);
  });

  it("saves the notes with Save, not when you leave the box", async () => {
    seed(() => backend.on("composer_update", composer({ notes: "Prefers small PRs." })));
    const app = await openSettings("Notes");
    const box = await screen.findByRole("textbox", { name: "Notes" });
    await app.user.type(box, "Prefers small PRs.");
    await app.user.tab();
    expect(backend.callsTo("composer_update")).toHaveLength(0);
    await app.user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(backend.callsTo("composer_update")[0]).toMatchObject({
        id: "c1",
        input: { notes: "Prefers small PRs." },
      }),
    );
  });

  it("deletes the Composer only after asking", async () => {
    seed(() => backend.on("composer_delete", null));
    const app = await openSettings("Delete");
    await app.user.click(await screen.findByRole("button", { name: "Delete Composer" }));
    const dialog = await screen.findByRole("dialog", { name: "Delete Release captain?" });
    expect(backend.callsTo("composer_delete")).toHaveLength(0);
    await app.user.click(within(dialog).getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(backend.callsTo("composer_delete")[0]).toMatchObject({ id: "c1" }));
    await waitFor(() => expect(app.router.state.location.pathname).toBe("/composers"));
  });
});

describe("Triggers", () => {
  const trigger = (over: object = {}) => ({
    id: "t1",
    composer_id: "c1",
    kind: "schedule",
    config: { daily_at: "09:00" },
    project_id: null,
    prompt: "Triage.",
    enabled: true,
    failures: 0,
    created_at: 1,
    last_fired_at: null,
    ...over,
  });

  it("says nothing starts the Composer, and that it only runs while Circles is open", async () => {
    seed();
    await openSettings("Starts on its own");
    expect(await screen.findByText(/It only runs while Circles is open/)).toBeInTheDocument();
  });

  it("adds a daily schedule", async () => {
    seed(() => backend.on("trigger_create", trigger()));
    const app = await openSettings("Starts on its own");
    await app.user.click(await screen.findByRole("button", { name: "Add trigger" }));
    const time = await screen.findByLabelText("Time of day");
    await app.user.clear(time);
    await app.user.type(time, "08:30");
    await app.user.type(screen.getByLabelText("Trigger prompt"), "Triage the open PRs.");
    await app.user.click(screen.getByRole("button", { name: "Save trigger" }));
    await waitFor(() => expect(backend.callsTo("trigger_create")).toHaveLength(1));
    expect(backend.callsTo("trigger_create")[0]).toMatchObject({
      composerId: "c1",
      input: {
        kind: "schedule",
        config: { daily_at: "08:30" },
        prompt: "Triage the open PRs.",
        enabled: true,
      },
    });
  });

  it("needs a project for a pull request trigger", async () => {
    seed(() =>
      backend.on(
        "trigger_create",
        trigger({ kind: "pull_request", config: {}, project_id: project().id }),
      ),
    );
    const app = await openSettings("Starts on its own");
    await app.user.click(await screen.findByRole("button", { name: "Add trigger" }));
    await app.user.selectOptions(await screen.findByLabelText("Trigger kind"), "pull_request");
    const save = screen.getByRole("button", { name: "Save trigger" });
    expect(save).toBeDisabled();
    await app.user.selectOptions(screen.getByLabelText("Project"), project().id);
    expect(save).toBeEnabled();
    await app.user.click(save);
    await waitFor(() =>
      expect(backend.callsTo("trigger_create")[0]).toMatchObject({
        input: { kind: "pull_request", project_id: project().id },
      }),
    );
  });

  it("adds a Jira trigger with a query, once Jira is connected", async () => {
    seed(() => {
      backend
        .on("jira_status", {
          connected: true,
          site: "t.atlassian.net",
          email: "me@x.com",
          account_name: "Me",
          needs_reconnect: false,
          method: "token",
          choose_site: [],
        })
        .on("trigger_create", trigger({ kind: "jira_issue", config: { jql: "project = P" } }));
    });
    const app = await openSettings("Starts on its own");
    await app.user.click(await screen.findByRole("button", { name: "Add trigger" }));
    await app.user.selectOptions(await screen.findByLabelText("Trigger kind"), "jira_issue");
    const save = screen.getByRole("button", { name: "Save trigger" });
    expect(save).toBeDisabled();
    expect(screen.queryByLabelText("Project")).not.toBeInTheDocument();
    await app.user.type(screen.getByLabelText("JQL query"), "project = P");
    await app.user.click(save);
    await waitFor(() =>
      expect(backend.callsTo("trigger_create")[0]).toMatchObject({
        input: { kind: "jira_issue", config: { jql: "project = P" }, project_id: null },
      }),
    );
  });

  it("does not offer a Jira trigger while Jira is not connected", async () => {
    seed();
    const app = await openSettings("Starts on its own");
    await app.user.click(await screen.findByRole("button", { name: "Add trigger" }));
    const option = within(await screen.findByLabelText("Trigger kind")).getByRole("option", {
      name: /Jira issue/,
    });
    expect(option).toBeDisabled();
  });

  it("lists triggers in words, switches one off, and deletes one", async () => {
    seed(() => {
      backend
        .on("trigger_list", [
          trigger(),
          trigger({ id: "t2", config: { every_minutes: 120 }, enabled: false, failures: 3 }),
        ])
        .on("trigger_update", trigger())
        .on("trigger_delete", null);
    });
    const app = await openSettings("Starts on its own");
    const section = await screen.findByRole("region", { name: "Triggers" });
    expect(within(section).getByText("Every day at 09:00")).toBeInTheDocument();
    expect(within(section).getByText("Every 2 hours")).toBeInTheDocument();
    expect(within(section).getByText(/switched off after repeated failures/)).toBeInTheDocument();
    await app.user.click(
      within(section).getByRole("switch", { name: "Trigger Every day at 09:00 is on" }),
    );
    await waitFor(() =>
      expect(backend.callsTo("trigger_update")[0]).toMatchObject({
        id: "t1",
        input: { enabled: false },
      }),
    );
    await app.user.click(
      within(section).getByRole("button", { name: "Delete trigger Every day at 09:00" }),
    );
    await waitFor(() => expect(backend.callsTo("trigger_delete")[0]).toMatchObject({ id: "t1" }));
  });
});

describe("Audit page", () => {
  it("lists every call, searches, filters by Composer and exports", async () => {
    seed(() => {
      backend
        .on("audit_list", [
          {
            id: "e1",
            at: Date.now(),
            composer_id: "c1",
            session_id: "r1",
            tool: "merge_pr",
            tier: "external",
            input: "{}",
            outcome: "ok",
            summary: "merged",
            approval_id: "a1",
          },
        ])
        .on("audit_export", "/data/exports/composer-audit-1.csv");
    });
    const app = renderApp("/composers/audit");
    const list = await screen.findByRole("list", { name: "Calls" });
    expect(within(list).getByText("merge pr")).toBeInTheDocument();
    expect(within(list).getByText("Release captain")).toBeInTheDocument();
    await app.user.type(screen.getByLabelText("Search the audit log"), "merge");
    await waitFor(() =>
      expect(backend.callsTo("audit_list").some((c) => c.search === "merge")).toBe(true),
    );
    await app.user.selectOptions(screen.getByLabelText("Composer"), "c1");
    await waitFor(() =>
      expect(backend.callsTo("audit_list").some((c) => c.composerId === "c1")).toBe(true),
    );
    await app.user.click(screen.getByRole("button", { name: "Export CSV" }));
    await waitFor(() =>
      expect(backend.callsTo("audit_export")[0]).toMatchObject({ composerId: "c1" }),
    );
    expect(await screen.findByText(/composer-audit-1\.csv/)).toBeInTheDocument();
  });

  it("says when nothing matches", async () => {
    seed();
    renderApp("/composers/audit");
    expect(await screen.findByText("No calls match.")).toBeInTheDocument();
  });
});
