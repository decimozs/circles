import type { AppInfo } from "@circles/bindings/AppInfo";
import type { Approval } from "@circles/bindings/Approval";
import type { BranchInfo } from "@circles/bindings/BranchInfo";
import type { Composer } from "@circles/bindings/Composer";
import type { ComposerTool } from "@circles/bindings/ComposerTool";
import type { Project } from "@circles/bindings/Project";
import type { PrStatus } from "@circles/bindings/PrStatus";
import type { PullRequest } from "@circles/bindings/PullRequest";
import type { Readiness } from "@circles/bindings/Readiness";
import type { RunView } from "@circles/bindings/RunView";
import type { ScriptsInfo } from "@circles/bindings/ScriptsInfo";
import type { Session } from "@circles/bindings/Session";
import type { Settings } from "@circles/bindings/Settings";
import type { SettingsView } from "@circles/bindings/SettingsView";
import type { Terminal } from "@circles/bindings/Terminal";
import type { Workspace } from "@circles/bindings/Workspace";

export const NOW = Date.now();
export const HOUR = 3_600_000;

export function project(overrides: Partial<Project> = {}): Project {
  return {
    id: "p1",
    name: "roster",
    path: "/code/roster",
    path_exists: true,
    default_branch: "main",
    remote_url: null,
    avatar_url: null,
    workspace_count: 0,
    created_at: NOW,
    updated_at: NOW,
    ...overrides,
  };
}

export function workspace(overrides: Partial<Workspace> = {}): Workspace {
  return {
    id: "w1",
    project_id: "p1",
    city: "Algiers",
    title: null,
    branch: "me/abc12",
    base_branch: "main",
    path: "/home/circles/workspaces/roster/algiers",
    state: "ready",
    error: null,
    pinned: false,
    section_id: null,
    pr_number: null,
    pr_url: null,
    created_at: NOW - HOUR,
    updated_at: NOW - HOUR,
    last_opened_at: null,
    archived_at: null,
    pending_prompt: null,
    pending_model: null,
    pending_effort: null,
    ...overrides,
  };
}

export function branch(overrides: Partial<BranchInfo> = {}): BranchInfo {
  return { name: "main", local: true, remote: true, checked_out_at: null, ...overrides };
}

export function pullRequest(overrides: Partial<PullRequest> = {}): PullRequest {
  return {
    number: 12,
    title: "Fix login",
    head_ref: "me/fix-login",
    author: "me",
    is_cross_repository: false,
    updated_at: "2026-10-01T10:00:00Z",
    ...overrides,
  };
}

export function appInfo(overrides: Partial<AppInfo> = {}): AppInfo {
  return {
    version: "0.1.0",
    machine_name: "Test Mac",
    platform: "macos",
    arch: "aarch64",
    paths: {
      data_dir: "/data",
      db_file: "/data/circles.db",
      settings_file: "/data/settings.toml",
      logs_dir: "/data/logs",
      home_dir: "/home/circles",
      workspaces_dir: "/home/circles/workspaces",
      repos_dir: "/home/circles/repos",
    },
    env: { source: { kind: "login_shell" }, var_count: 1, path_entries: [], tools: [] },
    db: { schema_version: 3, journal_mode: "wal" },
    ...overrides,
  };
}

export function terminal(overrides: Partial<Terminal> = {}): Terminal {
  return {
    id: "t-term",
    workspace_id: "w1",
    kind: "terminal",
    title: "Terminal",
    position: 2,
    running: false,
    has_output: false,
    exit: null,
    script: null,
    ...overrides,
  };
}

/** The tabs every workspace starts with. */
export function defaultTabs(
  overrides: {
    setup?: Partial<Terminal>;
    run?: Partial<Terminal>;
    terminal?: Partial<Terminal>;
  } = {},
): Terminal[] {
  return [
    terminal({ id: "t-setup", kind: "setup", title: "Setup", position: 0, ...overrides.setup }),
    terminal({ id: "t-run", kind: "run", title: "Run", position: 1, ...overrides.run }),
    terminal({ ...overrides.terminal }),
  ];
}

export function scripts(overrides: Partial<ScriptsInfo> = {}): ScriptsInfo {
  return { setup: null, run: [], archive: null, run_mode: "concurrent", ...overrides };
}

export function session(overrides: Partial<Session> = {}): Session {
  return {
    id: "s1",
    workspace_id: "w1",
    claude_session_id: null,
    title: null,
    status: "idle",
    model: "opus",
    effort: "high",
    permission_mode: "manual",
    position: 0,
    total_cost_usd: 0,
    turns: 0,
    last_error: null,
    running: false,
    created_at: NOW,
    updated_at: NOW,
    working_since: null,
    ...overrides,
  };
}

