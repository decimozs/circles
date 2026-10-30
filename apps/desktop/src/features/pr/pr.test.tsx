import type { Check } from "@circles/bindings/Check";
import type { PrStatus } from "@circles/bindings/PrStatus";
import type { Readiness } from "@circles/bindings/Readiness";
import type { ReviewThread } from "@circles/bindings/ReviewThread";
import { act, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useChat } from "@/stores/chat";
import { backend } from "@/test/backend";
import { prStatus, readiness, workspace } from "@/test/fixtures";
import { defaultBackend, renderApp } from "@/test/render";
import { POLL_IDLE_MS, POLL_OPEN_MS } from "./api";

const check = (name: string, bucket: Check["bucket"], extra: Partial<Check> = {}): Check => ({
  name,
  workflow: "CI",
  bucket,
  state: bucket,
  description: "",
  link: `https://github.com/o/r/actions/runs/1/job/${name}`,
  run_id: 1,
  ...extra,
});
const thread = (id: string, extra: Partial<ReviewThread> = {}): ReviewThread => ({
  id,
  path: "src/a.ts",
  line: 12,
  resolved: false,
  outdated: false,
  comments: [{ author: "alice", body: "Rename this", url: `https://x/${id}` }],
  ...extra,
});

function open(state: Readiness = readiness(), setup: () => void = () => {}) {
  defaultBackend();
  backend
    .on("workspace_get", workspace({ id: "w1", city: "Algiers" }))
    .on("pr_readiness", state)
    .on("pr_draft_seed", {
      title: "Add login",
      body: "- Add login\n- Fix typo",
      base: "main",
    });
  setup();
  return renderApp("/workspace/w1");
}

const nextStep = () => screen.findByRole("region", { name: "Next step" });

beforeEach(() => {
  useChat.setState(useChat.getInitialState(), true);
});

