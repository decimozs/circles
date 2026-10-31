import type { PrDetail } from "@circles/bindings/PrDetail";
import type { PrRow } from "@circles/bindings/PrRow";
import { screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { backend } from "@/test/backend";
import { project, workspace } from "@/test/fixtures";
import { defaultBackend, renderApp } from "@/test/render";
import { usePrs } from "./store";
import { buildTimeline } from "./timeline";

function row(over: Partial<PrRow> = {}): PrRow {
  return {
    project_id: "p1",
    project_name: "backend",
    repo: "acme/backend",
    number: 42,
    title: "STH-1766 : Bump nine dependencies",
    url: "https://github.com/acme/backend/pull/42",
    author: "Alfa-Roselle",
    author_avatar: "https://github.com/Alfa-Roselle.png?size=64",
    head_ref: "fix/STH-1766",
    base_ref: "develop",
    state: "open",
    is_draft: false,
    additions: 137,
    deletions: 10,
    changed_files: 2,
    created_at: "2026-10-20T10:00:00Z",
    updated_at: "2026-10-22T10:00:00Z",
    labels: [{ name: "deps", color: "0e8a16" }],
    review: "none",
    requested_reviewers: ["decimozs"],
    checks: { state: "passing", passed: 2, total: 3 },
    by_me: false,
    review_requested: true,
    ...over,
  };
}

function detail(over: Partial<PrDetail> = {}): PrDetail {
  return {
    row: row(),
    body: "## Summary\n\nOnly `requirements.txt` changes.",
    mergeable: "mergeable",
    merge_state: "clean",
    head_sha: "a".repeat(40),
    merged_by: null,
    merged_at: null,
    reviews: [
      {
        author: "ben",
        author_avatar: "https://github.com/ben.png?size=64",
        state: "approved",
        body: "Looks right",
        submitted_at: "2026-10-21T10:00:00Z",
      },
    ],
    comments: [
      {
        author: "ana",
        author_avatar: "https://github.com/ana.png?size=64",
        body: "Please check pymongo",
        created_at: "2026-10-21T09:00:00Z",
        url: "https://github.com/acme/backend/pull/42#issuecomment-1",
      },
      {
        author: "ben",
        author_avatar: "https://github.com/ben.png?size=64",
        body: "Done in the second commit",
        created_at: "2026-10-21T11:00:00Z",
        url: "https://github.com/acme/backend/pull/42#issuecomment-2",
      },
    ],
    commits: [
      {
        sha: "acc7a2b".padEnd(40, "0"),
        headline: "fix(STH-1766): bump nine dependencies",
        body: "",
        author: "Alfa-Roselle",
        author_avatar: "https://github.com/Alfa-Roselle.png?size=64",
        authored_at: "2026-10-20T11:00:00Z",
      },
    ],
    check_runs: [
      { name: "build", workflow: "CI", state: "passing", url: "https://github.com/x/1" },
      { name: "lint", workflow: "CI", state: "passing", url: "https://github.com/x/2" },
      { name: "test", workflow: "CI", state: "failing", url: "https://github.com/x/3" },
    ],
    ...over,
  };
}

describe("the timeline of a pull request", () => {
  it("lists what happened oldest first, with a merge at the end", () => {
    const merged = detail({ merged_by: "ana", merged_at: "2026-10-23T10:00:00Z" });
    expect(buildTimeline(merged).map((e) => [e.kind, e.author])).toEqual([
      ["opened", "Alfa-Roselle"],
      ["commit", "Alfa-Roselle"],
      ["comment", "ana"],
      ["review", "ben"],
      ["comment", "ben"],
      ["merged", "ana"],
    ]);
    const review = buildTimeline(merged).find((e) => e.kind === "review");
    expect(review?.title).toBe("approved these changes");
    const commit = buildTimeline(merged).find((e) => e.kind === "commit");
    expect(commit?.detail).toBe("acc7a2b");
  });

  it("puts an event with an unreadable date last, not first", () => {
    const events = buildTimeline(
      detail({
        comments: [{ author: "x", author_avatar: "", body: "?", created_at: "", url: "u" }],
      }),
    );
    expect(events[events.length - 1]?.kind).toBe("comment");
  });
});

function seed(d: PrDetail = detail()) {
  defaultBackend();
  backend
    .on("project_list", [project({ id: "p1", name: "backend" })])
    .on("workspace_list", [workspace({ project_id: "p1" })])
    .on("pr_browse_list", {
      rows: [row(), row({ number: 43, title: "AINT-37 : DNS", review_requested: false })],
      failures: [],
    })
    .on("pr_browse_detail", (args: Record<string, unknown>) =>
      args.number === 43 ? detail({ row: row({ number: 43, title: "AINT-37 : DNS" }) }) : d,
    );
}

describe("the pull request pane", () => {
  beforeEach(() => {
    localStorage.clear();
    usePrs.setState({ scope: null, which: "open", query: "", tabs: [], selected: null });
  });

  it("opens a pull request beside the list, in a tab, with its summary", async () => {
    seed();
    const { user } = renderApp("/pull-requests");
    await user.click(await screen.findByRole("button", { name: /#42 STH-1766/ }));

    expect(
      await screen.findByRole("heading", { name: /STH-1766 : Bump nine dependencies/ }),
    ).toBeInTheDocument();
    const tabs = screen.getByRole("tablist", { name: "Open pull requests" });
    // The tabs sit in the top bar, above both columns, not inside the pane.
    expect(tabs.closest("header")).not.toBeNull();
    expect(screen.queryByRole("button", { name: "Merge" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Check out" })).not.toBeInTheDocument();
    expect(within(tabs).getByRole("tab", { name: /#42/ })).toHaveAttribute("aria-selected", "true");
    expect(screen.getAllByText("acme/backend").length).toBeGreaterThan(0);
    expect(screen.getByText(/develop/)).toBeInTheDocument();
    expect(screen.getByText(/2 files/)).toBeInTheDocument();
    // The description is Markdown.
    expect(screen.getByText("requirements.txt").tagName).toBe("CODE");
    // Who reviews: one reviewed and approved, one was asked and has not answered.
    expect(screen.getByLabelText("Approved")).toBeInTheDocument();
    expect(screen.getByText("asked")).toBeInTheDocument();
    expect(screen.getByText("deps")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Checks: 2 of 3 passing/ })).toHaveAttribute(
      "aria-expanded",
      "false",
    );
  });

  it("shows the checks when asked, and the comments with the newest first", async () => {
    seed();
    const { user } = renderApp("/pull-requests");
    await user.click(await screen.findByRole("button", { name: /#42 STH-1766/ }));
    await user.click(await screen.findByRole("button", { name: /Checks: 2 of 3 passing/ }));
    expect(screen.getByRole("button", { name: /test/ })).toBeInTheDocument();
    const comments = within(screen.getByRole("region", { name: "Comments" }));
    const first = comments.getAllByRole("listitem")[0];
    expect(first).toHaveTextContent("Done in the second commit");
    await user.click(comments.getByRole("button", { name: "Newest first" }));
    expect(comments.getAllByRole("listitem")[0]).toHaveTextContent("Please check pymongo");
  });

  it("shows the timeline with commits, a review and comments", async () => {
    seed();
    const { user } = renderApp("/pull-requests");
    await user.click(await screen.findByRole("button", { name: /#42 STH-1766/ }));
    await user.click(await screen.findByRole("tab", { name: "Timeline" }));
    const timeline = within(await screen.findByRole("list", { name: "Timeline" }));
    expect(timeline.getByText("fix(STH-1766): bump nine dependencies")).toBeInTheDocument();
    expect(timeline.getByText("approved these changes")).toBeInTheDocument();
    expect(timeline.getByText("opened this pull request")).toBeInTheDocument();
    // Newest first by default: the last comment comes before the opening.
    const items = timeline.getAllByRole("listitem");
    expect(items[0]).toHaveTextContent("Done in the second commit");
    expect(items[items.length - 1]).toHaveTextContent("opened this pull request");
    await user.click(screen.getByRole("button", { name: "Newest first" }));
    expect(timeline.getAllByRole("listitem")[0]).toHaveTextContent("opened this pull request");
  });

  it("keeps several open, switches between them and closes them", async () => {
    seed();
    const { user } = renderApp("/pull-requests");
    await user.click(await screen.findByRole("button", { name: /#42 STH-1766/ }));
    await screen.findByRole("heading", { name: /STH-1766/ });
    await user.click(screen.getByRole("button", { name: /#43 AINT-37/ }));
    await screen.findByRole("heading", { name: /AINT-37 : DNS/ });
    const tabs = within(screen.getByRole("tablist", { name: "Open pull requests" }));
    expect(tabs.getAllByRole("tab")).toHaveLength(2);
    await user.click(tabs.getByRole("tab", { name: /#42/ }));
    expect(await screen.findByRole("heading", { name: /STH-1766/ })).toBeInTheDocument();
    await user.click(tabs.getByRole("button", { name: "Close #42" }));
    // The neighbour takes its place.
    expect(await screen.findByRole("heading", { name: /AINT-37 : DNS/ })).toBeInTheDocument();
    await user.click(tabs.getByRole("button", { name: "Close #43" }));
    await waitFor(() =>
      expect(screen.queryByRole("tablist", { name: "Open pull requests" })).not.toBeInTheDocument(),
    );
  });

  it("says when a pull request cannot be read", async () => {
    seed();
    backend.fail("pr_browse_detail", { code: "gh", message: "Not Found" });
    const { user } = renderApp("/pull-requests");
    await user.click(await screen.findByRole("button", { name: /#42 STH-1766/ }));
    expect(await screen.findByText("Not Found")).toBeInTheDocument();
  });

  it("warns of a conflict", async () => {
    seed(detail({ mergeable: "conflicting" }));
    const { user } = renderApp("/pull-requests");
    await user.click(await screen.findByRole("button", { name: /#42 STH-1766/ }));
    expect(await screen.findByText("This branch has conflicts with its base.")).toBeInTheDocument();
  });
});
