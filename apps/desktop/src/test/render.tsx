import { TooltipProvider } from "@circles/ui/components/tooltip";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { createMemoryRouter, RouterProvider } from "react-router";
import { Toaster } from "@/components/sonner";
import { ThemeProvider } from "@/components/theme-provider";
import { useStopAllDialog } from "@/features/composers/stop-all";
import { useComposerDialog } from "@/features/composers/store";
import { routes } from "@/routes/router";
import { useCreatePrefs } from "@/stores/create-prefs";
import { useReview } from "@/stores/review";
import { useUi } from "@/stores/ui";
import { backend } from "@/test/backend";
import {
  appInfo,
  defaultTabs,
  project,
  readiness,
  scripts,
  session,
  settingsView,
  toolCatalog,
  workspace,
} from "@/test/fixtures";

/** Backend answers every screen needs. Tests override what they care about. */
export function defaultBackend() {
  backend
    .on("get_app_info", appInfo())
    .on("session_auto_title", null)
    .on("genui_cards", [])
    .on("genui_pending", [])
    .on("smart_suggest", null)
    .on("smart_status", { embedded_available: true, provider: "embedded" })
    .on("jira_status", {
      connected: false,
      site: null,
      email: null,
      account_name: null,
      needs_reconnect: false,
      method: null,
      choose_site: [],
    })
    .on("composer_skills", [])
    .on("project_list", [project()])
    .on("workspace_list", [workspace()])
    .on("section_list", [])
    .on("workspace_get", (args: Record<string, unknown>) => workspace({ id: String(args.id) }))
    .on("workspace_events", [])
    .on("workspace_open", null)
    .on("terminal_list", defaultTabs())
    .on("scripts_get", scripts())
    .on("terminal_attach", { running: true })
    .on("terminal_detach", null)
    .on("terminal_write", null)
    .on("terminal_resize", null)
    .on("files_watch", null)
    .on("files_unwatch", null)
    .on("changes_list", [])
    .on("fs_list", [])
    .on("editor_list", [])
    .on("comment_list", [])
    .on("pr_readiness", readiness({ next_action: "none" }))
    .on("pr_checks", [])
    .on("pr_threads", [])
    .on("pr_deployments", [])
    .on("todo_list", [])
    .on("workspace_branch_states", [])
    .on("composer_list", [])
    .on("composer_runs", [])
    .on("composer_approvals", [])
    .on("composer_fleet", [])
    .on("composer_shared_list", [])
    .on("composer_workspace_links", [])
    .on("trigger_list", [])
    .on("audit_list", [])
    .on("tool_catalog", toolCatalog())
    .on("composers_status", { paused: false, running: 0, waiting_approvals: 0 })
    .on("git_graph", {
      commits: [],
      branch: "feature",
      base: "origin/main",
      ahead: 0,
      behind: 0,
      truncated: false,
    });
  chatBackend();
  settingsBackend();
}

/** Settings that remember what is written, like the real files do. Returns the levels. */
export function settingsBackend(initial: Parameters<typeof settingsView>[0] = {}) {
  const levels = structuredClone(initial) as Record<string, Record<string, unknown>>;
  const view = () => settingsView(levels);
  backend
    .on("settings_view", view)
    .on("settings_watch", null)
    .on("settings_unwatch", null)
    .on("settings_set", (args: Record<string, unknown>) => {
      const scope = args.scope === "local" ? "local_project" : String(args.scope);
      const path = args.path as string[];
      levels[scope] = levels[scope] ?? {};
      let cur = levels[scope] as Record<string, unknown>;
      for (const key of path.slice(0, -1)) {
        cur[key] = cur[key] ?? {};
        cur = cur[key] as Record<string, unknown>;
      }
      const last = path[path.length - 1] as string;
      if (args.value === null || args.value === undefined) delete cur[last];
      else cur[last] = args.value;
      return view();
    });
  return levels;
}

/** Chat answers: a workspace starts with one empty chat, and new chats join the list. */
export function chatBackend(existing = [session()]) {
  const sessions = [...existing];
  backend
    .on("session_list", () => sessions)
    .on("session_create", (args: Record<string, unknown>) => {
      const created = session({
        id: `s${sessions.length + 1}`,
        workspace_id: String(args.workspaceId),
      });
      sessions.push(created);
      return created;
    })
    .on(
      "session_get",
      (args: Record<string, unknown>) => sessions.find((s) => s.id === args.id) ?? sessions[0],
    )
    .on("session_attach", (args: Record<string, unknown>) => ({
      session: sessions.find((s) => s.id === args.id) ?? sessions[0],
      pending: [],
    }))
    .on("session_detach", null)
    .on("session_messages", [])
    .on("claude_rate_limit", null)
    .on("workspace_take_pending_prompt", null)
    .on("slash_commands", [])
    .on("workspace_search_files", [])
    .on("session_send", null)
    .on("session_rename", (args: Record<string, unknown>) =>
      session({ id: String(args.id), title: String(args.title) }),
    )
    .on("session_interrupt", null)
    .on("session_update", (args: Record<string, unknown>) =>
      session({ id: String(args.id), model: String(args.model ?? "opus") }),
    )
    .on("permission_reply", null)
    .on("session_close", null)
    .on("session_reopen", session())
    .on("session_reorder", null)
    .on("checkpoint_list", [])
    .on("checkpoint_last_revert", null);
  return sessions;
}

export function resetStores() {
  useUi.setState(useUi.getInitialState(), true);
  useCreatePrefs.setState(useCreatePrefs.getInitialState(), true);
  useReview.setState(useReview.getInitialState(), true);
  useComposerDialog.setState({ dialog: null });
  useStopAllDialog.setState({ open: false });
}

/** Renders the whole app shell at `route`, with the fake backend. */
export function renderApp(route = "/", ui: Partial<ReturnType<typeof useUi.getInitialState>> = {}) {
  resetStores();
  useUi.setState(ui);
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 }, mutations: { retry: false } },
  });
  const router = createMemoryRouter(routes, { initialEntries: [route] });
  // Menus and dialogs set `pointer-events: none` on the page while they animate closed.
  const user = userEvent.setup({ pointerEventsCheck: 0 });
  render(
    <QueryClientProvider client={queryClient}>
      <ThemeProvider>
        <TooltipProvider>
          <RouterProvider router={router} />
          <Toaster />
        </TooltipProvider>
      </ThemeProvider>
    </QueryClientProvider>,
  );
  return { router, queryClient, user };
}