describe("next action in the top bar", () => {
  it("offers to create a pull request for a branch with commits", async () => {
    open();
    expect(
      await within(await nextStep()).findByRole("button", { name: "Create PR" }),
    ).toBeInTheDocument();
  });

  it("shows no bar when there is nothing to do", async () => {
    open(readiness({ next_action: "none", git: { ...readiness().git, ahead: 0 } }));
    await waitFor(() => expect(backend.callsTo("pr_readiness").length).toBeGreaterThan(0));
    await screen.findByRole("tablist", { name: "Review" });
    // With nothing to say, the bar is not there at all.
    expect(screen.queryByRole("region", { name: "Next step" })).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /Create PR|Merge|Fix|Resolve|Respond/ }),
    ).not.toBeInTheDocument();
  });

  it("shows the pull request chip and the ready pill, and starts the merge", async () => {
    const app = open(
      readiness({
        pr: prStatus(),
        next_action: "merge",
        ready: true,
        checks: { total: 2, passed: 2, failed: 0, pending: 0 },
      }),
      () => backend.on("pr_merge", prStatus({ state: "merged" })),
    );
    expect(await within(await nextStep()).findByText("Ready to merge")).toBeInTheDocument();
    expect(
      within(await nextStep()).getByRole("button", { name: "Open pull request 42" }),
    ).toHaveTextContent("#42");
    await app.user.click(within(await nextStep()).getByRole("button", { name: "Merge" }));
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("Merge #42?");
    expect(dialog).toHaveTextContent("Everything looks ready.");
    await app.user.click(within(dialog).getByRole("button", { name: "Merge" }));
    await waitFor(() =>
      expect(backend.callsTo("pr_merge")).toEqual([{ workspaceId: "w1", overrideBlockers: false }]),
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("holds the merge back for open todos until the user chooses to merge anyway", async () => {
    const app = open(
      readiness({
        pr: prStatus(),
        next_action: "merge",
        open_todos: 2,
        blockers: [{ kind: "todos", message: "2 todos are still open.", overridable: true }],
      }),
      () => backend.on("pr_merge", prStatus({ state: "merged" })),
    );
    expect(
      within(await screen.findByRole("banner")).queryByText("Ready to merge"),
    ).not.toBeInTheDocument();
    await app.user.click(await within(await nextStep()).findByRole("button", { name: "Merge" }));
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("2 todos are still open.");
    const merge = within(dialog).getByRole("button", { name: "Merge" });
    expect(merge).toBeDisabled();
    await app.user.click(within(dialog).getByLabelText("Merge anyway"));
    expect(merge).toBeEnabled();
    await app.user.click(merge);
    await waitFor(() =>
      expect(backend.callsTo("pr_merge")).toEqual([{ workspaceId: "w1", overrideBlockers: true }]),
    );
  });

  it("does not let a draft be merged at all", async () => {
    const app = open(
      readiness({
        pr: prStatus({ is_draft: true }),
        next_action: "merge",
        blockers: [
          { kind: "draft", message: "The pull request is still a draft.", overridable: false },
        ],
      }),
    );
    await app.user.click(await within(await nextStep()).findByRole("button", { name: "Merge" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).queryByLabelText("Merge anyway")).not.toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Merge" })).toBeDisabled();
  });

  it("shows why a merge failed", async () => {
    const app = open(readiness({ pr: prStatus(), next_action: "merge", ready: true }), () =>
      backend.fail("pr_merge", { code: "gh", message: "Pull request is not mergeable" }),
    );
    await app.user.click(await within(await nextStep()).findByRole("button", { name: "Merge" }));
    await app.user.click(
      within(await screen.findByRole("dialog")).getByRole("button", { name: "Merge" }),
    );
    expect(await within(screen.getByRole("dialog")).findByRole("alert")).toHaveTextContent(
      "not mergeable",
    );
  });

  it("waits while checks run", async () => {
    open(readiness({ pr: prStatus(), next_action: "wait_for_checks" }));
    expect(
      await within(await nextStep()).findByRole("button", { name: "Checks running…" }),
    ).toBeDisabled();
  });

  it("starts an agent turn to fix failing checks, respond to a review or resolve conflicts", async () => {
    for (const [action, label, command] of [
      ["fix_checks", "Fix failing checks", "agent_fix_checks"],
      ["respond_to_review", "Respond to review", "agent_respond_review"],
      ["resolve_conflicts", "Resolve conflicts", "agent_resolve_conflicts"],
    ] as const) {
      const app = open(readiness({ pr: prStatus(), next_action: action }), () =>
        backend.on(command, null),
      );
      await app.user.click(await within(await nextStep()).findByRole("button", { name: label }));
      await waitFor(() =>
        expect(backend.callsTo(command)).toEqual([{ workspaceId: "w1", sessionId: "s1" }]),
      );
      app.router.navigate("/");
      document.body.innerHTML = "";
    }
  });

  it("offers to archive once the pull request is merged", async () => {
    const app = open(readiness({ pr: prStatus({ state: "merged" }), next_action: "archive" }), () =>
      backend.on("workspace_archive", workspace({ id: "w1", state: "archived" })),
    );
    await app.user.click(await within(await nextStep()).findByRole("button", { name: "Archive" }));
    await waitFor(() => expect(backend.callsTo("workspace_archive")).toHaveLength(1));
  });

  it("says GitHub is unavailable, without suggesting a pull request", async () => {
    open(
      readiness({
        next_action: "none",
        gh_error: {
          kind: "network",
          message: "Could not reach GitHub. Check your connection.",
          fix: null,
        },
      }),
    );
    expect(await within(await nextStep()).findByText("GitHub unavailable")).toBeInTheDocument();
    expect(
      within(await nextStep()).queryByRole("button", { name: "Create PR" }),
    ).not.toBeInTheDocument();
  });
});

describe("polling", () => {
  afterEach(() => vi.useRealTimers());

  it("asks again every minute while a pull request is open", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    open(readiness({ pr: prStatus(), next_action: "wait_for_checks" }));
    await waitFor(() => expect(backend.callsTo("pr_readiness")).toHaveLength(1));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(POLL_OPEN_MS + 1000);
    });
    await waitFor(() => expect(backend.callsTo("pr_readiness").length).toBeGreaterThanOrEqual(2));
  });

  it("asks less often while there is no pull request", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    open(readiness());
    await waitFor(() => expect(backend.callsTo("pr_readiness")).toHaveLength(1));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(POLL_OPEN_MS + 1000);
    });
    expect(backend.callsTo("pr_readiness")).toHaveLength(1);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(POLL_IDLE_MS);
    });
    await waitFor(() => expect(backend.callsTo("pr_readiness").length).toBeGreaterThanOrEqual(2));
  });
});

