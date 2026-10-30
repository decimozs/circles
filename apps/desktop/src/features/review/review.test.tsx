import type { ChangedFile } from "@circles/bindings/ChangedFile";
import type { Comment } from "@circles/bindings/Comment";
import type { FileContent } from "@circles/bindings/FileContent";
import type { FilePair } from "@circles/bindings/FilePair";
import type { FsEntry } from "@circles/bindings/FsEntry";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useChat } from "@/stores/chat";
import { useReview } from "@/stores/review";
import { backend, emitBackendEvent } from "@/test/backend";
import { workspace } from "@/test/fixtures";
import { defaultBackend, renderApp } from "@/test/render";

const entry = (name: string, dir = false, path = name, ignored = false): FsEntry => ({
  name,
  path,
  is_dir: dir,
  size: 10,
  ignored,
});
const change = (path: string, extra: Partial<ChangedFile> = {}): ChangedFile => ({
  path,
  old_path: null,
  status: "modified",
  added: 3,
  removed: 1,
  binary: false,
  ...extra,
});
const text = (path: string, body: string): FileContent => ({
  path,
  kind: "text",
  size: body.length,
  text: body,
  language: "typescript",
  data_base64: null,
  mime: null,
});
const pair = (path: string, original: string | null, modified: string | null): FilePair => ({
  path,
  original,
  modified,
  language: "typescript",
  binary: false,
  too_large: false,
});
const comment = (extra: Partial<Comment> = {}): Comment => ({
  id: "c1",
  workspace_id: "w1",
  path: "src/a.ts",
  line_start: 3,
  line_end: 3,
  body: "Rename this",
  status: "open",
  source: "user",
  sent_at: null,
  created_at: 1,
  ...extra,
});

function open(setup: () => void = () => {}) {
  defaultBackend();
  backend.on("workspace_get", workspace({ id: "w1", city: "Algiers", path: "/ws/algiers" }));
  setup();
  return renderApp("/workspace/w1");
}

async function tab(app: ReturnType<typeof renderApp>, name: RegExp | string) {
  await app.user.click(await screen.findByRole("tab", { name }));
}

beforeEach(() => {
  useChat.setState(useChat.getInitialState(), true);
});

describe("right pane tabs", () => {
  it("keep each label on one line, and scroll sideways when the pane is very narrow", async () => {
    open();
    const list = await screen.findByRole("tablist", { name: "Review" });
    expect(list).toHaveClass("overflow-x-auto");
    for (const tabName of ["All files", /Changes/, "Checks"]) {
      const tab = within(list).getByRole("tab", { name: tabName });
      expect(tab).toHaveClass("whitespace-nowrap", "shrink-0");
    }
  });
});

