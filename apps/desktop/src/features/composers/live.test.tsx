import { act, renderHook, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useNow } from "@/hooks/use-now";
import { useChat } from "@/stores/chat";
import { backend } from "@/test/backend";
import { composer, runView, session, workspace } from "@/test/fixtures";
import { chatBackend, defaultBackend, renderApp } from "@/test/render";
import { groupFleet } from "./composer-panel";
import { liveRows, runningCount } from "./live";

type Chat = "working" | "idle" | "blocked";

function item(
  id: string,
  title: string,
  status: Chat,
  over: Partial<ReturnType<typeof session>> = {},
) {
  return {
    child: {
      id: `f-${id}`,
      composer_id: "c1",
      session_id: "r1",
      workspace_id: `w-${id}`,
      chat_id: `k-${id}`,
      created_at: 1,
    },
    workspace: workspace({ id: `w-${id}`, city: title, title, updated_at: 1_000 }),
    chat: session({ id: `k-${id}`, workspace_id: `w-${id}`, status, ...over }),
  };
}

describe("the rows of the Fleet section", () => {
  it("puts what waits for you first, then what works, the longest going first, then the rest", () => {
    const rows = liveRows(
      groupFleet([
        item("idle", "Docs", "idle", { turns: 2, updated_at: 5_000 }),
        item("new", "Newer", "working", { working_since: 9_000 }),
        item("old", "Older", "working", { working_since: 2_000 }),
        item("ask", "Asks", "blocked", { working_since: 4_000 }),
      ]),
    );
    expect(rows.map((r) => [r.name, r.state])).toEqual([
      ["Asks", "waiting"],
      ["Older", "working"],
      ["Newer", "working"],
      ["Docs", "idle"],
    ]);
    expect(rows[1]?.since).toBe(2_000);
    expect(rows[3]?.done).toBe(true);
    expect(runningCount(rows)).toBe(3);
  });

  it("falls back to the last change for a chat with no start time, and counts none for idle ones", () => {
    const rows = liveRows(groupFleet([item("a", "A", "working", { updated_at: 7_000 })]));
    expect(rows[0]?.since).toBe(7_000);
    const idle = liveRows(groupFleet([item("b", "B", "idle")]));
    expect(idle[0]?.since).toBeNull();
    expect(runningCount(idle)).toBe(0);
    expect(liveRows([])).toEqual([]);
  });

  it("starts from the earliest of several agents in one workspace", () => {
    const first = item("x", "X", "working", { id: "k1", working_since: 8_000 });
    const second = { ...item("x", "X", "working", { id: "k2", working_since: 3_000 }) };
    const rows = liveRows(groupFleet([first, second]));
    expect(rows).toHaveLength(1);
    expect(rows[0]?.since).toBe(3_000);
  });
});

describe("the clock", () => {
  beforeEach(() => vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "Date"] }));
  afterEach(() => vi.useRealTimers());

  it("moves on every second while active, and does nothing while not", () => {
    vi.setSystemTime(new Date(2026, 9, 31, 12, 0, 0));
    const { result, rerender } = renderHook(({ active }) => useNow(active), {
      initialProps: { active: true },
    });
    const start = result.current;
    act(() => vi.advanceTimersByTime(3000));
    expect(result.current - start).toBeGreaterThanOrEqual(3000);

    rerender({ active: false });
    const stopped = result.current;
    act(() => vi.advanceTimersByTime(10_000));
    expect(result.current).toBe(stopped);
    expect(vi.getTimerCount()).toBe(0);
  });
});

function seed(items: ReturnType<typeof item>[]) {
  defaultBackend();
  backend
    .on("composer_list", [composer()])
    .on("composer_runs", [runView()])
    .on("composer_fleet", items)
    .on("composer_approvals", []);
  chatBackend([session({ id: "r1", workspace_id: "cw1", title: "Triage open PRs" })]);
  useChat.setState({ activeTab: {} });
}

async function openPanel() {
  const app = renderApp("/composer/c1/run/r1");
  await app.user.click(await screen.findByRole("button", { name: "Run details" }));
  return { ...app, panel: await screen.findByRole("dialog", { name: "This run" }) };
}