describe("pull request badge", () => {
  const cases: [string, Partial<Readiness>, Partial<PrStatus>, string, string][] = [
    ["Open", { next_action: "wait_for_checks" as const }, {}, "bg-success/10", "bg-success/20"],
    [
      "Ready to merge",
      { next_action: "merge" as const, ready: true },
      {},
      "bg-success/10",
      "bg-success/20",
    ],
    ["Draft", { next_action: "none" as const }, { is_draft: true }, "bg-muted/40", "bg-muted"],
    [
      "Merged",
      { next_action: "archive" as const },
      { state: "merged" as const },
      "bg-info/10",
      "bg-info/20",
    ],
    [
      "Closed",
      { next_action: "none" as const },
      { state: "closed" as const },
      "bg-destructive/10",
      "bg-destructive/20",
    ],
  ];

  for (const [label, state, pr, row, badge] of cases) {
    it(`says ${label} in a badge with its own colour, and tints the row to match`, async () => {
      open(readiness({ ...state, pr: prStatus(pr) }));
      const bar = await nextStep();
      const shown = within(bar).getByTestId("pr-state");
      expect(shown).toHaveTextContent(label);
      expect(shown).toHaveClass(badge);
      expect(bar).toHaveClass(row);
      // The number and the badge share the colour.
      expect(within(bar).getByRole("button", { name: /Open pull request/ })).toHaveClass(badge);
    });
  }

  it("has the pull request number, then its badge, on the left and the action on the right", async () => {
    open(readiness({ pr: prStatus(), next_action: "merge", ready: true }));
    const bar = await nextStep();
    const chip = within(bar).getByRole("button", { name: "Open pull request 42" });
    const badge = within(bar).getByTestId("pr-state");
    const action = within(bar).getByRole("button", { name: "Merge" });
    const order = [chip, badge, action].map((el) =>
      Array.from(bar.querySelectorAll("*")).indexOf(el),
    );
    expect(order).toEqual([...order].sort((a, b) => a - b));
    expect(badge.parentElement).toBe(bar);
    expect(chip.parentElement).toBe(bar);
    expect(action.parentElement).toHaveClass("ml-auto");
  });

  it("has no badge before there is a pull request, and the step to make one is violet", async () => {
    open(readiness());
    const bar = await nextStep();
    expect(within(bar).queryByTestId("pr-state")).not.toBeInTheDocument();
    const create = within(bar).getByRole("button", { name: "Create PR" });
    expect(create.className).toMatch(/bg-violet\/20/);
    expect(bar.className).toMatch(/bg-violet\/10/);
  });
});