describe("all files", () => {
  const tree = () =>
    backend.on("fs_list", (args: Record<string, unknown>) =>
      args.path === ""
        ? [entry("src", true), entry("README.md"), entry("dist", true, "dist", true)].filter(
            (e) => !e.ignored || args.showIgnored,
          )
        : [entry("a.ts", false, "src/a.ts")],
    );

  it("loads a folder only when it is opened and opens a file in the editor", async () => {
    const app = open(() => {
      tree();
      backend.on("fs_read", text("src/a.ts", "export const a = 1;"));
    });
    const files = await screen.findByRole("tree", { name: "Files" });
    expect(await within(files).findByRole("button", { name: "src" })).toBeInTheDocument();
    expect(backend.callsTo("fs_list")).toHaveLength(1);

    await app.user.click(within(files).getByRole("button", { name: "src" }));
    await app.user.click(await within(files).findByRole("button", { name: "a.ts" }));
    expect(backend.callsTo("fs_list").map((c) => c.path)).toEqual(["", "src"]);

    const editor = await screen.findByTestId("code-editor");
    expect(editor).toHaveAttribute("data-path", "src/a.ts");
    expect(editor).toHaveAttribute("data-readonly", "true");
    expect(editor).toHaveTextContent("export const a = 1;");
  });

  it("lists folders before files and hides ignored ones until asked", async () => {
    const app = open(tree);
    const files = await screen.findByRole("tree", { name: "Files" });
    await within(files).findByRole("button", { name: "src" });
    expect(within(files).queryByRole("button", { name: "dist" })).not.toBeInTheDocument();
    await app.user.click(screen.getByRole("button", { name: "Show ignored files" }));
    expect(await within(files).findByRole("button", { name: "dist" })).toBeInTheDocument();
    expect(backend.callsTo("fs_list").slice(-1)[0]).toMatchObject({ showIgnored: true });
  });

  it("searches by name", async () => {
    const app = open(() => {
      tree();
      backend.on("workspace_search_files", [{ path: "src/deep/button.tsx", is_dir: false }]);
      backend.on("fs_read", text("src/deep/button.tsx", "x"));
    });
    await app.user.type(await screen.findByLabelText("Search files"), "btn");
    const hit = await screen.findByRole("button", { name: "src/deep/button.tsx" });
    await app.user.click(hit);
    expect(await screen.findByTestId("code-editor")).toHaveAttribute(
      "data-path",
      "src/deep/button.tsx",
    );
  });

  it("copies paths, reveals files, opens editors and attaches a file to the chat", async () => {
    const app = open(() => {
      tree();
      backend.on("editor_list", [{ id: "cursor", name: "Cursor" }]);
      backend.on("fs_reveal", null).on("editor_open", null);
    });
    const writeText = vi.spyOn(navigator.clipboard, "writeText");
    const files = await screen.findByRole("tree", { name: "Files" });
    const row = await within(files).findByRole("button", { name: "README.md" });

    fireEvent.contextMenu(row);
    await app.user.click(await screen.findByRole("menuitem", { name: "Copy path" }));
    expect(writeText).toHaveBeenCalledWith("README.md");

    fireEvent.contextMenu(row);
    await app.user.click(await screen.findByRole("menuitem", { name: "Copy full path" }));
    expect(writeText).toHaveBeenCalledWith("/ws/algiers/README.md");

    fireEvent.contextMenu(row);
    await app.user.click(await screen.findByRole("menuitem", { name: "Reveal in Finder" }));
    expect(backend.callsTo("fs_reveal")).toEqual([{ workspaceId: "w1", path: "README.md" }]);

    fireEvent.contextMenu(row);
    await app.user.click(await screen.findByRole("menuitem", { name: "Open in Cursor" }));
    expect(backend.callsTo("editor_open")).toEqual([
      { workspaceId: "w1", editor: "cursor", path: "README.md" },
    ]);

    fireEvent.contextMenu(row);
    await app.user.click(await screen.findByRole("menuitem", { name: "Attach to chat" }));
    await waitFor(() => expect(useChat.getState().drafts.s1).toBe("@README.md "));
  });

  it("shows an image and describes binary and huge files", async () => {
    const app = open(() => {
      tree();
      backend.on("fs_read", (args: Record<string, unknown>) => {
        const base = { path: String(args.path), size: 2048, text: null, language: "plaintext" };
        if (args.path === "README.md") {
          return { ...base, kind: "image", data_base64: "AQID", mime: "image/png" };
        }
        if (args.path === "dist") return { ...base, kind: "binary", data_base64: null, mime: null };
        return { ...base, kind: "too_large", size: 5 * 1024 * 1024, data_base64: null, mime: null };
      });
    });
    await screen.findByRole("tree", { name: "Files" });
    useReview.getState().openFile("w1", "README.md");
    expect(await screen.findByRole("img", { name: "README.md" })).toHaveAttribute(
      "src",
      "data:image/png;base64,AQID",
    );
    act(() => useReview.getState().openFile("w1", "dist"));
    expect(await screen.findByText(/binary file/)).toBeInTheDocument();
    act(() => useReview.getState().openFile("w1", "huge.log"));
    expect(await screen.findByText(/too large to open/)).toBeInTheDocument();
    expect(screen.queryByTestId("code-editor")).not.toBeInTheDocument();
    expect(app.router).toBeDefined();
  });

  it("edits and saves a file", async () => {
    const app = open(() => {
      tree();
      backend.on("fs_read", text("README.md", "old"));
      backend.on("fs_write", null);
    });
    await screen.findByRole("tree", { name: "Files" });
    act(() => useReview.getState().openFile("w1", "README.md"));
    const editor = await screen.findByTestId("code-editor");
    expect(editor).toHaveAttribute("data-readonly", "true");
    expect(screen.queryByRole("button", { name: "Save" })).not.toBeInTheDocument();

    await app.user.click(screen.getByRole("button", { name: "Edit" }));
    expect(screen.getByTestId("code-editor")).toHaveAttribute("data-readonly", "false");
    fireEvent.change(screen.getByLabelText("Editor text"), { target: { value: "new text" } });
    await app.user.click(await screen.findByRole("button", { name: "Save" }));
    expect(backend.callsTo("fs_write")).toEqual([
      { workspaceId: "w1", path: "README.md", text: "new text" },
    ]);
  });

  it("leaves the file with Escape and the close button", async () => {
    const app = open(() => {
      tree();
      backend.on("fs_read", text("README.md", "x"));
    });
    await screen.findByRole("tree", { name: "Files" });
    act(() => useReview.getState().openFile("w1", "README.md"));
    await screen.findByTestId("code-editor");
    // The file opens over the chat, which stays where it was.
    expect(screen.getByRole("region", { name: "Chat", hidden: true })).toBeInTheDocument();
    // Focus moves into the dialog, so Escape is meant for it.
    const dialog = await screen.findByRole("dialog");
    await waitFor(() => expect(dialog).toContainElement(document.activeElement as HTMLElement));
    await app.user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByTestId("code-editor")).not.toBeInTheDocument());
    expect(await screen.findByRole("region", { name: "Chat" })).toBeInTheDocument();

    act(() => useReview.getState().openFile("w1", "README.md"));
    await app.user.click(await screen.findByRole("button", { name: "Close file" }));
    await waitFor(() => expect(screen.queryByTestId("code-editor")).not.toBeInTheDocument());
  });
});

