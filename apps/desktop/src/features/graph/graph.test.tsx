import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useReview } from "@/stores/review";
import { backend, emitBackendEvent } from "@/test/backend";
import { workspace } from "@/test/fixtures";
import { defaultBackend, renderApp } from "@/test/render";

const commit = (
  sha: string,
  subject: string,
  parents: string[],
  refs: { name: string; kind: string }[] = [],
) => ({
  sha: sha.padEnd(40, "0"),
  parents: parents.map((p) => p.padEnd(40, "0")),
  subject,
  author: "Ada",
  time: Math.floor(Date.now() / 1000) - 180,
  refs,
});

const graph = (extra: Partial<Record<string, unknown>> = {}) => ({
  commits: [
    commit(
      "f2",
      "Add login form",
      ["f1"],
      [
        { name: "feature", kind: "head" },
        { name: "origin/feature", kind: "remote" },
      ],
    ),
    commit("m1", "Fix build", ["root"], [{ name: "main", kind: "branch" }]),
    commit("f1", "Start login", ["root"]),
    commit("root", "Initial commit", []),
  ],
  branch: "feature",
  base: "main",
  ahead: 2,
  behind: 1,
  truncated: false,
  ...extra,
});

function open(data = graph()) {
  defaultBackend();
  backend.on("workspace_get", workspace({ id: "w1" })).on("git_graph", data);
  useReview.setState({ graphOpen: true, graphHeight: null });
  return renderApp("/workspace/w1");
}

