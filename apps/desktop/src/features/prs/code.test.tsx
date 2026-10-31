import type { FilePair } from "@circles/bindings/FilePair";
import type { PrFiles } from "@circles/bindings/PrFiles";
import { screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { backend } from "@/test/backend";
import { project, workspace } from "@/test/fixtures";
import { defaultBackend, renderApp } from "@/test/render";
import { usePrs } from "./store";

const BASE = "b".repeat(40);
const HEAD = "a".repeat(40);

function files(): PrFiles {
  return {
    base_sha: BASE,
    head_sha: HEAD,
    files: [
      {
        path: "src/a.rs",
        previous_path: null,
        status: "modified",
        additions: 3,
        deletions: 1,
        has_patch: true,
      },
      {
        path: "src/new.rs",
        previous_path: "src/old.rs",
        status: "renamed",
        additions: 0,
        deletions: 0,
        has_patch: false,
      },
      {
        path: "docs/b.md",
        previous_path: null,
        status: "added",
        additions: 10,
        deletions: 0,
        has_patch: true,
      },
      {
        path: "logo.png",
        previous_path: null,
        status: "added",
        additions: 0,
        deletions: 0,
        has_patch: false,
      },
      {
        path: "gone.txt",
        previous_path: null,
        status: "removed",
        additions: 0,
        deletions: 4,
        has_patch: true,
      },
    ],
  };
}

function pair(over: Partial<FilePair> = {}): FilePair {
  return {
    path: "src/a.rs",
    original: "old\n",
    modified: "new\n",
    language: "rust",
    binary: false,
    too_large: false,
    ...over,
  };
}

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
  review: "none",
  requested_reviewers: [],
  checks: { state: "none", passed: 0, total: 0 },
  by_me: false,
  review_requested: false,
};

function seed(read: (a: Record<string, unknown>) => unknown = () => pair()) {
  defaultBackend();
  backend
    .on("project_list", [project({ id: "p1", name: "backend" })])
    .on("workspace_list", [workspace({ project_id: "p1" })])
    .on("pr_browse_list", { rows: [row], failures: [] })
    .on("pr_browse_detail", {
      row,
      body: "",
      mergeable: "mergeable",
      merge_state: "clean",
      head_sha: HEAD,
      merged_by: null,
      merged_at: null,
      reviews: [],
      comments: [],
      commits: [],
      check_runs: [],
    })
    .on("pr_browse_files", files())
    .on("pr_browse_file", read);
}

async function openCode() {
  const app = renderApp("/pull-requests");
  await app.user.click(await screen.findByRole("button", { name: /#42 Bump deps/ }));
  await app.user.click(await screen.findByRole("tab", { name: "Code" }));
  await screen.findByRole("button", { name: "Expand src/a.rs" });
  return app;
}

describe("the Code tab of a pull request", () => {
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

  it("lists the changed files with their status and size, asking for the right pull request", async () => {
    seed();
    await openCode();
    expect(screen.getByText("5 files · 0 / 5 viewed")).toBeInTheDocument();
    expect(screen.getByText("src/old.rs → src/new.rs")).toBeInTheDocument();
    expect(screen.getByText("Renamed")).toBeInTheDocument();
    expect(screen.getByText("Deleted")).toBeInTheDocument();
    expect(screen.getByText("+3")).toBeInTheDocument();
    const call = backend.calls.find((c) => c.command === "pr_browse_files");
    // The tip of the branch is passed, so what was kept for it can be used without asking GitHub.
    expect(call?.args).toEqual({
      projectId: "p1",
      repo: "acme/backend",
      number: 42,
      headSha: HEAD,
    });
    // Nothing is read until a file is opened.
    expect(backend.calls.some((c) => c.command === "pr_browse_file")).toBe(false);
  });

  it("shows both sides of a file in the diff editor when it is opened", async () => {
    seed();
    const { user } = await openCode();
    await user.click(screen.getByRole("button", { name: "Expand src/a.rs" }));
    expect(await screen.findByTestId("diff-editor")).toHaveAttribute("data-language", "rust");
    expect(screen.getByTestId("original")).toHaveTextContent("old");
    expect(screen.getByTestId("modified")).toHaveTextContent("new");
    const call = backend.calls.find((c) => c.command === "pr_browse_file");
    expect(call?.args.request).toEqual({
      project_id: "p1",
      repo: "acme/backend",
      base_sha: BASE,
      head_sha: HEAD,
      path: "src/a.rs",
      previous_path: null,
      status: "modified",
    });
    await user.click(screen.getByRole("button", { name: "Show one column" }));
    expect(screen.getByTestId("diff-editor")).toHaveAttribute("data-split", "false");
  });

  it("keeps at most two files open, the oldest closing first", async () => {
    seed();
    const { user } = await openCode();
    for (const path of ["src/a.rs", "docs/b.md", "gone.txt"]) {
      await user.click(screen.getByRole("button", { name: `Expand ${path}` }));
    }
    await waitFor(() => expect(screen.getAllByTestId("diff-editor")).toHaveLength(2));
    expect(screen.getByRole("button", { name: "Expand src/a.rs" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Collapse gone.txt" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Collapse all" }));
    expect(screen.queryByTestId("diff-editor")).not.toBeInTheDocument();
  });

  it("marks a file as viewed, puts it away, and remembers it for this version of the branch", async () => {
    seed();
    const { user } = await openCode();
    await user.click(screen.getByRole("button", { name: "Expand src/a.rs" }));
    await screen.findByTestId("diff-editor");
    const item = screen.getByText("src/a.rs").closest("li") as HTMLElement;
    await user.click(within(item).getByRole("checkbox", { name: "Viewed" }));
    expect(screen.queryByTestId("diff-editor")).not.toBeInTheDocument();
    expect(screen.getByText("5 files · 1 / 5 viewed")).toBeInTheDocument();
    expect(usePrs.getState().viewed[`p1#42@${HEAD}`]).toEqual(["src/a.rs"]);
    await user.click(within(item).getByRole("checkbox", { name: "Viewed" }));
    expect(screen.getByText("5 files · 0 / 5 viewed")).toBeInTheDocument();
  });

  it("says so for a binary file, a file too large, and a failed read", async () => {
    seed((a) => {
      const path = (a.request as { path: string }).path;
      if (path === "logo.png") return pair({ path, binary: true, original: null, modified: null });
      if (path === "docs/b.md")
        return pair({ path, too_large: true, original: null, modified: null });
      throw { code: "gh", message: "GitHub could not be reached", details: null };
    });
    const { user } = await openCode();
    await user.click(screen.getByRole("button", { name: "Expand logo.png" }));
    expect(await screen.findByText(/binary file/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Expand docs/b.md" }));
    expect(await screen.findByText(/too large to compare/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Expand src/a.rs" }));
    expect(await screen.findByText("GitHub could not be reached")).toBeInTheDocument();
  });

  it("says when the files cannot be listed", async () => {
    seed();
    backend.fail("pr_browse_files", { code: "gh", message: "Not Found" });
    const app = renderApp("/pull-requests");
    await app.user.click(await screen.findByRole("button", { name: /#42 Bump deps/ }));
    await app.user.click(await screen.findByRole("tab", { name: "Code" }));
    expect(await screen.findByText("Not Found")).toBeInTheDocument();
  });
});
