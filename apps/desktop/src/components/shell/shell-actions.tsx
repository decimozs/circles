import { type QueryClient, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { useMatch, useNavigate } from "react-router";
import { toast } from "sonner";
import { useTheme } from "@/components/theme-provider";
import { useStopAllDialog } from "@/features/composers/stop-all";
import { useComposerDialog } from "@/features/composers/store";
import { startPullRequest } from "@/features/pr/flow";
import { projectKeys } from "@/features/projects/api";
import { terminalKeys } from "@/features/terminal/api";
import { useWorkspace, workspaceKeys } from "@/features/workspaces/api";
import { type Action, findShortcutConflicts, useActionRegistry } from "@/lib/actions";
import { errorKind, ipc } from "@/lib/ipc";
import { TOAST_WITH_ACTION_MS } from "@/lib/toast";
import { useReview } from "@/stores/review";
import { useUi } from "@/stores/ui";

/** Archives a workspace from a keyboard or palette action, offering to commit uncommitted work. */
export async function archiveWorkspace(
  queryClient: QueryClient,
  id: string,
  city: string,
  commitDirty: boolean,
): Promise<void> {
  try {
    await ipc.workspaceArchive(id, commitDirty);
    toast(`Archived ${city}`);
  } catch (error) {
    if (errorKind(error) === "dirty_tree" && error instanceof Error) {
      toast.warning(error.message, {
        duration: TOAST_WITH_ACTION_MS,
        action: {
          label: "Commit and archive",
          onClick: () => void archiveWorkspace(queryClient, id, city, true),
        },
      });
    } else {
      toast.error(error instanceof Error ? error.message : String(error));
    }
  } finally {
    queryClient.invalidateQueries({ queryKey: workspaceKeys.all });
    queryClient.invalidateQueries({ queryKey: projectKeys.all });
  }
}

/** Opens the diff of the first changed file, or closes the viewer when it is already open. */
async function toggleDiff(workspaceId: string) {
  const review = useReview.getState();
  if (review.views[workspaceId]?.kind === "diff") return review.closeView(workspaceId);
  try {
    const files = await ipc.changesList(workspaceId, review.scope);
    if (files.length === 0) return void toast("No changes to show yet");
    review.setRightTab("changes");
    review.openDiff(workspaceId, files[0].path, files[0].old_path);
  } catch (error) {
    toast.error(error instanceof Error ? error.message : String(error));
  }
}

/** Registers the shell's own actions. Mount once inside the router and theme providers. */
export function ShellActions() {
  const navigate = useNavigate();
  const { resolvedTheme, setTheme } = useTheme();
  const register = useActionRegistry((s) => s.register);
  const queryClient = useQueryClient();
  const workspaceId = useMatch("/workspace/:id")?.params.id;
  const workspace = useWorkspace(workspaceId).data;
  const onComposer = useMatch("/composer/:id/*") !== null;

  useEffect(() => {
    const ui = useUi.getState;
    const actions: Action[] = [
      ...(workspaceId
        ? [
            {
              id: "review.diff",
              title: "Toggle diff viewer",
              group: "Review" as const,
              shortcuts: [{ code: "KeyD", mod: true, shift: true }],
              keywords: ["changes", "diff", "review"],
              run: () => void toggleDiff(workspaceId),
            },
            {
              id: "pr.create",
              title: "Create pull request",
              group: "Git" as const,
              shortcuts: [{ code: "KeyP", mod: true, shift: true }],
              keywords: ["pr", "github", "push"],
              run: () => void startPullRequest(queryClient, workspaceId),
            },
            {
              id: "review.files",
              title: "Show all files",
              group: "Review" as const,
              keywords: ["tree", "browse"],
              run: () => useReview.getState().setRightTab("files"),
            },
            {
              id: "review.changes",
              title: "Show changes",
              group: "Review" as const,
              keywords: ["git", "diff"],
              run: () => useReview.getState().setRightTab("changes"),
            },
          ]
        : []),
      {
        id: "composers.open",
        title: "Open Composers",
        group: "Composers",
        keywords: ["manager", "agent", "orchestrate"],
        run: () => navigate("/composers"),
      },
      {
        id: "composers.new",
        title: "New Composer",
        group: "Composers",
        keywords: ["create composer", "manager"],
        run: () => useComposerDialog.getState().openCreate(),
      },
      {
        id: "composers.audit",
        title: "Open the Composer audit log",
        group: "Composers",
        keywords: ["history", "tool calls", "log"],
        run: () => navigate("/composers/audit"),
      },
      {
        id: "composers.pause",
        title: "Pause or resume Composers",
        group: "Composers",
        keywords: ["halt", "freeze"],
        run: () =>
          void ipc
            .composersStatus()
            .then((status) => ipc.composersPause(!status.paused))
            .then((status) => {
              queryClient.invalidateQueries({ queryKey: ["composers-status"] });
              toast(status.paused ? "Composers are paused" : "Composers are running");
            })
            .catch((e) => toast.error(e instanceof Error ? e.message : String(e))),
      },
      {
        id: "composers.stop",
        title: "Stop all Composers",
        group: "Composers",
        shortcuts: [{ code: "Period", mod: true, shift: true }],
        keywords: ["kill switch", "emergency", "halt everything"],
        run: () => useStopAllDialog.getState().setOpen(true),
      },
      {
        id: "nav.agent-debug",
        title: "Agent debug",
        group: "App",
        keywords: ["claude", "chat", "agent", "developer", "diagnostics"],
        run: () => navigate("/debug/agent"),
      },
      {
        id: "nav.back",
        title: "Go back",
        group: "Navigation",
        shortcuts: [{ code: "BracketLeft", mod: true }],
        run: () => navigate(-1),
      },
      {
        id: "nav.forward",
        title: "Go forward",
        group: "Navigation",
        shortcuts: [{ code: "BracketRight", mod: true }],
        run: () => navigate(1),
      },
      {
        id: "nav.settings",
        title: "Open settings",
        group: "Navigation",
        shortcuts: [{ code: "Comma", mod: true }],
        run: () => navigate("/settings/general"),
      },
      {
        id: "workspace.create",
        title: "Create workspace",
        group: "Workspaces",
        shortcuts: [{ code: "KeyN", mod: true }],
        run: () => ui().openCreate(workspace?.project_id ?? null),
      },
      {
        id: "section.create",
        title: "Create section",
        group: "Workspaces",
        run: () => ui().setCreateSectionOpen(true),
      },
      {
        id: "project.add",
        title: "Add project",
        group: "Workspaces",
        shortcuts: [{ code: "KeyA", mod: true, alt: true }],
        run: () => ui().setAddProjectOpen(true),
      },
      {
        id: "app.search",
        title: "Search",
        group: "App",
        shortcuts: [{ code: "KeyK", mod: true }],
        run: () => ui().setPaletteOpen(!ui().paletteOpen),
      },
      {
        id: "app.toggle-left",
        title: "Toggle left sidebar",
        group: "App",
        shortcuts: [{ code: "KeyB", mod: true }, { code: "BracketLeft" }],
        run: () => ui().toggleLeft(),
      },
      {
        id: "app.toggle-right",
        title: "Toggle right sidebar",
        group: "App",
        shortcuts: [{ code: "KeyB", mod: true, alt: true }, { code: "BracketRight" }],
        // A Composer's page has a popover where a workspace has a pane.
        run: () => (onComposer ? ui().setRunOpen(!ui().runOpen) : ui().toggleRight()),
      },
      {
        id: "app.zen",
        title: "Toggle zen mode",
        group: "App",
        shortcuts: [{ code: "Period", mod: true }],
        run: () => ui().toggleZen(),
      },
      {
        id: "app.theme",
        title: "Toggle theme",
        group: "App",
        shortcuts: [{ code: "KeyT", mod: true, alt: true }],
        run: () => setTheme(resolvedTheme === "dark" ? "light" : "dark"),
      },
      {
        id: "app.shortcuts",
        title: "Keyboard shortcuts",
        group: "App",
        shortcuts: [{ code: "Slash", mod: true }],
        run: () => ui().setShortcutsOpen(!ui().shortcutsOpen),
      },
    ];

    const live =
      workspace &&
      ["ready", "idle", "working", "blocked", "setup_running"].includes(workspace.state);
    if (workspace && live) {
      const show = (terminalId: string) => {
        ui().selectDockTab(workspace.id, terminalId);
        ui().setDockOpen(true);
        ui().setBigTerminal(false);
      };
      const refresh = () => queryClient.invalidateQueries({ queryKey: terminalKeys.all });
      actions.push(
        {
          id: "script.run",
          title: "Run",
          group: "Terminal",
          shortcuts: [{ code: "KeyR", mod: true }],
          keywords: ["start dev", "run script", "start"],
          run: () =>
            ipc
              .scriptRun(workspace.id)
              .then((terminal) => show(terminal.id))
              .catch((e: Error) => toast.error(e.message))
              .finally(refresh),
        },
        {
          id: "script.stop",
          title: "Stop run script",
          group: "Terminal",
          shortcuts: [{ code: "KeyR", mod: true, shift: true }],
          keywords: ["stop dev", "kill"],
          run: () =>
            ipc
              .scriptStop(workspace.id)
              .catch((e: Error) => toast.error(e.message))
              .finally(refresh),
        },
        {
          id: "terminal.new",
          title: "New terminal",
          group: "Terminal",
          shortcuts: [{ code: "KeyT", mod: true }],
          run: () =>
            ipc
              .terminalCreate(workspace.id)
              .then((terminal) => show(terminal.id))
              .catch((e: Error) => toast.error(e.message))
              .finally(refresh),
        },
        {
          id: "terminal.toggle",
          title: "Toggle terminal",
          group: "Terminal",
          // Two ways to the same thing: opening and closing, and nothing else.
          shortcuts: [
            { code: "KeyJ", mod: true },
            { code: "KeyJ", mod: true, shift: true },
          ],
          run: () => ui().toggleDock(),
        },
        {
          id: "terminal.big",
          title: "Maximize terminal",
          group: "Terminal",
          keywords: ["fullscreen terminal", "big terminal", "restore terminal"],
          shortcuts: [{ code: "KeyM", mod: true, shift: true }],
          run: () => ui().toggleBigTerminal(),
        },
      );
    }

    if (workspace && workspace.state !== "archived") {
      actions.push({
        id: "workspace.archive",
        title: `Archive ${workspace.city}`,
        group: "Workspaces",
        keywords: ["archive workspace"],
        run: () => void archiveWorkspace(queryClient, workspace.id, workspace.city, false),
      });
    }

    if (import.meta.env.DEV) {
      for (const conflict of findShortcutConflicts(actions)) {
        console.warn(`Shortcut conflict: ${conflict}`);
      }
    }
    return register("shell", actions);
  }, [
    navigate,
    onComposer,
    queryClient,
    register,
    resolvedTheme,
    setTheme,
    workspace,
    workspaceId,
  ]);

  return null;
}