describe("create pull request", () => {
  async function openDialog(app: ReturnType<typeof open>) {
    await app.user.click(
      await within(await nextStep()).findByRole("button", { name: "Create PR" }),
    );
    return screen.findByRole("dialog");
  }

  it("starts from the commits and creates the pull request", async () => {
    const app = open(readiness(), () => backend.on("pr_create", prStatus({ number: 7 })));
    const dialog = await openDialog(app);
    await waitFor(() => expect(within(dialog).getByLabelText("Title")).toHaveValue("Add login"));
    expect(within(dialog).getByLabelText("Description")).toHaveValue("- Add login\n- Fix typo");

    await app.user.clear(within(dialog).getByLabelText("Title"));
    await app.user.type(within(dialog).getByLabelText("Title"), "Add the login page");
    await app.user.click(within(dialog).getByLabelText("Open as a draft"));
    backend.on(
      "pr_readiness",
      readiness({ pr: prStatus({ number: 7 }), next_action: "wait_for_checks" }),
    );
    await app.user.click(within(dialog).getByRole("button", { name: "Create pull request" }));
    await waitFor(() => expect(backend.callsTo("pr_create")).toHaveLength(1));
    expect(backend.callsTo("pr_create")[0]).toEqual({
      workspaceId: "w1",
      title: "Add the login page",
      body: "- Add login\n- Fix typo",
      draft: true,
    });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(
      await within(await nextStep()).findByRole("button", { name: "Open pull request 7" }),
    ).toBeInTheDocument();
  });

  it("previews the description as markdown and goes back to editing it", async () => {
    const app = open(readiness(), () => backend.on("pr_create", prStatus({ number: 7 })));
    const dialog = await openDialog(app);
    const box = within(dialog).getByLabelText("Description");
    await waitFor(() => expect(box).toHaveValue("- Add login\n- Fix typo"));
    await app.user.clear(box);
    await app.user.type(
      box,
      "## What changed{Enter}{Enter}Added a **login** form{Enter}{Enter}- one{Enter}- two",
    );

    await app.user.click(within(dialog).getByRole("button", { name: "Preview" }));
    const preview = within(dialog).getByRole("region", { name: "Description preview" });
    expect(within(preview).getByRole("heading", { name: "What changed" })).toBeInTheDocument();
    expect(within(preview).getByText("login").tagName).toBe("STRONG");
    expect(within(preview).getAllByRole("listitem")).toHaveLength(2);
    expect(within(dialog).queryByLabelText("Description")).not.toBeInTheDocument();

    await app.user.click(within(dialog).getByRole("button", { name: "Write" }));
    expect(within(dialog).getByLabelText("Description")).toHaveValue(
      "## What changed\n\nAdded a **login** form\n\n- one\n- two",
    );

    // What is created is the markdown text itself.
    await app.user.click(within(dialog).getByRole("button", { name: "Create pull request" }));
    await waitFor(() => expect(backend.callsTo("pr_create")).toHaveLength(1));
    expect(backend.callsTo("pr_create")[0].body).toBe(
      "## What changed\n\nAdded a **login** form\n\n- one\n- two",
    );
  });

  it("says so when there is nothing to preview", async () => {
    const app = open(readiness());
    backend.on("pr_draft_seed", { title: "T", body: "", base: "main" });
    const dialog = await openDialog(app);
    await waitFor(() => expect(within(dialog).getByLabelText("Title")).toHaveValue("T"));
    await app.user.click(within(dialog).getByRole("button", { name: "Preview" }));
    expect(within(dialog).getByText("Nothing to preview yet.")).toBeInTheDocument();
  });

  it("shows no warning for a branch without commits, and does not allow creating", async () => {
    const app = open(readiness({ git: { ...readiness().git, ahead: 0 }, next_action: "none" }));
    await app.user.keyboard("{Meta>}{Shift>}p{/Shift}{/Meta}");
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findByLabelText("Title");
    expect(within(dialog).queryByRole("status")).not.toBeInTheDocument();
    expect(dialog).not.toHaveTextContent(/no commits|Nothing is committed/);
    expect(within(dialog).getByRole("button", { name: "Create pull request" })).toBeDisabled();
  });

  it("opens with the shortcut and needs a title", async () => {
    const app = open();
    await within(await nextStep()).findByRole("button", { name: "Create PR" });
    await app.user.keyboard("{Meta>}{Shift>}p{/Shift}{/Meta}");
    const dialog = await screen.findByRole("dialog");
    await app.user.clear(within(dialog).getByLabelText("Title"));
    expect(within(dialog).getByRole("button", { name: "Create pull request" })).toBeDisabled();
  });

  it("shows why creating failed and keeps what was typed", async () => {
    const app = open(readiness(), () =>
      backend.fail("pr_create", { code: "git", message: "Could not push: permission denied" }),
    );
    const dialog = await openDialog(app);
    await waitFor(() => expect(within(dialog).getByLabelText("Title")).toHaveValue("Add login"));
    await app.user.click(within(dialog).getByRole("button", { name: "Create pull request" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("permission denied");
    expect(within(dialog).getByLabelText("Title")).toHaveValue("Add login");
  });

  it("lets Claude draft the title and description in the dialog, without making a chat", async () => {
    let finish: (draft: { title: string; body: string }) => void = () => {};
    const app = open(readiness(), () => {
      backend.on(
        "pr_draft",
        () =>
          new Promise((resolve) => {
            finish = resolve;
          }),
      );
    });
    const dialog = await openDialog(app);
    await waitFor(() => expect(within(dialog).getByLabelText("Title")).toHaveValue("Add login"));
    await app.user.click(within(dialog).getByRole("button", { name: "Draft with Agent" }));
    expect(within(dialog).getByRole("button", { name: "Agent is drafting…" })).toBeDisabled();
    expect(within(dialog).getByRole("button", { name: "Create pull request" })).toBeDisabled();

    await act(async () => finish({ title: "Add the login page", body: "A form and a route." }));
    await waitFor(() =>
      expect(within(dialog).getByLabelText("Title")).toHaveValue("Add the login page"),
    );
    expect(within(dialog).getByLabelText("Description")).toHaveValue("A form and a route.");
    expect(backend.callsTo("pr_draft")).toEqual([{ workspaceId: "w1", model: expect.any(String) }]);
    // Nothing in the chat changed.
    expect(backend.callsTo("session_create")).toHaveLength(0);
    expect(backend.callsTo("session_send")).toHaveLength(0);
    expect(screen.queryByText("PR draft")).not.toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Draft with Agent" })).toBeEnabled();
  });

  it("keeps what was typed and says why when drafting fails", async () => {
    const app = open(readiness(), () =>
      backend.fail("pr_draft", {
        code: "claude_missing",
        message: "Claude Code (`claude`) was not found.",
      }),
    );
    const dialog = await openDialog(app);
    await waitFor(() => expect(within(dialog).getByLabelText("Title")).toHaveValue("Add login"));
    await app.user.click(within(dialog).getByRole("button", { name: "Draft with Agent" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("was not found");
    expect(within(dialog).getByLabelText("Title")).toHaveValue("Add login");
    expect(within(dialog).getByRole("button", { name: "Create pull request" })).toBeEnabled();
  });
});

describe("commit before the pull request", () => {
  const dirty = () =>
    readiness({
      git: { ...readiness().git, ahead: 0, dirty: true, unstaged: 2 },
      next_action: "create_pr",
    });
  const setup = () => {
    backend.on("changes_list", [
      { path: "src/a.ts", old_path: null, status: "modified", added: 5, removed: 1, binary: false },
      { path: "docs.md", old_path: null, status: "added", added: 3, removed: 0, binary: false },
    ]);
  };
  const made = [
    { sha: "abc1234", message: "Add a", files: ["src/a.ts"] },
    { sha: "def5678", message: "Document it", files: ["docs.md"] },
  ];

  it("offers Create PR for uncommitted work and shows the commit dialog first", async () => {
    const app = open(dirty(), setup);
    await app.user.click(
      await within(await nextStep()).findByRole("button", { name: "Create PR" }),
    );
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("Commit your changes");
    expect(dialog).not.toHaveTextContent("Create pull request");
    const files = await within(dialog).findByRole("list", { name: "Changed files" });
    expect(files).toHaveTextContent("src/a.ts");
    expect(files).toHaveTextContent("docs.md");
    expect(within(dialog).getByText("2 files changed")).toBeInTheDocument();
  });

  it("shows a loading circle while Claude makes the commits, then opens the pull request dialog", async () => {
    let finish: (commits: typeof made) => void = () => {};
    const app = open(dirty(), () => {
      setup();
      backend.on(
        "commit_with_claude",
        () =>
          new Promise((resolve) => {
            finish = resolve;
          }),
      );
    });
    await app.user.keyboard("{Meta>}{Shift>}p{/Shift}{/Meta}");
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findByRole("list", { name: "Changed files" });
    await app.user.click(within(dialog).getByRole("button", { name: "Commit with Agent" }));

    expect(await within(dialog).findByRole("status")).toHaveTextContent("Making your commits");
    expect(
      within(dialog).queryByRole("button", { name: "Commit with Agent" }),
    ).not.toBeInTheDocument();
    expect(within(dialog).queryByRole("list", { name: "Changed files" })).not.toBeInTheDocument();

    backend.on("pr_readiness", readiness({ git: { ...readiness().git, ahead: 2 } }));
    await act(async () => finish(made));
    await waitFor(() =>
      expect(screen.getByRole("dialog")).toHaveTextContent("Create pull request"),
    );
    expect(screen.getByRole("dialog")).not.toHaveTextContent("Commit your changes");
    expect(backend.callsTo("commit_with_claude")).toEqual([
      { workspaceId: "w1", model: expect.any(String) },
    ]);
  });

  it("can put everything in one commit with a message", async () => {
    const app = open(dirty(), () => {
      setup();
      backend.on("commit_everything", [made[0]]);
    });
    await app.user.keyboard("{Meta>}{Shift>}p{/Shift}{/Meta}");
    const dialog = await screen.findByRole("dialog");
    await app.user.click(
      await within(dialog).findByRole("button", { name: "Choose how to commit" }),
    );
    await app.user.click(await screen.findByRole("menuitemradio", { name: "One commit" }));
    await app.user.click(within(dialog).getByRole("button", { name: "One commit" }));
    const commit = within(dialog).getByRole("button", { name: "Commit everything" });
    expect(commit).toBeDisabled();
    await app.user.type(within(dialog).getByLabelText("Commit message"), "Add pagination");
    await app.user.click(commit);
    await waitFor(() =>
      expect(backend.callsTo("commit_everything")).toEqual([
        { workspaceId: "w1", message: "Add pagination" },
      ]),
    );
    await waitFor(() =>
      expect(screen.getByRole("dialog")).toHaveTextContent("Create pull request"),
    );
  });

  it("shows what went wrong and lets the user try another way", async () => {
    const app = open(dirty(), () => {
      setup();
      backend.fail("commit_with_claude", {
        code: "invalid",
        message:
          "The agent did not answer in the expected form. Commit everything at once instead.",
      });
    });
    await app.user.keyboard("{Meta>}{Shift>}p{/Shift}{/Meta}");
    const dialog = await screen.findByRole("dialog");
    await app.user.click(await within(dialog).findByRole("button", { name: "Commit with Agent" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("Commit everything at once");
    expect(within(dialog).getByRole("list", { name: "Changed files" })).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Choose how to commit" })).toBeEnabled();
    expect(dialog).toHaveTextContent("Commit your changes");
  });

  it("commits with the agent by default, and one commit is a choice from the same button", async () => {
    const app = open(dirty(), setup);
    await app.user.keyboard("{Meta>}{Shift>}p{/Shift}{/Meta}");
    const dialog = await screen.findByRole("dialog");
    expect(await within(dialog).findByRole("button", { name: "Commit with Agent" })).toBeEnabled();
    expect(within(dialog).queryByRole("button", { name: "One commit" })).not.toBeInTheDocument();
    await app.user.click(within(dialog).getByRole("button", { name: "Choose how to commit" }));
    expect(await screen.findByRole("menuitemradio", { name: "Commit with Agent" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    await app.user.click(screen.getByRole("menuitemradio", { name: "One commit" }));
    expect(await within(dialog).findByRole("button", { name: "One commit" })).toBeInTheDocument();
    expect(
      within(dialog).queryByRole("button", { name: "Commit with Agent" }),
    ).not.toBeInTheDocument();
  });

  it("goes straight to the pull request when there are commits", async () => {
    const app = open(readiness(), setup);
    await app.user.keyboard("{Meta>}{Shift>}p{/Shift}{/Meta}");
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("Create pull request");
    expect(dialog).not.toHaveTextContent("Commit your changes");
  });
});

describe("checks tab", () => {
  const withPr = (extra: Partial<Readiness> = {}) =>
    readiness({
      pr: prStatus({ review_decision: "changes_requested" }),
      next_action: "fix_checks",
      checks: { total: 3, passed: 1, failed: 1, pending: 1 },
      ...extra,
    });
  const setup = () => {
    backend.on("pr_checks", [
      check("build", "fail"),
      check("lint", "pass"),
      check("e2e", "pending"),
    ]);
    backend.on("pr_threads", [
      thread("T1"),
      thread("T2", { resolved: true, comments: [{ author: "bob", body: "Done", url: "u2" }] }),
    ]);
    backend.on("pr_deployments", [
      {
        environment: "preview",
        state: "in_progress",
        url: "https://preview.example.com",
        created_at: "2026-01-01",
      },
    ]);
    backend.on("pr_thread_resolve", null);
  };

  async function checksTab(app: ReturnType<typeof open>) {
    await app.user.click(await screen.findByRole("tab", { name: "Checks" }));
  }

  it("shows git, the pull request, checks, deployments and review threads", async () => {
    const app = open(withPr(), setup);
    await checksTab(app);
    const git = await screen.findByRole("region", { name: "Git" });
    expect(git).toHaveTextContent("me/abc12");
    expect(git).toHaveTextContent("2 ahead, 0 behind main");
    expect(git).toHaveTextContent("Clean");
    expect(git).toHaveTextContent("Not yet");

    const pr = screen.getByRole("region", { name: "Pull request" });
    expect(pr).toHaveTextContent("#42 Add login");
    expect(pr).toHaveTextContent("changes requested");

    const checks = await screen.findByRole("region", { name: "Checks" });
    expect(checks).toHaveTextContent("1 of 3 passed");
    expect(within(checks).getByLabelText("Failed")).toBeInTheDocument();
    expect(within(checks).getByLabelText("Passed")).toBeInTheDocument();
    expect(within(checks).getByLabelText("Running")).toBeInTheDocument();
    expect(within(checks).getByText("CI / build")).toBeInTheDocument();

    expect(await screen.findByRole("region", { name: "Deployments" })).toHaveTextContent("preview");
    expect(screen.getByRole("region", { name: "Review threads" })).toHaveTextContent(
      "1 unresolved",
    );
  });

  it("shows uncommitted work and conflicts in the git section", async () => {
    const app = open(
      readiness({
        git: {
          ...readiness().git,
          dirty: true,
          staged: 1,
          unstaged: 2,
          untracked: 3,
          conflicts_with_base: true,
          pushed: true,
        },
      }),
    );
    await checksTab(app);
    const git = await screen.findByRole("region", { name: "Git" });
    expect(git).toHaveTextContent("1 staged, 2 changed, 3 new");
    expect(git).toHaveTextContent("Conflicts");
    expect(git).toHaveTextContent("with main");
    expect(screen.getByRole("region", { name: "Pull request" })).toHaveTextContent(
      "No pull request yet.",
    );
    expect(screen.queryByRole("region", { name: "Checks" })).not.toBeInTheDocument();
  });

  it("resolves and reopens a review thread", async () => {
    const app = open(withPr(), setup);
    await checksTab(app);
    const threads = await screen.findByRole("region", { name: "Review threads" });
    await within(threads).findByText("Rename this");
    await app.user.click(within(threads).getByRole("button", { name: "Resolve" }));
    expect(backend.callsTo("pr_thread_resolve")[0]).toEqual({
      workspaceId: "w1",
      threadId: "T1",
      resolved: true,
    });
    await app.user.click(within(threads).getByRole("button", { name: "Reopen" }));
    expect(backend.callsTo("pr_thread_resolve")[1]).toEqual({
      workspaceId: "w1",
      threadId: "T2",
      resolved: false,
    });
  });

  it("explains a GitHub problem with its fix, and tries again", async () => {
    const app = open(
      readiness({
        gh_error: {
          kind: "not_authenticated",
          message: "The GitHub CLI is not signed in. Run `gh auth login` in a terminal.",
          fix: "gh auth login",
        },
      }),
    );
    await checksTab(app);
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("not signed in");
    expect(within(alert).getByText("gh auth login")).toBeInTheDocument();
    const calls = backend.callsTo("pr_readiness").length;
    await app.user.click(within(alert).getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(backend.callsTo("pr_readiness").length).toBeGreaterThan(calls));
    expect(screen.getByRole("region", { name: "Pull request" })).toHaveTextContent(
      "Could not look for a pull request.",
    );
  });

  it("works offline: git is shown and the GitHub parts say so", async () => {
    const app = open(
      readiness({
        pr: prStatus(),
        gh_error: {
          kind: "network",
          message: "Could not reach GitHub. Check your connection.",
          fix: null,
        },
      }),
      () => {
        backend.fail("pr_checks", {
          code: "gh",
          message: "Could not reach GitHub. Check your connection.",
        });
        backend.fail("pr_deployments", { code: "gh", message: "Could not reach GitHub." });
        backend.fail("pr_threads", {
          code: "gh",
          message: "Could not reach GitHub. Check your connection.",
        });
      },
    );
    await checksTab(app);
    expect(await screen.findByRole("region", { name: "Git" })).toHaveTextContent("me/abc12");
    expect(await screen.findByText("Deployments are not available.")).toBeInTheDocument();
    expect(
      within(await screen.findByRole("region", { name: "Checks" })).getByRole("alert"),
    ).toHaveTextContent("Could not reach GitHub");
  });
});