describe("the Fleet section of the run panel", () => {
  beforeEach(() => localStorage.clear());

  it("counts what runs, shows each working agent with its time, and folds the rest away", async () => {
    const now = Date.now();
    seed([
      item("a", "Add dark mode", "working", { working_since: now - 19_000 }),
      item("b", "Fix login", "blocked", { working_since: now - 125_000 }),
      item("c", "Docs", "idle", { turns: 3, updated_at: now - 4 * 60_000 }),
    ]);
    const { panel, user } = await openPanel();
    const fleet = within(await within(panel).findByRole("region", { name: "Fleet" }));
    expect(fleet.getByRole("status")).toHaveTextContent("· 2 running");
    const rows = fleet.getAllByRole("listitem");
    // What waits for you comes first, with its time.
    expect(rows[0]).toHaveTextContent("Fix login");
    expect(rows[0]).toHaveTextContent("Waiting for you");
    expect(rows[0]).toHaveTextContent(/2m 0[5-7]s/);
    expect(rows[1]).toHaveTextContent("Add dark mode");
    expect(rows[1]).toHaveTextContent("Working");
    expect(rows[1]).toHaveTextContent(/^.*(19|20|21)s$/);
    // The finished one is folded under a count until asked for.
    expect(fleet.queryByText("Docs")).not.toBeInTheDocument();
    await user.click(fleet.getByRole("button", { name: /Done 1/ }));
    const done = within(fleet.getByRole("list", { name: "Finished workspaces" }));
    expect(done.getByText("Docs")).toBeInTheDocument();
    expect(done.getByText("Done")).toBeInTheDocument();
    expect(done.getByText("4min ago")).toBeInTheDocument();
  });

  it("shows everything, not folded, when nothing runs", async () => {
    seed([item("c", "Docs", "idle", { turns: 3, updated_at: Date.now() - 60_000 })]);
    const { panel } = await openPanel();
    const fleet = within(await within(panel).findByRole("region", { name: "Fleet" }));
    expect(fleet.getByRole("status")).toHaveTextContent("· nothing running");
    expect(fleet.getByText("Docs")).toBeInTheDocument();
    expect(fleet.queryByRole("button", { name: /Done/ })).not.toBeInTheDocument();
  });

  it("says when the run has started nothing", async () => {
    seed([]);
    const { panel } = await openPanel();
    expect(
      await within(panel).findByText("This run has not started any workspace yet."),
    ).toBeInTheDocument();
    expect(within(panel).getByRole("status")).toHaveTextContent("· nothing started");
  });

  it("opens the workspace of a row", async () => {
    seed([item("a", "Add dark mode", "working", { working_since: Date.now() - 5000 })]);
    const { panel, user } = await openPanel();
    await user.click(await within(panel).findByRole("button", { name: "Open Add dark mode" }));
    await waitFor(() =>
      expect(backend.callsTo("workspace_get").some((c) => c.id === "w-a")).toBe(true),
    );
  });

  it("shows a spinner on the button while anything runs, also with the panel closed", async () => {
    seed([item("a", "Add dark mode", "working", { working_since: Date.now() - 5000 })]);
    renderApp("/composer/c1/run/r1");
    const toggle = await screen.findByRole("button", { name: "Run details" });
    expect(await within(toggle).findByRole("status", { name: "1 running" })).toBeInTheDocument();
  });

  it("shows no spinner when nothing runs, and no Steps anywhere in the panel", async () => {
    seed([item("c", "Docs", "idle", { turns: 1 })]);
    const { panel } = await openPanel();
    await within(panel).findByRole("region", { name: "Fleet" });
    const toggle = screen.getByRole("button", { name: "Run details" });
    expect(within(toggle).queryByRole("status", { name: /running/ })).not.toBeInTheDocument();
    expect(within(panel).queryByText(/Steps/)).not.toBeInTheDocument();
    for (const name of ["Fleet graph", "Approvals"]) {
      expect(
        within(panel).getByRole("button", { name: new RegExp(`^${name}`) }),
      ).toBeInTheDocument();
    }
    // There is no Details link: the repositories, integrations and Settings are in the panel.
    expect(within(panel).queryByRole("button", { name: /^Details/ })).not.toBeInTheDocument();
    expect(within(panel).getByRole("region", { name: "Repositories" })).toBeInTheDocument();
    expect(within(panel).getByRole("region", { name: "Integrations" })).toBeInTheDocument();
    expect(within(panel).getByRole("button", { name: "Composer settings" })).toBeInTheDocument();
  });
});