describe("changes", () => {
  const files = [
    change("src/a.ts", { added: 5, removed: 2 }),
    change("src/b.ts", { status: "added", added: 10, removed: 0 }),
    change("README.md", { status: "deleted", added: 0, removed: 4 }),
  ];
  const setup = () => {
    backend.on("changes_list", files);
    backend.on("diff_files", (args: Record<string, unknown>) =>
      pair(String(args.path), "before", "after"),
    );
  };

  it("lists changed files with their counts and a total, on its tab", async () => {
    const app = open(setup);
    const changesTab = await screen.findByRole("tab", { name: /Changes/ });
    await waitFor(() => expect(changesTab).toHaveTextContent("3"));
    await app.user.click(changesTab);
    const row = await screen.findByRole("button", { name: /a\.ts/ });
    expect(row).toHaveTextContent("+5");
    expect(row).toHaveTextContent("−2");
    expect(screen.getByLabelText("Added")).toBeInTheDocument();
    expect(screen.getByLabelText("Deleted")).toBeInTheDocument();
    expect(screen.getByText(/3 files/).parentElement).toHaveTextContent("+15");
  });

  it("opens on the uncommitted changes", async () => {
    const app = open(setup);
    await tab(app, /Changes/);
    await waitFor(() =>
      expect(backend.callsTo("changes_list").map((c) => c.scope)).toContain("uncommitted"),
    );
    expect(screen.getByRole("button", { name: /^Uncommitted/ })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });

  it("switches between all changes and uncommitted ones, and groups by folder", async () => {
    const app = open(setup);
    await tab(app, /Changes/);
    await app.user.click(screen.getByRole("button", { name: /^All changes/ }));
    await screen.findByRole("button", { name: /a\.ts/ });
    expect(backend.callsTo("changes_list").map((c) => c.scope)).toContain("all");
    await app.user.click(screen.getByRole("button", { name: /^Uncommitted/ }));
    await waitFor(() =>
      expect(backend.callsTo("changes_list").slice(-1)[0]).toMatchObject({ scope: "uncommitted" }),
    );
    await app.user.click(screen.getByRole("button", { name: /^All changes/ }));
    await screen.findByRole("button", { name: /a\.ts/ });

    await app.user.click(screen.getByRole("button", { name: "Group by folder" }));
    expect(await screen.findByRole("region", { name: "src" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Top level" })).toBeInTheDocument();
  });

  it("counts both kinds of changes, with the totals at the right and no note under them", async () => {
    const app = open(() => {
      backend.on("changes_list", (args: Record<string, unknown>) =>
        args.scope === "all" ? files : [],
      );
    });
    await tab(app, /Changes/);
    await app.user.click(await screen.findByRole("button", { name: /^All changes/ }));
    expect(await screen.findByRole("button", { name: "All changes 3" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Uncommitted 0" })).toBeInTheDocument();
    // The count of files is at the left, and the lines added and removed are at the right.
    const count = await screen.findByText(/3 files/);
    const footer = count.parentElement as HTMLElement;
    expect(footer.className).toMatch(/justify-between/);
    expect(footer.firstElementChild).toBe(count);
    expect(footer.lastElementChild).toHaveTextContent(/^\+\d+\s*−\d+$/);
    expect(screen.queryByText(/Compared with main/)).not.toBeInTheDocument();
    expect(screen.queryByText(/Commits already pushed/)).not.toBeInTheDocument();

    await app.user.click(screen.getByRole("button", { name: "Uncommitted 0" }));
    expect(
      await screen.findByText("Nothing uncommitted. The 3 changed files are all committed."),
    ).toBeInTheDocument();
  });

  it("says so when there is nothing to show", async () => {
    const app = open(() => backend.on("changes_list", []));
    await tab(app, /Changes/);
    await app.user.click(await screen.findByRole("button", { name: /^All changes/ }));
    expect(await screen.findByText("No changes on this branch yet.")).toBeInTheDocument();
  });

  it("refreshes while files change, and watches only while the workspace is open", async () => {
    const app = open(setup);
    await tab(app, /Changes/);
    await screen.findByRole("button", { name: /a\.ts/ });
    expect(backend.callsTo("files_watch")).toEqual([{ workspaceId: "w1" }]);

    backend.on("changes_list", [...files, change("src/new.ts")]);
    act(() => emitBackendEvent("workspace:files-changed", { workspace_id: "w1" }));
    expect(await screen.findByRole("button", { name: /new\.ts/ })).toBeInTheDocument();

    const before = backend.callsTo("changes_list").length;
    act(() => emitBackendEvent("workspace:files-changed", { workspace_id: "other" }));
    expect(backend.callsTo("changes_list")).toHaveLength(before);

    await app.router.navigate("/");
    await waitFor(() => expect(backend.callsTo("files_unwatch")).toEqual([{ workspaceId: "w1" }]));
  });

  it("opens the diff of a file, in order, with arrows to the neighbours", async () => {
    const app = open(setup);
    await tab(app, /Changes/);
    await app.user.click(await screen.findByRole("button", { name: /a\.ts/ }));
    const diff = await screen.findByTestId("diff-editor");
    expect(diff).toHaveAttribute("data-path", "src/a.ts");
    expect(screen.getByTestId("original")).toHaveTextContent("before");
    expect(screen.getByTestId("modified")).toHaveTextContent("after");
    expect(screen.getByText("1 of 3")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Previous file" })).toBeDisabled();

    await app.user.click(screen.getByRole("button", { name: "Next file" }));
    await waitFor(() =>
      expect(screen.getByTestId("diff-editor")).toHaveAttribute("data-path", "src/b.ts"),
    );
    expect(screen.getByText("2 of 3")).toBeInTheDocument();
    await app.user.click(screen.getByRole("button", { name: "Next file" }));
    await waitFor(() =>
      expect(screen.getByTestId("diff-editor")).toHaveAttribute("data-path", "README.md"),
    );
    expect(screen.getByRole("button", { name: "Next file" })).toBeDisabled();
    await app.user.click(screen.getByRole("button", { name: "Previous file" }));
    await waitFor(() =>
      expect(screen.getByTestId("diff-editor")).toHaveAttribute("data-path", "src/b.ts"),
    );
  });

  it("toggles side by side and one column, and closes", async () => {
    const app = open(setup);
    await tab(app, /Changes/);
    await app.user.click(await screen.findByRole("button", { name: /a\.ts/ }));
    expect(await screen.findByTestId("diff-editor")).toHaveAttribute("data-split", "true");
    await app.user.click(screen.getByRole("button", { name: "Show one column" }));
    expect(screen.getByTestId("diff-editor")).toHaveAttribute("data-split", "false");
    await app.user.click(screen.getByRole("button", { name: "Show side by side" }));
    expect(screen.getByTestId("diff-editor")).toHaveAttribute("data-split", "true");
    await app.user.keyboard("{Escape}");
    expect(screen.queryByTestId("diff-editor")).not.toBeInTheDocument();
  });

  it("describes binary and huge changes instead of comparing them", async () => {
    const app = open(() => {
      backend.on("changes_list", [change("logo.png", { binary: true }), change("big.log")]);
      backend.on("diff_files", (args: Record<string, unknown>) => ({
        ...pair(String(args.path), null, null),
        binary: args.path === "logo.png",
        too_large: args.path === "big.log",
      }));
    });
    await tab(app, /Changes/);
    await app.user.click(await screen.findByRole("button", { name: /logo\.png/ }));
    expect(await screen.findByText(/binary file/)).toBeInTheDocument();
    await app.user.click(screen.getByRole("button", { name: "Next file" }));
    expect(await screen.findByText(/too large to compare/)).toBeInTheDocument();
    expect(screen.queryByTestId("diff-editor")).not.toBeInTheDocument();
  });

  it("opens the first changed file with the shortcut and closes it with the same keys", async () => {
    const app = open(setup);
    await screen.findByRole("tree", { name: "Files" });
    await app.user.keyboard("{Meta>}{Shift>}d{/Shift}{/Meta}");
    expect(await screen.findByTestId("diff-editor")).toHaveAttribute("data-path", "src/a.ts");
    expect(screen.getByRole("tab", { name: /Changes/, hidden: true })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await app.user.keyboard("{Meta>}{Shift>}d{/Shift}{/Meta}");
    await waitFor(() => expect(screen.queryByTestId("diff-editor")).not.toBeInTheDocument());
  });

  it("reports a diff that cannot be read", async () => {
    const app = open(() => {
      backend.on("changes_list", files);
      backend.fail("diff_files", {
        code: "invalid",
        message: "That path is outside the workspace.",
      });
    });
    await tab(app, /Changes/);
    await app.user.click(await screen.findByRole("button", { name: /a\.ts/ }));
    expect(await screen.findByRole("alert")).toHaveTextContent("outside the workspace");
  });
});

describe("line comments", () => {
  const setup =
    (comments: Comment[] = []) =>
    () => {
      backend.on("changes_list", [change("src/a.ts")]);
      backend.on("diff_files", pair("src/a.ts", "a", "b"));
      backend.on("comment_list", comments);
      backend.on("comment_create", comment({ id: "new", body: "Added" }));
      backend.on("comment_set_status", comment({ status: "resolved" }));
      backend.on("comment_delete", null);
      backend.on("comment_send", null);
    };

  async function openDiff(app: ReturnType<typeof renderApp>) {
    await tab(app, /Changes/);
    await app.user.click(await screen.findByRole("button", { name: /a\.ts/ }));
    await screen.findByTestId("diff-editor");
  }

  it("adds a comment on the clicked line", async () => {
    const app = open(setup());
    await openDiff(app);
    await app.user.click(screen.getByRole("button", { name: "Gutter line 3" }));
    await app.user.type(await screen.findByLabelText(/Comment on line 3/), "Why is this here?");
    await app.user.click(screen.getByRole("button", { name: "Add comment" }));
    await waitFor(() => expect(backend.callsTo("comment_create")).toHaveLength(1));
    expect(backend.callsTo("comment_create")[0]).toEqual({
      workspaceId: "w1",
      comment: {
        path: "src/a.ts",
        line_start: 3,
        line_end: 3,
        body: "Why is this here?",
        source: "user",
      },
    });
    await waitFor(() =>
      expect(screen.queryByLabelText(/Comment on line 3/)).not.toBeInTheDocument(),
    );
  });

  it("comments on a range of lines and can be cancelled", async () => {
    const app = open(setup());
    await openDiff(app);
    await app.user.click(screen.getByRole("button", { name: "Gutter lines 5 to 7" }));
    expect(await screen.findByLabelText(/Comment on lines 5–7/)).toBeInTheDocument();
    await app.user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByLabelText(/Comment on lines/)).not.toBeInTheDocument();
    expect(backend.callsTo("comment_create")).toHaveLength(0);
  });

  it("does not add an empty comment", async () => {
    const app = open(setup());
    await openDiff(app);
    await app.user.click(screen.getByRole("button", { name: "Gutter line 3" }));
    expect(await screen.findByRole("button", { name: "Add comment" })).toBeDisabled();
  });

  it("marks commented lines and lists the comments of the file", async () => {
    const app = open(
      setup([comment(), comment({ id: "c2", path: "other.ts", body: "elsewhere" })]),
    );
    await openDiff(app);
    const dock = await screen.findByRole("region", { name: "Comments" });
    expect(within(dock).getByText("Rename this")).toBeInTheDocument();
    expect(within(dock).queryByText("elsewhere")).not.toBeInTheDocument();
    expect(screen.getByTestId("marks")).toHaveTextContent("3-3:open");
  });

  it("resolves, reopens and deletes a comment", async () => {
    const app = open(setup([comment()]));
    await openDiff(app);
    await app.user.click(await screen.findByRole("button", { name: "Resolve comment" }));
    expect(backend.callsTo("comment_set_status")[0]).toEqual({ id: "c1", resolved: true });
    await app.user.click(screen.getByRole("button", { name: "Delete comment" }));
    expect(backend.callsTo("comment_delete")).toEqual([{ id: "c1" }]);
  });

  it("shows a resolved comment as resolved and offers to reopen it", async () => {
    const app = open(setup([comment({ status: "resolved" })]));
    await openDiff(app);
    await app.user.click(await screen.findByRole("button", { name: "Reopen comment" }));
    expect(backend.callsTo("comment_set_status")[0]).toEqual({ id: "c1", resolved: false });
    expect(screen.getByTestId("marks")).toHaveTextContent("3-3:resolved");
    expect(screen.queryByRole("button", { name: /Send .* to chat/ })).not.toBeInTheDocument();
  });

  it("sends the open comments to the chat", async () => {
    const app = open(
      setup([comment(), comment({ id: "c2", line_start: 8, line_end: 8, body: "Second" })]),
    );
    await openDiff(app);
    await app.user.click(await screen.findByRole("button", { name: "Send 2 to chat" }));
    await waitFor(() => expect(backend.callsTo("comment_send")).toHaveLength(1));
    expect(backend.callsTo("comment_send")[0]).toEqual({
      workspaceId: "w1",
      sessionId: "s1",
      ids: ["c1", "c2"],
    });
  });

  it("comments on a line of a file as well", async () => {
    const app = open(() => {
      setup()();
      backend.on("fs_list", [entry("README.md")]);
      backend.on("fs_read", text("README.md", "line"));
    });
    await screen.findByRole("tree", { name: "Files" });
    act(() => useReview.getState().openFile("w1", "README.md"));
    await app.user.click(await screen.findByRole("button", { name: "Gutter line 3" }));
    await app.user.type(await screen.findByLabelText(/Comment on line 3/), "Fix");
    await app.user.click(screen.getByRole("button", { name: "Add comment" }));
    await waitFor(() =>
      expect(backend.callsTo("comment_create")[0]).toMatchObject({
        comment: { path: "README.md", line_start: 3 },
      }),
    );
  });
});

describe("workspace menu", () => {
  const menu = async (user: ReturnType<typeof renderApp>["user"]) =>
    user.click(await screen.findByRole("button", { name: "Actions for Algiers" }));

  it("opens the workspace in an installed editor, shows it in Finder and copies its path", async () => {
    const app = open(() => {
      backend.on("editor_list", [
        { id: "cursor", name: "Cursor" },
        { id: "vscode", name: "VS Code" },
      ]);
      backend.on("editor_open", null).on("fs_reveal", null);
    });
    const writeText = vi.spyOn(navigator.clipboard, "writeText");
    await menu(app.user);
    await app.user.click(await screen.findByRole("menuitem", { name: "Open in VS Code" }));
    expect(backend.callsTo("editor_open")).toEqual([
      { workspaceId: "w1", editor: "vscode", path: null },
    ]);

    await menu(app.user);
    await app.user.click(await screen.findByRole("menuitem", { name: "Copy path" }));
    expect(writeText).toHaveBeenCalledWith("/ws/algiers");

    await menu(app.user);
    await app.user.click(await screen.findByRole("menuitem", { name: "Reveal in Finder" }));
    expect(backend.callsTo("fs_reveal")).toEqual([{ workspaceId: "w1", path: null }]);
  });

  it("says when no editor was found", async () => {
    const app = open();
    await menu(app.user);
    expect(await screen.findByRole("menuitem", { name: /No editor found/ })).toBeInTheDocument();
  });
});
