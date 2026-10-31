import type { PrDetail } from "@circles/bindings/PrDetail";
import { screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { backend } from "@/test/backend";
import { project, workspace } from "@/test/fixtures";
import { defaultBackend, renderApp } from "@/test/render";
import { mergeConcerns } from "./merge-dialog";
import { usePrs } from "./store";

const row = {
  project_id: "p1",
  project_name: "backend",
  repo: "acme/backend",
  number: 42,
  title: "Bump deps",
  url: "https://github.com/acme/backend/pull/42",
  author: "ana",
  author_avatar: "https://github.com/ana.png?size=64",
  head_ref: "fix/x",
  base_ref: "develop",
  state: "open",
  is_draft: false,
  additions: 13,
  deletions: 5,
  changed_files: 5,
  created_at: "2026-10-20T10:00:00Z",
  updated_at: "2026-10-22T10:00:00Z",
  labels: [],
  review: "approved",
  requested_reviewers: [],
  checks: { state: "passing", passed: 3, total: 3 },
  by_me: false,
  review_requested: false,
};

function detail(over: Partial<PrDetail["row"]> = {}, rest: Partial<PrDetail> = {}): PrDetail {
  return {
    row: { ...row, ...over },
    body: "",
    mergeable: "mergeable",
    merge_state: "clean",
    head_sha: "a".repeat(40),
    merged_by: null,
    merged_at: null,
    reviews: [],
    comments: [],
    commits: [],
    check_runs: [],
    ...rest,
  };
}

describe("what stands in the way of a merge", () => {
  it("blocks a closed pull request, a draft and a conflict, which cannot be overridden", () => {
    expect(mergeConcerns(detail({ state: "merged" })).blocked).toMatch(/not open/);
    expect(mergeConcerns(detail({ is_draft: true })).blocked).toMatch(/draft/);
    expect(mergeConcerns(detail({}, { mergeable: "conflicting" })).blocked).toMatch(/conflicts/);
    expect(mergeConcerns(detail())).toEqual({ blocked: null, warnings: [] });
  });

  it("warns about checks, reviews and the branch rules, which can be overridden", () => {
    const warnings = (d: PrDetail) => mergeConcerns(d).warnings;
    expect(warnings(detail({ checks: { state: "failing", passed: 1, total: 3 } }))).toEqual([
      "Some checks are failing.",
    ]);
    expect(warnings(detail({ checks: { state: "pending", passed: 1, total: 3 } }))).toEqual([
      "Some checks have not finished.",
    ]);
    expect(warnings(detail({ review: "changes_requested" }))).toEqual([
      "A reviewer asked for changes.",
    ]);
    expect(warnings(detail({ review: "review_required" }))).toEqual([
      "It has not been approved yet.",
    ]);
    expect(warnings(detail({}, { merge_state: "blocked" }))[0]).toMatch(/branch rules/);
    expect(warnings(detail({}, { merge_state: "behind" }))).toEqual([
      "The branch is behind its base.",
    ]);
  });
});

function seed(d: PrDetail) {
  defaultBackend();
  backend
    .on("project_list", [project({ id: "p1", name: "backend" })])
    .on("workspace_list", [workspace({ project_id: "p1" })])
    .on("pr_browse_list", { rows: [d.row], failures: [] })
    .on("pr_browse_detail", d)
    .on("pr_browse_merge", null)
    .on("workspace_create", workspace({ id: "from-pr" }));
}

async function openPr() {
  const app = renderApp("/pull-requests");
  await app.user.click(await screen.findByRole("button", { name: /#42 Bump deps/ }));
  await screen.findByRole("heading", { name: /Bump deps/ });
  return app;
}

describe("Merge and Check out", () => {
  beforeEach(() => {
    localStorage.clear();
    usePrs.setState({
      scope: null,
      which: "open",
      query: "",
      tabs: [],
      selected: null,
      viewed: {},
    });
  });

  it("asks before merging and merges the way that was chosen", async () => {
    seed(detail());
    const { user } = await openPr();
    await user.click(screen.getByRole("button", { name: "Merge" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Merge #42 into develop?")).toBeInTheDocument();
    // A merge commit is the default, and nothing was sent yet.
    expect(within(dialog).getByRole("radio", { name: /Merge commit/ })).toBeChecked();
    expect(backend.calls.some((c) => c.command === "pr_browse_merge")).toBe(false);
    await user.click(within(dialog).getByRole("radio", { name: /Squash and merge/ }));
    await user.click(within(dialog).getByRole("button", { name: "Merge" }));
    await waitFor(() =>
      expect(backend.calls.find((c) => c.command === "pr_browse_merge")?.args).toEqual({
        projectId: "p1",
        number: 42,
        method: "squash",
      }),
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("does not merge until a warning has been acknowledged", async () => {
    seed(detail({ checks: { state: "failing", passed: 1, total: 3 } }));
    const { user } = await openPr();
    await user.click(screen.getByRole("button", { name: "Merge" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Some checks are failing.")).toBeInTheDocument();
    const confirm = within(dialog).getByRole("button", { name: "Merge" });
    expect(confirm).toBeDisabled();
    await user.click(within(dialog).getByRole("checkbox", { name: "Merge anyway" }));
    expect(confirm).toBeEnabled();
  });

  it("cannot merge a draft, and offers no Merge for one that is already merged", async () => {
    seed(detail({ is_draft: true }));
    const first = await openPr();
    await first.user.click(screen.getByRole("button", { name: "Merge" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByRole("alert")).toHaveTextContent(/draft/);
    expect(within(dialog).getByRole("button", { name: "Merge" })).toBeDisabled();
  });

  it("has no Merge button for a pull request that is not open", async () => {
    seed(detail({ state: "merged" }));
    await openPr();
    expect(screen.queryByRole("button", { name: "Merge" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Check out" })).toBeInTheDocument();
  });

  it("shows the error when GitHub refuses, and keeps the dialog", async () => {
    seed(detail());
    backend.fail("pr_browse_merge", { code: "gh", message: "Pull request is not mergeable" });
    const { user } = await openPr();
    await user.click(screen.getByRole("button", { name: "Merge" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Merge" }));
    await waitFor(() =>
      expect(backend.calls.some((c) => c.command === "pr_browse_merge")).toBe(true),
    );
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("checks the pull request out into a workspace of its project", async () => {
    seed(detail());
    const { user } = await openPr();
    expect(screen.getByRole("button", { name: "Copy the checkout command" })).toBeInTheDocument();
    expect(screen.getByText("gh pr checkout 42")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Check out" }));
    await waitFor(() => {
      const call = backend.calls.find((c) => c.command === "workspace_create");
      expect(call?.args).toEqual({
        projectId: "p1",
        options: {
          source: { kind: "pull_request", number: 42 },
          prompt: null,
          model: null,
          effort: null,
          attachments: [],
        },
      });
    });
    // The new workspace opens.
    await waitFor(() =>
      expect(screen.queryByRole("region", { name: "Pull requests" })).not.toBeInTheDocument(),
    );
  });
});
