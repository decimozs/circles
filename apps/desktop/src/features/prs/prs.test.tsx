import type { PrList } from "@circles/bindings/PrList";
import type { PrRow } from "@circles/bindings/PrRow";
import { screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { backend, emitBackendEvent } from "@/test/backend";
import { project, workspace } from "@/test/fixtures";
import { defaultBackend, renderApp } from "@/test/render";
import { groupRows, matches, NO_FILTERS, parseQuery, sortRows } from "./query";
import { usePrs } from "./store";

function row(over: Partial<PrRow> = {}): PrRow {
  return {
    project_id: "p1",
    project_name: "backend",
    repo: "acme/backend",
    number: 1,
    title: "Add login",
    url: "https://github.com/acme/backend/pull/1",
    author: "ana",
    author_avatar: "https://github.com/ana.png?size=64",
    head_ref: "feat/login",
    base_ref: "main",
    state: "open",
    is_draft: false,
    additions: 10,
    deletions: 2,
    changed_files: 1,
    created_at: "2026-10-01T00:00:00Z",
    updated_at: "2026-10-02T00:00:00Z",
    labels: [],
    review: "none",
    requested_reviewers: [],
    checks: { state: "passing", passed: 3, total: 3 },
    by_me: false,
    review_requested: false,
    ...over,
  };
}

describe("searching the list", () => {
  it("understands words, numbers and key:value terms, with quotes", () => {
    expect(parseQuery('login #12 label:"good first issue" author:Ana is:draft')).toEqual({
      words: ["login"],
      numbers: [12],
      label: ["good first issue"],
      author: ["ana"],
      repo: [],
      branch: [],
      is: ["draft"],
      review: [],
      checks: [],
    });
    // An unknown key is just a word, and a key with nothing after it too.
    expect(parseQuery("foo:bar label:").words).toEqual(["foo:bar", "label:"]);
  });

  it("keeps only rows that satisfy every term", () => {
    const a = row({ number: 1, title: "Fix login", labels: [{ name: "bug", color: "d73a4a" }] });
    const b = row({
      number: 2,
      title: "Add search",
      author: "ben",
      is_draft: true,
      head_ref: "fix/search",
    });
    const none = NO_FILTERS;
    const hits = (q: string) =>
      [a, b].filter((r) => matches(r, parseQuery(q), none)).map((r) => r.number);
    expect(hits("")).toEqual([1, 2]);
    expect(hits("login")).toEqual([1]);
    expect(hits("label:bug")).toEqual([1]);
    expect(hits("label:bug search")).toEqual([]);
    expect(hits("author:ben")).toEqual([2]);
    expect(hits("is:draft")).toEqual([2]);
    expect(hits("#2")).toEqual([2]);
    expect(hits("repo:backend")).toEqual([1, 2]);
    expect(hits("branch:feat/")).toEqual([1]);
    expect(hits("checks:failing")).toEqual([]);
    expect(matches(b, parseQuery(""), { ...none, draft: "hide" })).toBe(false);
    expect(matches(a, parseQuery(""), { ...none, draft: "only" })).toBe(false);
    expect(matches(a, parseQuery(""), { ...none, review: "approved" })).toBe(false);
  });

  it("sorts by activity, age or size, and ties go to the higher number", () => {
    const rows = [
      row({
        number: 1,
        updated_at: "2026-10-03T00:00:00Z",
        created_at: "2026-09-01T00:00:00Z",
        additions: 5,
        deletions: 0,
      }),
      row({
        number: 2,
        updated_at: "2026-10-05T00:00:00Z",
        created_at: "2026-10-01T00:00:00Z",
        additions: 500,
        deletions: 10,
      }),
      row({
        number: 3,
        updated_at: "2026-10-05T00:00:00Z",
        created_at: "2026-09-15T00:00:00Z",
        additions: 50,
        deletions: 0,
      }),
    ];
    const order = (s: Parameters<typeof sortRows>[1]) => sortRows(rows, s).map((r) => r.number);
    expect(order("updated")).toEqual([3, 2, 1]);
    expect(order("newest")).toEqual([2, 3, 1]);
    expect(order("oldest")).toEqual([1, 3, 2]);
    expect(order("largest")).toEqual([2, 3, 1]);
    expect(order("smallest")).toEqual([1, 3, 2]);
  });

  it("puts what waits for you first, then yours, then the rest, and drops empty groups", () => {
    const groups = groupRows([
      row({ number: 1 }),
      row({ number: 2, by_me: true }),
      row({ number: 3, review_requested: true }),
    ]);
    expect(groups.map((g) => [g.title, g.rows.map((r) => r.number)])).toEqual([
      ["Needs your review", [3]],
      ["Yours", [2]],
      ["Others", [1]],
    ]);
    expect(groupRows([row({ by_me: true })]).map((g) => g.id)).toEqual(["mine"]);
    expect(groupRows([])).toEqual([]);
  });
});

function seed(list: Pick<PrList, "rows" | "failures"> & Partial<PrList>) {
  defaultBackend();
  backend
    .on("project_list", [project(), project({ id: "p2", name: "portfolio" })])
    .on("workspace_list", [workspace({ last_opened_at: 5, project_id: "p2" })])
    .on("pr_browse_list", { fetched_at: Date.now(), refreshing: false, ...list });
}

describe("the Pull requests page", () => {
  beforeEach(() => {
    localStorage.clear();
    usePrs.setState({ scope: undefined, which: "open", query: "" });
  });

  it("has its own item in the sidebar, after Composer", async () => {
    seed({ rows: [], failures: [] });
    renderApp("/");
    await screen.findByRole("main");
    const nav = screen.getByRole("navigation", { name: "Primary" });
    const names = [...nav.querySelectorAll("a, button")].map((e) => e.textContent?.trim());
    expect(names.slice(-2)).toEqual(["Composer", "Pull requests"]);
  });

  it("lists the pull requests in groups, with size, and starts with the project worked in last", async () => {
    seed({
      rows: [
        row({
          number: 7,
          title: "Review me",
          review_requested: true,
          additions: 1137,
          deletions: 0,
        }),
        row({ number: 8, title: "My change", by_me: true, review: "approved" }),
        row({
          number: 9,
          title: "Someone else",
          checks: { state: "failing", passed: 1, total: 3 },
        }),
      ],
      failures: [],
    });
    const { user } = renderApp("/pull-requests");
    expect(await screen.findByRole("button", { name: "#7 Review me" })).toBeInTheDocument();
    for (const group of ["Needs your review", "Yours", "Others"]) {
      expect(screen.getByRole("region", { name: group })).toBeInTheDocument();
    }
    expect(screen.getByText("+1,137")).toBeInTheDocument();
    expect(screen.getByLabelText("Approved")).toBeInTheDocument();
    expect(screen.getByLabelText("Checks failing: 1 of 3 passed")).toBeInTheDocument();
    // No project was ever chosen, so the one with the newest workspace is asked.
    const call = backend.calls.find((c) => c.command === "pr_browse_list");
    expect(call?.args).toEqual({ projectId: "p2", which: "open", refresh: false });
    expect(
      within(screen.getByRole("region", { name: "Pull requests" })).getByRole("button", {
        name: /portfolio/,
      }),
    ).toBeInTheDocument();
    await user.type(screen.getByLabelText("Search pull requests"), "someone");
    expect(screen.queryByRole("button", { name: "#7 Review me" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "#9 Someone else" })).toBeInTheDocument();
  });

  it("asks for every project when All projects is chosen, and remembers it", async () => {
    seed({ rows: [row()], failures: [] });
    const { user } = renderApp("/pull-requests");
    await screen.findByRole("button", { name: "#1 Add login" });
    await user.click(
      within(screen.getByRole("region", { name: "Pull requests" })).getByRole("button", {
        name: /portfolio/,
      }),
    );
    await user.click(await screen.findByRole("menuitemradio", { name: "All projects" }));
    await waitFor(() =>
      expect(
        backend.calls
          .filter((c) => c.command === "pr_browse_list")
          .some((c) => c.args.projectId === null),
      ).toBe(true),
    );
    expect(usePrs.getState().scope).toBeNull();
    // With all projects, the second line names the repository.
    expect(await screen.findByText("acme/backend")).toBeInTheDocument();
  });

  it("says when nothing matches and when a repository could not be asked", async () => {
    seed({
      rows: [row()],
      failures: [
        {
          project_id: "p9",
          project_name: "old",
          message: "The GitHub CLI is not signed in.",
          fix: "gh auth login",
          gone: false,
        },
      ],
    });
    const { user } = renderApp("/pull-requests");
    await screen.findByRole("button", { name: "#1 Add login" });
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("old: The GitHub CLI is not signed in.");
    expect(alert).toHaveTextContent("gh auth login");
    await user.type(screen.getByLabelText("Search pull requests"), "zzz");
    expect(await screen.findByText("No pull request matches.")).toBeInTheDocument();
  });

  it("says so when there are none, and shows an error from the backend", async () => {
    seed({ rows: [], failures: [] });
    renderApp("/pull-requests");
    expect(await screen.findByText("No pull requests here.")).toBeInTheDocument();
  });

  it("shows an error when asking fails altogether", async () => {
    defaultBackend();
    backend.fail("pr_browse_list", { code: "gh", message: "GitHub could not be reached" });
    renderApp("/pull-requests");
    expect(await screen.findByText(/GitHub could not be reached/)).toBeInTheDocument();
  });

  it("shows what was kept at once with its age, and again when the refresh is done", async () => {
    let calls = 0;
    defaultBackend();
    backend
      .on("project_list", [project(), project({ id: "p2", name: "portfolio" })])
      .on("workspace_list", [workspace({ last_opened_at: 5, project_id: "p2" })])
      .on("pr_browse_list", () => {
        calls += 1;
        return calls === 1
          ? {
              rows: [row({ number: 7, title: "Kept answer" })],
              failures: [],
              fetched_at: Date.now() - 4 * 60_000,
              refreshing: true,
            }
          : {
              rows: [
                row({ number: 7, title: "Kept answer" }),
                row({ number: 8, title: "New one" }),
              ],
              failures: [],
              fetched_at: Date.now(),
              refreshing: false,
            };
      });
    renderApp("/pull-requests");
    expect(await screen.findByRole("button", { name: "#7 Kept answer" })).toBeInTheDocument();
    expect(screen.getByText(/Updated 4min ago/)).toBeInTheDocument();
    expect(screen.getByText(/refreshing…/)).toBeInTheDocument();
    expect(screen.queryByText("Asking GitHub…")).not.toBeInTheDocument();
    emitBackendEvent("cache:changed", "pr-list");
    expect(await screen.findByRole("button", { name: "#8 New one" })).toBeInTheDocument();
    expect(await screen.findByText("Updated just now")).toBeInTheDocument();
  });

  it("asks GitHub straight away, not for what was kept, when refresh is pressed", async () => {
    seed({ rows: [row()], failures: [] });
    const { user } = renderApp("/pull-requests");
    await screen.findByRole("button", { name: "#1 Add login" });
    expect(backend.calls.find((c) => c.command === "pr_browse_list")?.args.refresh).toBe(false);
    await user.click(screen.getByRole("button", { name: "Refresh" }));
    await waitFor(() =>
      expect(
        backend.calls.some((c) => c.command === "pr_browse_list" && c.args.refresh === true),
      ).toBe(true),
    );
  });

  it("keeps the list on screen while another project is loaded", async () => {
    let release: (value: PrList) => void = () => {};
    defaultBackend();
    backend
      .on("project_list", [project(), project({ id: "p2", name: "portfolio" })])
      .on("workspace_list", [workspace({ last_opened_at: 5, project_id: "p2" })])
      .on("pr_browse_list", (args: Record<string, unknown>) =>
        args.projectId === "p2"
          ? {
              rows: [row({ title: "First list" })],
              failures: [],
              fetched_at: Date.now(),
              refreshing: false,
            }
          : new Promise<PrList>((resolve) => {
              release = resolve;
            }),
      );
    const { user } = renderApp("/pull-requests");
    await screen.findByRole("button", { name: "#1 First list" });
    await user.click(
      within(screen.getByRole("region", { name: "Pull requests" })).getByRole("button", {
        name: /portfolio/,
      }),
    );
    await user.click(await screen.findByRole("menuitemradio", { name: "All projects" }));
    await waitFor(() =>
      expect(
        backend.calls.some((c) => c.command === "pr_browse_list" && c.args.projectId === null),
      ).toBe(true),
    );
    // No loading screen replaces what was shown.
    expect(screen.getByRole("button", { name: "#1 First list" })).toBeInTheDocument();
    expect(screen.queryByText("Asking GitHub…")).not.toBeInTheDocument();
    release({
      rows: [row({ title: "Everything" })],
      failures: [],
      fetched_at: Date.now(),
      refreshing: false,
    });
    expect(await screen.findByRole("button", { name: "#1 Everything" })).toBeInTheDocument();
  });
});