describe("git graph", () => {
  it("opens the graph large over the chat with Expand", async () => {
    const app = open();
    const region = await screen.findByRole("region", { name: "Git graph" });
    await within(region).findByText("Add login form");
    await app.user.click(within(region).getByRole("button", { name: "Expand graph" }));
    const dialog = await screen.findByRole("dialog", { name: "Git graph" });
    expect(await within(dialog).findByText("Add login form")).toBeInTheDocument();
    expect(within(dialog).getByText("feature → main")).toBeInTheDocument();
    await app.user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("draws the branch and its base with refs, counts and ages", async () => {
    open();
    const region = await screen.findByRole("region", { name: "Git graph" });
    await within(region).findByText("Add login form");
    for (const text of ["Fix build", "Start login", "Initial commit", "feature", "main"]) {
      expect(within(region).getByText(text)).toBeInTheDocument();
    }
    expect(within(region).queryByText(/feature → main/)).not.toBeInTheDocument();
    expect(within(region).getByTestId("graph-counts")).toHaveTextContent("↑2");
    expect(within(region).getByTestId("graph-counts")).toHaveTextContent("↓1");
    expect(within(region).getAllByText("3min").length).toBeGreaterThan(0);
    // Two lines side by side: the branch and its base.
    expect(region.querySelectorAll("li svg").length).toBe(4);
    expect(region.querySelectorAll("li svg path").length).toBeGreaterThan(4);
  });

  it("badges only the current branch and the base, not remote copies, other branches or tags", async () => {
    open(
      graph({
        commits: [
          commit(
            "f2",
            "Tip",
            ["f1"],
            [
              { name: "feature", kind: "head" },
              { name: "origin/feature", kind: "remote" },
              { name: "other-work", kind: "branch" },
              { name: "v1.0", kind: "tag" },
            ],
          ),
          commit(
            "m1",
            "Base",
            ["f1"],
            [
              { name: "origin/main", kind: "remote" },
              { name: "main", kind: "branch" },
            ],
          ),
          commit("f1", "Old", [], [{ name: "origin/main", kind: "remote" }]),
        ],
      }),
    );
    const region = await screen.findByRole("region", { name: "Git graph" });
    await within(region).findByText("Tip");
    const chips = (text: string) => within(region).queryAllByText(text);
    expect(chips("feature")).toHaveLength(1);
    expect(chips("main")).toHaveLength(1);
    // The remote copy of the base shows where the local one is not on the commit.
    expect(chips("origin/main")).toHaveLength(1);
    for (const hidden of ["origin/feature", "other-work", "v1.0"]) {
      expect(chips(hidden)).toHaveLength(0);
    }
  });

  it("marks the checked out commit with a hollow dot", async () => {
    open();
    const region = await screen.findByRole("region", { name: "Git graph" });
    await within(region).findByText("Add login form");
    const dots = [...region.querySelectorAll("li")].map((li) =>
      li.querySelector("circle")?.getAttribute("fill"),
    );
    expect(dots[0]).toBe("var(--background)");
    expect(dots[1]).not.toBe("var(--background)");
  });

  it("refreshes when files, commits or branches change", async () => {
    open();
    await screen.findByText("Add login form");
    const before = backend.callsTo("git_graph").length;
    backend.on(
      "git_graph",
      graph({ commits: [commit("f3", "A brand new commit", ["f2"]), ...graph().commits] }),
    );
    act(() => emitBackendEvent("workspace:files-changed", { workspace_id: "w1" }));
    expect(await screen.findByText("A brand new commit")).toBeInTheDocument();
    expect(backend.callsTo("git_graph").length).toBeGreaterThan(before);
    // Another workspace's changes are not ours.
    const count = backend.callsTo("git_graph").length;
    act(() => emitBackendEvent("workspace:files-changed", { workspace_id: "other" }));
    await new Promise((r) => setTimeout(r, 50));
    expect(backend.callsTo("git_graph").length).toBe(count);
  });

  it("refreshes when the window gets focus", async () => {
    open();
    await screen.findByText("Add login form");
    const before = backend.callsTo("git_graph").length;
    act(() => {
      window.dispatchEvent(new Event("focus"));
    });
    await waitFor(() => expect(backend.callsTo("git_graph").length).toBeGreaterThan(before));
  });

  it("collapses to its header and stops fetching", async () => {
    const app = open();
    await screen.findByText("Add login form");
    await app.user.click(screen.getByRole("button", { name: /Graph/ }));
    expect(screen.queryByText("Add login form")).not.toBeInTheDocument();
    expect(useReview.getState().graphOpen).toBe(false);
    const count = backend.callsTo("git_graph").length;
    act(() => emitBackendEvent("workspace:files-changed", { workspace_id: "w1" }));
    await new Promise((r) => setTimeout(r, 50));
    expect(backend.callsTo("git_graph").length).toBe(count);
  });

  it("says when older commits are cut off, and shows errors", async () => {
    open(graph({ truncated: true }));
    expect(await screen.findByText("Older commits are not shown.")).toBeInTheDocument();
  });

  it("shows an error from the backend", async () => {
    defaultBackend();
    backend
      .on("workspace_get", workspace({ id: "w1" }))
      .fail("git_graph", { code: "git", message: "not a repository" });
    useReview.setState({ graphOpen: true, graphHeight: null });
    renderApp("/workspace/w1");
    const region = await screen.findByRole("region", { name: "Git graph" });
    expect(await within(region).findByRole("alert")).toHaveTextContent("not a repository");
  });
});

describe("resizing the graph", () => {
  it("grows and shrinks with the arrow keys, within limits, and resets on double click", async () => {
    const app = open();
    const handle = await screen.findByRole("separator", { name: "Resize graph" });
    const region = screen.getByRole("region", { name: "Git graph" });
    // jsdom has no layout: give the pane and the graph a size.
    const rect = (height: number, bottom = 800) =>
      ({
        height,
        bottom,
        top: bottom - height,
        left: 0,
        right: 0,
        width: 0,
        x: 0,
        y: 0,
        toJSON() {},
      }) as DOMRect;
    (region.parentElement as HTMLElement).getBoundingClientRect = () => rect(700);
    region.getBoundingClientRect = () => rect(250);

    handle.focus();
    await app.user.keyboard("{ArrowUp}");
    expect(useReview.getState().graphHeight).toBe(274);
    await app.user.keyboard("{ArrowDown}{ArrowDown}");
    expect(useReview.getState().graphHeight).toBe(226);
    await app.user.keyboard("{End}");
    expect(useReview.getState().graphHeight).toBe(120);
    await app.user.keyboard("{ArrowDown}");
    expect(useReview.getState().graphHeight).toBe(120);
    await app.user.keyboard("{Home}");
    expect(useReview.getState().graphHeight).toBe(540);
    expect(region).toHaveStyle({ height: "540px" });
    await app.user.dblClick(handle);
    expect(useReview.getState().graphHeight).toBeNull();
  });

  it("follows the pointer while dragging", async () => {
    open();
    const handle = await screen.findByRole("separator", { name: "Resize graph" });
    const region = screen.getByRole("region", { name: "Git graph" });
    (region.parentElement as HTMLElement).getBoundingClientRect = () =>
      ({
        height: 700,
        bottom: 800,
        top: 100,
        left: 0,
        right: 0,
        width: 0,
        x: 0,
        y: 100,
        toJSON() {},
      }) as DOMRect;
    handle.setPointerCapture = () => {};
    handle.hasPointerCapture = () => true;
    fireEvent.pointerDown(handle, { pointerId: 1, clientY: 600 });
    fireEvent.pointerMove(handle, { pointerId: 1, clientY: 500 });
    expect(useReview.getState().graphHeight).toBe(300);
    fireEvent.pointerMove(handle, { pointerId: 1, clientY: 50 });
    expect(useReview.getState().graphHeight).toBe(540);
    fireEvent.pointerMove(handle, { pointerId: 1, clientY: 790 });
    expect(useReview.getState().graphHeight).toBe(120);
  });

  it("has no handle while collapsed", async () => {
    const app = open();
    await screen.findByRole("separator", { name: "Resize graph" });
    await app.user.click(screen.getByRole("button", { name: /Graph/ }));
    expect(screen.queryByRole("separator", { name: "Resize graph" })).not.toBeInTheDocument();
  });
});