export function prStatus(overrides: Partial<PrStatus> = {}): PrStatus {
  return {
    number: 42,
    url: "https://github.com/o/r/pull/42",
    title: "Add login",
    state: "open",
    is_draft: false,
    mergeable: "clean",
    merge_state: "clean",
    review_decision: null,
    head_ref: "me/abc12",
    base_ref: "main",
    ...overrides,
  };
}

/** A branch with commits and no pull request yet, unless the overrides say otherwise. */
export function readiness(overrides: Partial<Readiness> = {}): Readiness {
  return {
    git: {
      branch: "me/abc12",
      base: "main",
      ahead: 2,
      behind: 0,
      dirty: false,
      staged: 0,
      unstaged: 0,
      untracked: 0,
      conflicts: 0,
      conflicts_with_base: false,
      pushed: false,
    },
    pr: null,
    checks: { total: 0, passed: 0, failed: 0, pending: 0 },
    unresolved_threads: 0,
    blockers: [],
    next_action: "create_pr",
    ready: false,
    gh_error: null,
    ...overrides,
  };
}

type Values = Record<string, unknown>;

/** Settings with nothing set. */
export function emptySettings(): Settings {
  return {
    instructions: {
      general: null,
      review: null,
      pr: null,
      fix_errors: null,
      conflicts: null,
      branch_rename: null,
      commits: null,
    },
    git: { merge_method: null, branch_prefix: null, branch_pattern: null },
    general: {
      send_key: null,
      follow_up: null,
      notifications: null,
      sound: null,
      long_text_to_attachment: null,
    },
    appearance: { theme: null, font_size: null, density: null },
    models: { model: null, effort: null, plan_by_default: null },
    agents: { claude_path: null, permission_mode: null, extra_args: [] },
    advanced: { log_level: null, checkpoint_days: null },
    composers: {
      daily_run_cap: null,
      menu_bar: null,
      keep_awake: null,
      start_at_login: null,
      notify_waiting: null,
    },
    integrations: { jira_in_chats: null },
    smart: { titles: null, suggestions: null, first_prompt: null },
    side: { model: null, read_files: null },
    genui: { composers: null, chats: null },
    experimental: {},
    scripts: { setup: null, run: null, archive: null, run_mode: null },
    environment_variables: {},
    files_to_copy: [],
  };
}

function merge(target: Values, source: Values): Values {
  for (const [key, value] of Object.entries(source)) {
    const cur = target[key];
    if (
      value &&
      typeof value === "object" &&
      !Array.isArray(value) &&
      cur &&
      typeof cur === "object"
    ) {
      merge(cur as Values, value as Values);
    } else {
      target[key] = value;
    }
  }
  return target;
}

/** A settings view built from what each level holds. */
export function settingsView(
  levels: Partial<Record<"user" | "project" | "local_project", Values>> = {},
): SettingsView {
  const settings = emptySettings() as unknown as Values;
  for (const scope of ["user", "project", "local_project"] as const) {
    merge(settings, structuredClone(levels[scope] ?? {}));
  }
  return {
    settings: settings as unknown as Settings,
    layers: [],
    values: (["user", "project", "local_project"] as const).map((scope) => ({
      scope,
      values: levels[scope] ?? {},
    })),
  };
}

export function composer(overrides: Partial<Composer> = {}): Composer {
  return {
    id: "c1",
    name: "Release captain",
    instructions: "Keep main green.",
    workspace_id: "cw1",
    mode: "manual",
    autonomy: {},
    integrations: [],
    projects: null,
    limits: {
      max_budget_usd: null,
    },
    allow_override: false,
    notes: "",
    model: null,
    effort: null,
    pinned: false,
    created_at: NOW,
    updated_at: NOW,
    ...overrides,
  };
}

/** A few tools of each tier, enough for the screens that list them. */
export function toolCatalog(): ComposerTool[] {
  const t = (name: string, tier: ComposerTool["tier"]): ComposerTool => ({
    name,
    description: `${name} description`,
    tier,
    repeat_safe: tier === "read",
  });
  return [
    t("list_projects", "read"),
    t("get_workspace", "read"),
    t("create_workspace", "write"),
    t("start_chat", "write"),
    t("create_pr", "external"),
    t("merge_pr", "external"),
    t("delete_workspace", "destructive"),
  ];
}

export function runView(
  overrides: Partial<RunView> & { id?: string; title?: string | null } = {},
): RunView {
  const { id = "r1", title = "Triage open PRs", ...rest } = overrides;
  return {
    session: session({ id, workspace_id: "cw1", title, updated_at: NOW - 3 * 60_000 }),
    started_by: "person",
    pending_approvals: 0,
    ...rest,
  };
}

export function approval(overrides: Partial<Approval> = {}): Approval {
  return {
    id: "a1",
    composer_id: "c1",
    session_id: "r1",
    tool: "delete_workspace",
    tier: "destructive",
    input: JSON.stringify({ workspace_id: "w9" }),
    status: "pending",
    created_at: NOW,
    answered_at: null,
    ...overrides,
  };
}
