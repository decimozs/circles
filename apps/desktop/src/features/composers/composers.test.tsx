import { act, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { useChat } from "@/stores/chat";
import { useUi } from "@/stores/ui";
import { backend } from "@/test/backend";
import { approval, composer, project, runView, session } from "@/test/fixtures";
import { chatBackend, defaultBackend, renderApp, settingsBackend } from "@/test/render";

function seed(extra?: () => void) {
  defaultBackend();
  backend
    .on("composer_list", [composer()])
    .on("composer_runs", [runView()])
    .on("project_list", [project()]);
  extra?.();
}

describe("sidebar", () => {
  it("lists Search, Create, Home, Composer and Pull requests in that order, and one row per Composer with no runs under it", async () => {
    seed();
    renderApp("/");
    const nav = await screen.findByRole("navigation", { name: "Primary" });
    const labels = within(nav)
      .getAllByRole("link")
      .concat(within(nav).getAllByRole("button"))
      .map((e) => e.textContent?.trim());
    expect(labels).toContain("Composer");
    const items = [...nav.children].map((e) => e.textContent?.trim());
    expect(items).toEqual(["Search", "Create", "Home", "Composer", "Pull requests"]);

    const group = await screen.findByRole("region", { name: "Composers" });
    const row = await within(group).findByRole("link", { name: /Release captain/ });
    expect(row).toHaveAttribute("href", "/composer/c1");
    // A Composer has one chat: no run is listed under it.
    expect(within(group).queryByRole("link", { name: /Triage open PRs/ })).not.toBeInTheDocument();
    expect(within(group).getAllByRole("link")).toHaveLength(1);
  });

  describe("the row of a Composer shows what it is doing", () => {
    const rowOf = async (runs: ReturnType<typeof runView>[]) => {
      seed(() => backend.on("composer_runs", runs));
      renderApp("/");
      const group = await screen.findByRole("region", { name: "Composers" });
      const link = await within(group).findByRole("link", { name: /Release captain/ });
      const row = link.parentElement as HTMLElement;
      await waitFor(() => expect(row).toHaveAttribute("data-tone"));
      return Object.assign(row, { ring: within(row).getByTestId("composer-ring") });
    };
    const run = (status: "idle" | "working" | "blocked") =>
      runView({ session: session({ id: "r1", workspace_id: "cw1", status }) });

    it("has no ring on its circle before its first run, and no border or status colour on the row", async () => {
      const row = await rowOf([]);
      expect(row.ring.className).toContain("border-transparent");
      // The ring touches the circle: its holder has a one pixel padding and nothing more.
      expect(row.ring.parentElement?.className).toContain("p-px");
      expect(row.className).not.toMatch(/(^|\s)(border|ring)(-|\s|$)/);
      expect(row.className).not.toMatch(/bg-(success|warning|muted)/);
      expect(row).toHaveAttribute("data-tone", "new");
    });

    it("has no ring when idle", async () => {
      const row = await rowOf([run("idle")]);
      await waitFor(() => expect(row).toHaveAttribute("data-tone", "idle"));
      expect(row.ring.className).toContain("border-transparent");
    });

    it("has a solid green ring that turns while it works", async () => {
      const row = await rowOf([run("working")]);
      await waitFor(() => expect(row).toHaveAttribute("data-tone", "running"));
      expect(row.className).not.toContain("bg-success");
      expect(row.ring.className).toContain("border-success");
      expect(row.ring.className).toContain("animate-spin");
      expect(row.ring.className).not.toContain("border-dashed");
    });

    it("has a solid yellow ring that pulses while it waits for you", async () => {
      const row = await rowOf([run("blocked")]);
      await waitFor(() => expect(row).toHaveAttribute("data-tone", "waiting"));
      expect(row.className).not.toContain("bg-warning");
      expect(row.ring.className).toContain("border-warning");
      expect(row.ring.className).toContain("animate-pulse");
      expect(row.ring.className).not.toContain("border-dashed");
    });
  });

  it("shows how many things wait for you on the Composers item", async () => {
    seed(() => backend.on("composer_approvals", [approval(), approval({ id: "a2" })]));
    renderApp("/");
    expect(await screen.findByRole("status", { name: "2 waiting for you" })).toHaveTextContent("2");
  });

  it("puts a pinned Composer under Pinned and leaves it out of the group", async () => {
    seed(() => backend.on("composer_list", [composer({ pinned: true })]));
    renderApp("/");
    const pinned = await screen.findByRole("region", { name: "Pinned" });
    expect(within(pinned).getByRole("link", { name: /Release captain/ })).toHaveAttribute(
      "href",
      "/composer/c1",
    );
    expect(screen.queryByRole("region", { name: "Composers" })).not.toBeInTheDocument();
  });

  it("has no plus for another run: a Composer has one chat", async () => {
    seed();
    renderApp("/");
    await screen.findByRole("region", { name: "Composers" });
    expect(screen.queryByRole("button", { name: /New run/ })).not.toBeInTheDocument();
  });

  it("is quiet when there are no Composers", async () => {
    seed(() => backend.on("composer_list", []));
    renderApp("/");
    await screen.findByRole("navigation", { name: "Primary" });
    expect(screen.queryByRole("region", { name: "Composers" })).not.toBeInTheDocument();
  });
});

describe("Composers home", () => {
  it("lists the Composers with what they may do, and the templates", async () => {
    seed();
    renderApp("/composers");
    const card = await within(await screen.findByRole("main")).findByRole("button", {
      name: /Release captain/,
    });
    // A badge says what it is doing, and the card shows what it is for.
    await waitFor(() => expect(card).toHaveTextContent("Idle"));
    expect(card).toHaveTextContent("Keep main green.");
    expect(card).not.toHaveTextContent("On its own");
    const templates = screen.getByRole("region", { name: "Templates" });
    expect(within(templates).getAllByRole("button")).toHaveLength(4);
  });

  it("opens the dialog filled in from a template", async () => {
    seed();
    const app = renderApp("/composers");
    await app.user.click(
      await screen.findByRole("button", { name: /Review all open pull requests/ }),
    );
    const dialog = await screen.findByRole("dialog", { name: "New Composer" });
    expect(within(dialog).getByLabelText("Name")).toHaveValue("PR reviewer");
    // What the template says about its tools is not shown, and it is kept when it is saved.
    expect(within(dialog).queryByText("What it may do")).not.toBeInTheDocument();
    backend.on("composer_create", composer());
    await app.user.click(within(dialog).getByRole("button", { name: "Create Composer" }));
    await waitFor(() => expect(backend.callsTo("composer_create")).toHaveLength(1));
    const { input } = backend.callsTo("composer_create")[0] as {
      input: { autonomy: object; instructions: string };
    };
    expect(input.autonomy).toEqual({ merge_pr: "off", create_pr: "off" });
    expect(input.instructions).toMatch(/pull requests/);
  });

  it("answers what waits for you", async () => {
    seed(() => {
      backend.on("composer_approvals", [approval()]).on("approval_answer", null);
    });
    const app = renderApp("/composers");
    const waiting = await screen.findByRole("region", { name: "Waiting for you" });
    expect(within(waiting).getByText("Release captain")).toBeInTheDocument();
    await app.user.click(within(waiting).getByRole("button", { name: "Allow for this run" }));
    await waitFor(() =>
      expect(backend.callsTo("approval_answer")[0]).toMatchObject({
        id: "a1",
        allow: true,
        forRun: true,
      }),
    );
  });

  it("has no pause, stop or audit buttons here: those are in Settings and the palette", async () => {
    seed();
    renderApp("/composers");
    await screen.findByRole("heading", { name: "Composers" });
    for (const name of ["Pause Composers", "Stop all", "Audit log"]) {
      expect(screen.queryByRole("button", { name })).not.toBeInTheDocument();
      expect(screen.queryByRole("link", { name })).not.toBeInTheDocument();
    }
  });

  it("says so, and where to resume, while Composers are paused", async () => {
    seed(() => backend.on("composers_status", { paused: true, running: 0, waiting_approvals: 0 }));
    renderApp("/composers");
    expect(await screen.findByText(/Composers are paused/)).toBeInTheDocument();
    expect(screen.getByText(/Settings, Composers/)).toBeInTheDocument();
  });

  it("pauses and resumes from the settings", async () => {
    seed(() => backend.on("composers_pause", { paused: true, running: 0, waiting_approvals: 0 }));
    settingsBackend();
    const app = renderApp("/settings/composers");
    await app.user.click(await screen.findByRole("button", { name: "Pause Composers" }));
    await waitFor(() =>
      expect(backend.callsTo("composers_pause")[0]).toMatchObject({ paused: true }),
    );
  });

  it("asks before stopping everything, then does it", async () => {
    seed(() =>
      backend.on("composers_stop_all", { paused: true, running: 0, waiting_approvals: 0 }),
    );
    settingsBackend();
    const app = renderApp("/settings/composers");
    await app.user.click(await screen.findByRole("button", { name: "Stop all" }));
    const dialog = await screen.findByRole("dialog", { name: "Stop every Composer?" });
    expect(backend.callsTo("composers_stop_all")).toHaveLength(0);
    await app.user.click(within(dialog).getByRole("button", { name: "Stop everything" }));
    await waitFor(() => expect(backend.callsTo("composers_stop_all")).toHaveLength(1));
  });

  it("puts the templates first, then the list with a status badge and a search", async () => {
    seed(() => {
      backend
        .on("composer_list", [
          composer({ id: "c1", name: "Release captain" }),
          composer({ id: "c2", name: "Ticket triager", instructions: "Reads Jira." }),
        ])
        .on("composer_runs", (args: Record<string, unknown>) =>
          args.composerId === "c1"
            ? [runView({ session: session({ id: "r1", workspace_id: "cw1", status: "working" }) })]
            : [],
        );
    });
    const app = renderApp("/composers");
    const templates = await screen.findByRole("region", { name: "Templates" });
    const yours = await screen.findByRole("region", { name: "Your Composers" });
    // The templates come before the list.
    expect(
      templates.compareDocumentPosition(yours) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    const captain = within(yours).getByRole("button", { name: /Release captain/ });
    await waitFor(() => expect(captain).toHaveTextContent("Running"));
    expect(within(yours).getByRole("button", { name: /Ticket triager/ })).toHaveTextContent(
      "No runs yet",
    );

    // The search narrows by name and by instructions.
    await app.user.type(within(yours).getByLabelText("Search Composers"), "jira");
    expect(
      within(yours).queryByRole("button", { name: /Release captain/ }),
    ).not.toBeInTheDocument();
    expect(within(yours).getByRole("button", { name: /Ticket triager/ })).toBeInTheDocument();
    await app.user.clear(within(yours).getByLabelText("Search Composers"));
    await app.user.type(within(yours).getByLabelText("Search Composers"), "zzz");
    expect(within(yours).getByText(/No Composer matches/)).toBeInTheDocument();
  });

  it("shows New Composer in the header, on the right of the title", async () => {
    seed();
    const app = renderApp("/composers");
    const heading = await screen.findByRole("heading", { name: "Composers" });
    const button = screen.getByRole("button", { name: /New Composer/ });
    expect(heading.compareDocumentPosition(button) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(heading.closest("header")).toContainElement(button);
    await app.user.click(button);
    expect(await screen.findByRole("dialog", { name: "New Composer" })).toBeInTheDocument();
  });

  it("lists Composers shared in repositories and can start one from them", async () => {
    seed(() => {
      backend
        .on("composer_shared_list", [
          {
            project_id: "p1",
            project_name: "roster",
            file: "reviewer.toml",
            name: "Shared reviewer",
            instructions: "Review.",
            tools: ["list_projects"],
            limits: null,
            error: null,
          },
          {
            project_id: "p1",
            project_name: "roster",
            file: "bad.toml",
            name: "bad",
            instructions: "",
            tools: null,
            limits: null,
            error: "Not a tool.",
          },
        ])
        .on("composer_shared_input", {
          name: "Shared reviewer",
          instructions: "Review.",
          autonomy: { merge_pr: "off" },
          projects: null,
          limits: composer().limits,
          allow_override: false,
          notes: "",
          model: null,
          effort: null,
        });
    });
    const app = renderApp("/composers");
    const region = await screen.findByRole("region", { name: "In your repositories" });
    expect(within(region).getByRole("alert")).toHaveTextContent("Not a tool.");
    const buttons = within(region).getAllByRole("button", { name: "Add to mine" });
    expect(buttons[1]).toBeDisabled();
    await app.user.click(buttons[0] as HTMLElement);
    const dialog = await screen.findByRole("dialog", { name: "New Composer" });
    expect(within(dialog).getByLabelText("Name")).toHaveValue("Shared reviewer");
    backend.on("composer_create", composer());
    await app.user.click(within(dialog).getByRole("button", { name: "Create Composer" }));
    await waitFor(() => expect(backend.callsTo("composer_create")).toHaveLength(1));
    expect(
      (backend.callsTo("composer_create")[0] as { input: { autonomy: object } }).input.autonomy,
    ).toEqual({ merge_pr: "off" });
  });
});

describe("Composer dialog", () => {
  it("asks only for a name and instructions, and creates with the safe defaults", async () => {
    seed(() => backend.on("composer_create", composer({ id: "c2", name: "Helper" })));
    const app = renderApp("/composers");
    await app.user.click(await screen.findByRole("button", { name: "New Composer" }));
    const dialog = await screen.findByRole("dialog", { name: "New Composer" });
    for (const gone of ["What it may do", "Limits for one request", "Projects it may use"]) {
      expect(within(dialog).queryByText(gone)).not.toBeInTheDocument();
    }
    expect(within(dialog).queryByRole("radio")).not.toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Create Composer" })).toBeDisabled();
    await app.user.type(within(dialog).getByLabelText("Name"), "Helper");
    await app.user.type(within(dialog).getByLabelText("Instructions"), "Do the thing.");
    expect(within(dialog).queryByLabelText("Model")).not.toBeInTheDocument();
    expect(within(dialog).queryByLabelText("Effort")).not.toBeInTheDocument();
    await app.user.click(within(dialog).getByRole("button", { name: "Create Composer" }));
    await waitFor(() => expect(backend.callsTo("composer_create")).toHaveLength(1));
    const { input } = backend.callsTo("composer_create")[0] as { input: Record<string, unknown> };
    expect(input).toMatchObject({
      name: "Helper",
      instructions: "Do the thing.",
      model: null,
      effort: null,
      autonomy: {},
      projects: null,
      allow_override: false,
      limits: { max_budget_usd: null },
    });
  });

  it("improves the instructions with Claude, and brings the old text back with Undo", async () => {
    seed(() =>
      backend.on("composer_improve_instructions", "# Goal\n\nGet my tickets and fill them in."),
    );
    const app = renderApp("/composers");
    await app.user.click(await screen.findByRole("button", { name: "New Composer" }));
    const dialog = await screen.findByRole("dialog", { name: "New Composer" });
    expect(within(dialog).getByRole("button", { name: "Improve" })).toBeDisabled();
    await app.user.type(within(dialog).getByLabelText("Instructions"), "get my tickets, fill em");
    await app.user.click(within(dialog).getByRole("button", { name: "Improve" }));
    await waitFor(() =>
      expect(within(dialog).getByLabelText("Instructions")).toHaveValue(
        "# Goal\n\nGet my tickets and fill them in.",
      ),
    );
    expect(backend.callsTo("composer_improve_instructions")).toEqual([
      { text: "get my tickets, fill em", model: null },
    ]);
    await app.user.click(within(dialog).getByRole("button", { name: "Undo" }));
    expect(within(dialog).getByLabelText("Instructions")).toHaveValue("get my tickets, fill em");
    expect(within(dialog).queryByRole("button", { name: "Undo" })).not.toBeInTheDocument();
  });

  it("locks and pulses the instructions while Claude improves them", async () => {
    let finish: (text: string) => void = () => {};
    const answer = new Promise<string>((resolve) => {
      finish = resolve;
    });
    seed(() => backend.on("composer_improve_instructions", () => answer));
    const app = renderApp("/composers");
    await app.user.click(await screen.findByRole("button", { name: "New Composer" }));
    const dialog = await screen.findByRole("dialog", { name: "New Composer" });
    const box = within(dialog).getByLabelText("Instructions");
    expect(box).toBeEnabled();
    await app.user.type(box, "do it");
    await app.user.click(within(dialog).getByRole("button", { name: "Improve" }));
    expect(await within(dialog).findByRole("button", { name: "Improving" })).toBeDisabled();
    expect(box).toBeDisabled();
    expect(box).toHaveClass("animate-pulse");
    await act(async () => finish("# Goal\n\nDo it."));
    await waitFor(() => expect(box).toBeEnabled());
    expect(box).not.toHaveClass("animate-pulse");
    expect(box).toHaveValue("# Goal\n\nDo it.");
  });

  it("leaves the instructions as they were when improving fails", async () => {
    seed(() =>
      backend.fail("composer_improve_instructions", {
        code: "internal",
        message: "The agent could not draft it: Invalid API key",
      }),
    );
    const app = renderApp("/composers");
    await app.user.click(await screen.findByRole("button", { name: "New Composer" }));
    const dialog = await screen.findByRole("dialog", { name: "New Composer" });
    await app.user.type(within(dialog).getByLabelText("Instructions"), "do it");
    await app.user.click(within(dialog).getByRole("button", { name: "Improve" }));
    await waitFor(() => expect(backend.callsTo("composer_improve_instructions")).toHaveLength(1));
    expect(within(dialog).getByLabelText("Instructions")).toHaveValue("do it");
    expect(within(dialog).queryByRole("button", { name: "Undo" })).not.toBeInTheDocument();
    expect(await screen.findByText(/Invalid API key/)).toBeInTheDocument();
  });

  it("puts the mode beside the name, 80% and 20%, and starts in manual", async () => {
    seed(() => backend.on("composer_create", composer()));
    const app = renderApp("/composers");
    await app.user.click(await screen.findByRole("button", { name: "New Composer" }));
    const dialog = await screen.findByRole("dialog", { name: "New Composer" });
    const name = within(dialog).getByLabelText("Name");
    const mode = within(dialog).getByLabelText("Mode");
    expect(name.parentElement?.parentElement).toBe(mode.parentElement?.parentElement);
    expect(name.parentElement?.parentElement?.className).toMatch(/grid-cols-\[4fr_1fr\]/);
    expect(mode).toHaveValue("manual");
    expect([...(mode as HTMLSelectElement).options].map((o) => o.textContent)).toEqual([
      "Ask",
      "Accept edits",
      "Plan",
      "Full access",
    ]);
    await app.user.type(name, "X");
    await app.user.click(within(dialog).getByRole("button", { name: "Create Composer" }));
    await waitFor(() => expect(backend.callsTo("composer_create")).toHaveLength(1));
    expect((backend.callsTo("composer_create")[0] as { input: { mode: string } }).input.mode).toBe(
      "manual",
    );
  });

  it("starts in the permission mode the settings name, like a chat, and says what bypass means", async () => {
    seed(() => {
      settingsBackend({ user: { agents: { permission_mode: "bypass" } } });
      backend.on("composer_create", composer({ mode: "bypass" }));
    });
    const app = renderApp("/composers");
    await app.user.click(await screen.findByRole("button", { name: "New Composer" }));
    const dialog = await screen.findByRole("dialog", { name: "New Composer" });
    await waitFor(() => expect(within(dialog).getByLabelText("Mode")).toHaveValue("bypass"));
    expect(within(dialog).getByLabelText("Mode").className).toMatch(/text-destructive/);
    expect(within(dialog).getByText(/Does everything on its own/)).toBeInTheDocument();
    await app.user.type(within(dialog).getByLabelText("Name"), "Boss");
    await app.user.click(within(dialog).getByRole("button", { name: "Create Composer" }));
    await waitFor(() => expect(backend.callsTo("composer_create")).toHaveLength(1));
    expect((backend.callsTo("composer_create")[0] as { input: { mode: string } }).input.mode).toBe(
      "bypass",
    );
  });

  it("sends the mode that was picked instead of the settings'", async () => {
    seed(() => {
      settingsBackend({ user: { agents: { permission_mode: "bypass" } } });
      backend.on("composer_create", composer());
    });
    const app = renderApp("/composers");
    await app.user.click(await screen.findByRole("button", { name: "New Composer" }));
    const dialog = await screen.findByRole("dialog", { name: "New Composer" });
    await waitFor(() => expect(within(dialog).getByLabelText("Mode")).toHaveValue("bypass"));
    await app.user.selectOptions(within(dialog).getByLabelText("Mode"), "plan");
    expect(within(dialog).getByText(/Only looks and makes a plan/)).toBeInTheDocument();
    await app.user.type(within(dialog).getByLabelText("Name"), "Planner");
    await app.user.click(within(dialog).getByRole("button", { name: "Create Composer" }));
    await waitFor(() => expect(backend.callsTo("composer_create")).toHaveLength(1));
    expect((backend.callsTo("composer_create")[0] as { input: { mode: string } }).input.mode).toBe(
      "plan",
    );
  });

  it("shows the mode of the Composer being edited, not the settings'", async () => {
    seed(() => {
      settingsBackend({ user: { agents: { permission_mode: "bypass" } } });
      backend
        .on("composer_list", [composer({ mode: "accept_edits" })])
        .on("composer_update", composer());
    });
    const app = renderApp("/composer/c1");
    await app.user.click(await screen.findByRole("tab", { name: "Details" }));
    await app.user.click(await screen.findByRole("button", { name: "Composer settings" }));
    const dialog = await screen.findByRole("dialog", { name: /settings/ });
    expect(within(dialog).queryByRole("button", { name: "Edit" })).not.toBeInTheDocument();
    expect(within(dialog).getByLabelText("Mode")).toHaveValue("accept_edits");
    // Nothing changed yet, so there is nothing to save.
    expect(within(dialog).queryByRole("button", { name: "Save" })).not.toBeInTheDocument();
    await app.user.type(within(dialog).getByLabelText("Name"), " 2");
    await app.user.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(backend.callsTo("composer_update")).toHaveLength(1));
    expect((backend.callsTo("composer_update")[0] as { input: { mode: string } }).input.mode).toBe(
      "accept_edits",
    );
  });

  it("edits an existing Composer and keeps what it may do, its limits and its projects", async () => {
    seed(() => {
      backend
        .on("composer_list", [
          composer({
            autonomy: { merge_pr: "allow" },
            projects: ["p1"],
            allow_override: true,
            limits: { ...composer().limits, max_budget_usd: 5 },
          }),
        ])
        .on("composer_update", composer());
    });
    const app = renderApp("/composer/c1");
    await app.user.click(await screen.findByRole("tab", { name: "Details" }));
    await app.user.click(await screen.findByRole("button", { name: "Composer settings" }));
    const dialog = await screen.findByRole("dialog", { name: /settings/ });
    expect(within(dialog).queryByRole("button", { name: "Edit" })).not.toBeInTheDocument();
    expect(within(dialog).getByLabelText("Name")).toHaveValue("Release captain");
    await app.user.clear(within(dialog).getByLabelText("Name"));
    await app.user.type(within(dialog).getByLabelText("Name"), "Captain");
    await app.user.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(backend.callsTo("composer_update")[0]).toMatchObject({ id: "c1" }));
    expect(
      (backend.callsTo("composer_update")[0] as { input: Record<string, unknown> }).input,
    ).toMatchObject({
      name: "Captain",
      autonomy: { merge_pr: "allow" },
      projects: ["p1"],
      allow_override: true,
      limits: { max_budget_usd: 5 },
    });
  });

  it("shows the reason when it cannot be saved", async () => {
    seed(() =>
      backend.fail("composer_create", {
        code: "invalid",
        message: "The spending limit must be more than 0.",
      }),
    );
    const app = renderApp("/composers");
    await app.user.click(await screen.findByRole("button", { name: "New Composer" }));
    const dialog = await screen.findByRole("dialog", { name: "New Composer" });
    await app.user.type(within(dialog).getByLabelText("Name"), "X");
    await app.user.click(within(dialog).getByRole("button", { name: "Create Composer" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "The spending limit must be more than 0.",
    );
  });
});

describe("Composer page", () => {
  beforeEach(() => useChat.setState({ activeTab: {} }));

  it("starts with a box to say what to do, and begins a run with it", async () => {
    seed(() => {
      backend
        .on("composer_runs", [])
        .on("session_list", [])
        .on("composer_run_start", session({ id: "r5", workspace_id: "cw1" }));
    });
    const app = renderApp("/composer/c1");
    expect(await screen.findByRole("heading", { name: "Release captain" })).toBeInTheDocument();
    // No counts of tools. With no integration there is only the name.
    expect(screen.queryByText(/On its own/)).not.toBeInTheDocument();
    expect(screen.queryByRole("list", { name: "Integrations" })).not.toBeInTheDocument();
    // The instructions are not repeated here. Edit has them.
    expect(screen.queryByText(/Keep main green/)).not.toBeInTheDocument();
    await app.user.type(await screen.findByLabelText("What should it do?"), "Triage the open PRs");
    await app.user.click(screen.getByRole("button", { name: "Run" }));
    await waitFor(() => expect(backend.callsTo("composer_run_start")).toHaveLength(1));
    expect(backend.callsTo("composer_run_start")[0]).toMatchObject({
      composerId: "c1",
      message: "Triage the open PRs",
    });
    await waitFor(() => expect(app.router.state.location.pathname).toBe("/composer/c1/run/r5"));
  });

  it("shows the integrations it uses as badges, and how they are doing", async () => {
    const connected = {
      connected: true,
      site: "t.atlassian.net",
      email: null,
      account_name: "Me",
      needs_reconnect: false,
      method: "oauth",
      choose_site: [],
    };
    seed(() => {
      backend
        .on("composer_list", [composer({ integrations: ["jira"] })])
        .on("composer_runs", [])
        .on("session_list", [])
        .on("jira_status", connected);
    });
    renderApp("/composer/c1");
    const list = await screen.findByRole("list", { name: "Integrations" });
    await waitFor(() => expect(list).toHaveTextContent("Jira"));
    await waitFor(() => expect(list).toHaveTextContent("Connected"));
  });

  it("says when an integration it uses needs attention", async () => {
    seed(() => {
      backend
        .on("composer_list", [composer({ integrations: ["jira"] })])
        .on("composer_runs", [])
        .on("session_list", []);
    });
    renderApp("/composer/c1");
    const list = await screen.findByRole("list", { name: "Integrations" });
    await waitFor(() => expect(list).toHaveTextContent("Not connected"));
  });

  it("shows an example first message as the placeholder, and Tab takes it", async () => {
    seed(() => {
      backend
        .on("composer_runs", [])
        .on("session_list", [])
        .on("smart_first_prompt", "List my open Jira tickets and start on the first one");
    });
    const app = renderApp("/composer/c1");
    const box = await screen.findByLabelText("What should it do?");
    await waitFor(() =>
      expect((box as HTMLTextAreaElement).placeholder).toContain("List my open Jira tickets"),
    );
    // It was asked with this Composer's name and instructions, and nothing is typed for the person.
    expect(backend.callsTo("smart_first_prompt")[0]).toEqual({
      name: "Release captain",
      purpose: "Keep main green.",
    });
    expect((box as HTMLTextAreaElement).value).toBe("");
    await app.user.click(box);
    await app.user.keyboard("{Tab}");
    expect((box as HTMLTextAreaElement).value).toBe(
      "List my open Jira tickets and start on the first one",
    );
    expect(backend.callsTo("composer_run_start")).toHaveLength(0);
  });

  it("keeps the usual placeholder when there is no example", async () => {
    seed(() => {
      backend.on("composer_runs", []).on("session_list", []).on("smart_first_prompt", null);
    });
    renderApp("/composer/c1");
    const box = (await screen.findByLabelText("What should it do?")) as HTMLTextAreaElement;
    await waitFor(() => expect(backend.callsTo("smart_first_prompt")).toHaveLength(1));
    expect(box.placeholder).toMatch(/^What should it do\?/);
  });

  it("says when the Composer does not exist", async () => {
    seed(() => backend.on("composer_list", []));
    renderApp("/composer/nope");
    expect(await screen.findByText(/does not exist any more/)).toBeInTheDocument();
  });

  it("shows a run in the usual chat, without a permission mode, and with approvals above the input", async () => {
    seed(() => {
      chatBackend([session({ id: "r1", workspace_id: "cw1", title: "Triage open PRs" })]);
      backend.on("composer_approvals", [approval()]).on("approval_answer", null);
    });
    const app = renderApp("/composer/c1/run/r1");
    expect(await screen.findByLabelText("Message")).toHaveAttribute(
      "placeholder",
      "Tell the Composer what you want done",
    );
    expect(screen.queryByLabelText("Permission mode")).not.toBeInTheDocument();
    const card = await screen.findByRole("group", { name: "Approval for delete workspace" });
    expect(card).toHaveTextContent("deletes or removes something");
    expect(card).toHaveTextContent("w9");
    await app.user.click(within(card).getByRole("button", { name: "Deny" }));
    await waitFor(() =>
      expect(backend.callsTo("approval_answer")[0]).toMatchObject({ id: "a1", allow: false }),
    );
  });

  it("shows the Composer's mode in the chat input and changes it for the Composer", async () => {
    seed(() => {
      chatBackend([session({ id: "r1", workspace_id: "cw1", title: "Triage open PRs" })]);
      backend
        .on("composer_list", [composer({ mode: "bypass" })])
        .on("composer_set_mode", composer({ mode: "plan" }));
    });
    const app = renderApp("/composer/c1/run/r1");
    const mode = await screen.findByLabelText("Mode");
    expect(mode).toHaveValue("bypass");
    expect(mode.className).toMatch(/text-destructive/);
    expect([...(mode as HTMLSelectElement).options].map((o) => o.textContent)).toEqual([
      "Ask",
      "Accept edits",
      "Plan",
      "Full access",
    ]);
    await app.user.selectOptions(mode, "plan");
    await waitFor(() =>
      expect(backend.callsTo("composer_set_mode")[0]).toMatchObject({ id: "c1", mode: "plan" }),
    );
  });

  it("shows its one chat with no tab and no button for another", async () => {
    seed(() => {
      chatBackend([session({ id: "r1", workspace_id: "cw1", title: "Triage open PRs" })]);
    });
    renderApp("/composer/c1/run/r1");
    await screen.findByRole("heading", { name: "Release captain" });
    expect(screen.queryByRole("tablist", { name: "Chats" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /New (run|chat)/ })).not.toBeInTheDocument();
  });

  it("shows tabs only when a schedule or trigger has started another run", async () => {
    seed(() => {
      chatBackend([
        session({ id: "r1", workspace_id: "cw1", title: "Triage open PRs" }),
        session({ id: "r2", workspace_id: "cw1", title: "Nightly check" }),
      ]);
    });
    renderApp("/composer/c1/run/r1");
    expect(await screen.findByRole("tab", { name: /Triage open PRs/ })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /Nightly check/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /New (run|chat)/ })).not.toBeInTheDocument();
  });

  it("shows only the facts on the Details tab, and a Settings button in its footer", async () => {
    seed(() => {
      chatBackend([session({ id: "r1", workspace_id: "cw1", title: "Triage open PRs" })]);
    });
    const app = renderApp("/composer/c1/run/r1");
    await screen.findByRole("heading", { name: "Release captain" });
    await app.user.click(await screen.findByRole("tab", { name: "Details" }));
    expect(await screen.findByRole("button", { name: "Composer settings" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Repositories" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Integrations" })).toBeInTheDocument();
    // None of the configuration is on the page: it is in the dialog.
    for (const name of ["Autonomy", "Limits", "Notes", "Triggers"]) {
      expect(screen.queryByRole("region", { name })).not.toBeInTheDocument();
    }
    expect(screen.queryByRole("button", { name: "Delete Composer" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Edit" })).not.toBeInTheDocument();
    expect(screen.queryByText(/Keep main green/)).not.toBeInTheDocument();
  });

  it("opens a settings dialog with a sidebar of categories, General first", async () => {
    seed(() => {
      chatBackend([session({ id: "r1", workspace_id: "cw1", title: "Triage open PRs" })]);
    });
    const app = renderApp("/composer/c1/run/r1");
    await app.user.click(await screen.findByRole("tab", { name: "Details" }));
    await app.user.click(await screen.findByRole("button", { name: "Composer settings" }));
    const nav = await screen.findByRole("navigation", { name: "Composer settings" });
    const labels = within(nav)
      .getAllByRole("button")
      .map((b) => b.textContent);
    expect(labels).toEqual([
      "General",
      "Autonomy",
      "Limits",
      "Notes",
      "Starts on its own",
      "Delete",
    ]);
    expect(within(nav).getByRole("button", { name: "General" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(screen.getByRole("region", { name: "General" })).toBeInTheDocument();
    // One category at a time.
    expect(screen.queryByRole("region", { name: "Autonomy" })).not.toBeInTheDocument();
    await app.user.click(within(nav).getByRole("button", { name: "Autonomy" }));
    expect(await screen.findByRole("region", { name: "Autonomy" })).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "General" })).not.toBeInTheDocument();
    expect(within(nav).getByRole("button", { name: "Autonomy" })).toHaveAttribute(
      "aria-current",
      "page",
    );
  });

  it("has no closed chats button in the top bar of a Composer, and keeps it for a workspace", async () => {
    seed(() => {
      chatBackend([session({ id: "r1", workspace_id: "cw1", title: "Triage open PRs" })]);
    });
    const composerPage = renderApp("/composer/c1/run/r1");
    await screen.findByRole("heading", { name: "Release captain" });
    expect(screen.queryByRole("button", { name: "Chat history" })).not.toBeInTheDocument();
    composerPage.router.navigate("/workspace/w1");
    expect(await screen.findByRole("button", { name: "Chat history" })).toBeInTheDocument();
  });

  it("lists the integrations on the Details tab, under the title", async () => {
    const connected = {
      connected: true,
      site: "t.atlassian.net",
      email: null,
      account_name: "Me",
      needs_reconnect: false,
      method: "oauth",
      choose_site: [],
    };
    seed(() => {
      chatBackend([session({ id: "r1", workspace_id: "cw1", title: "Triage open PRs" })]);
      backend
        .on("composer_list", [composer({ integrations: ["jira"] })])
        .on("jira_status", connected);
    });
    const app = renderApp("/composer/c1/run/r1");
    await app.user.click(await screen.findByRole("tab", { name: "Details" }));
    const section = await screen.findByRole("region", { name: "Integrations" });
    await waitFor(() => expect(section).toHaveTextContent("Connected"));
    expect(section).toHaveTextContent("Jira");
    expect(section).toHaveTextContent("t.atlassian.net");
    // After the repositories, and before the footer with Settings.
    const settings = screen.getByRole("button", { name: "Composer settings" });
    expect(
      section.compareDocumentPosition(settings) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("lists the repositories a Composer handles, before the integrations", async () => {
    seed(() => {
      chatBackend([session({ id: "r1", workspace_id: "cw1", title: "Triage open PRs" })]);
      backend
        .on("composer_list", [composer({ projects: ["p2"] })])
        .on("project_list", [
          project({ id: "p1", name: "alpha" }),
          project({ id: "p2", name: "beta" }),
        ]);
    });
    const app = renderApp("/composer/c1/run/r1");
    await app.user.click(await screen.findByRole("tab", { name: "Details" }));
    const repos = await screen.findByRole("region", { name: "Repositories" });
    await waitFor(() => expect(repos).toHaveTextContent("beta"));
    expect(repos).toHaveTextContent("Only this repository");
    expect(repos).not.toHaveTextContent("alpha");
    const integrations = screen.getByRole("region", { name: "Integrations" });
    expect(
      repos.compareDocumentPosition(integrations) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("says all repositories when it is not limited, and lists them", async () => {
    seed(() => {
      chatBackend([session({ id: "r1", workspace_id: "cw1", title: "Triage open PRs" })]);
      backend
        .on("composer_list", [composer({ projects: null })])
        .on("project_list", [
          project({ id: "p1", name: "alpha" }),
          project({ id: "p2", name: "beta" }),
        ]);
    });
    const app = renderApp("/composer/c1/run/r1");
    await app.user.click(await screen.findByRole("tab", { name: "Details" }));
    const repos = await screen.findByRole("region", { name: "Repositories" });
    await waitFor(() => expect(repos).toHaveTextContent("alpha"));
    expect(repos).toHaveTextContent("All repositories");
    expect(repos).toHaveTextContent("beta");
  });

  it("says on the Details tab when Jira is off for the Composer", async () => {
    seed(() => {
      chatBackend([session({ id: "r1", workspace_id: "cw1", title: "Triage open PRs" })]);
    });
    const app = renderApp("/composer/c1/run/r1");
    await app.user.click(await screen.findByRole("tab", { name: "Details" }));
    const section = await screen.findByRole("region", { name: "Integrations" });
    expect(section).toHaveTextContent("Off for this Composer");
  });

  it("opens the new Composer's page with the right pane closed and the left one left alone", async () => {
    useUi.setState({ leftOpen: true, rightOpen: true });
    seed(() => backend.on("composer_create", composer({ id: "c7", name: "Fresh" })));
    const app = renderApp("/composers");
    await app.user.click(await screen.findByRole("button", { name: /New Composer/ }));
    const dialog = await screen.findByRole("dialog", { name: "New Composer" });
    await app.user.type(within(dialog).getByLabelText("Name"), "Fresh");
    await app.user.click(within(dialog).getByRole("button", { name: "Create Composer" }));
    await waitFor(() => expect(app.router.state.location.pathname).toBe("/composer/c7"));
    expect(useUi.getState().rightOpen).toBe(false);
    expect(useUi.getState().leftOpen).toBe(true);
  });
});
