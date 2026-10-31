import { EFFORTS, MODELS } from "@/lib/models";

export type Control =
  | { kind: "toggle"; default: boolean }
  | {
      kind: "select";
      options: { value: string; label: string }[];
      default?: string;
      /** Written to the file as a number. */
      numeric?: boolean;
    }
  | { kind: "text"; placeholder?: string; mono?: boolean }
  | { kind: "textarea"; placeholder?: string; mono?: boolean }
  | { kind: "list"; placeholder?: string }
  | { kind: "custom" };

export type SettingDef = {
  id: string;
  page: string;
  title: string;
  description: string;
  /** Where the value lives in the settings file. Custom rows have none. */
  path: string[];
  control: Control;
  keywords?: string;
  /** A smaller line under the description. */
  hint?: string;
};

const opts = (...pairs: [string, string][]) => pairs.map(([value, label]) => ({ value, label }));

/** Every row of the personal and agent pages. The search looks through the same list. */
export const SETTINGS: SettingDef[] = [
  {
    id: "send_key",
    page: "general",
    title: "Send messages with",
    description: "The key that sends a chat message. The other one breaks the line.",
    path: ["general", "send_key"],
    control: {
      kind: "select",
      options: opts(["enter", "Enter"], ["mod_enter", "⌘ Enter"]),
      default: "enter",
    },
    keywords: "keyboard shortcut return",
    hint: "Use ⇧↵ for new lines",
  },
  {
    id: "follow_up",
    page: "general",
    title: "Follow-up behaviour",
    description:
      "What a message sent while the agent works does: steer the current turn or wait in a queue.",
    path: ["general", "follow_up"],
    control: {
      kind: "select",
      options: opts(["steer", "Steer"], ["queue", "Queue"]),
      default: "steer",
    },
    keywords: "queue steer interrupt",
    hint: "Use ⌘↵ for the other one",
  },
  {
    id: "notifications",
    page: "general",
    title: "Notifications",
    description:
      "A system notification when the agent finishes or needs you while Circles is in the background.",
    path: ["general", "notifications"],
    control: { kind: "toggle", default: true },
    keywords: "alerts desktop",
  },
  {
    id: "sound",
    page: "general",
    title: "Completion sound",
    description: "Play a sound with the notification.",
    path: ["general", "sound"],
    control: { kind: "toggle", default: true },
    keywords: "audio chime",
  },
  {
    id: "sound_test",
    page: "general",
    title: "Test notification",
    description: "Send a test notification and play the sound.",
    path: [],
    control: { kind: "custom" },
    keywords: "sound try",
  },
  {
    id: "full_disk_access",
    page: "general",
    title: "Full disk access",
    description: "Open the macOS privacy settings so agents can read protected folders.",
    path: [],
    control: { kind: "custom" },
    keywords: "privacy permissions macos",
  },
  {
    id: "long_text",
    page: "general",
    title: "Long text as attachment",
    description:
      "Pasted text over 5000 characters becomes an attachment instead of filling the box.",
    path: ["general", "long_text_to_attachment"],
    control: { kind: "toggle", default: true },
    keywords: "paste",
  },
  {
    id: "theme",
    page: "appearance",
    title: "Theme",
    description: "Light, dark, or follow the system.",
    path: ["appearance", "theme"],
    control: {
      kind: "select",
      options: opts(["system", "System"], ["light", "Light"], ["dark", "Dark"]),
      default: "system",
    },
    keywords: "dark mode light colours",
  },
  {
    id: "font_size",
    page: "appearance",
    title: "Font size",
    description: "The size of text across the app.",
    path: ["appearance", "font_size"],
    control: {
      kind: "select",
      options: opts(["small", "Small"], ["medium", "Medium"], ["large", "Large"]),
      default: "medium",
    },
    keywords: "text zoom",
  },
  {
    id: "density",
    page: "appearance",
    title: "Sidebar density",
    description: "How much room workspace rows take in the sidebar.",
    path: ["appearance", "density"],
    control: {
      kind: "select",
      options: opts(["comfortable", "Comfortable"], ["compact", "Compact"]),
      default: "comfortable",
    },
    keywords: "compact spacing",
  },
  {
    id: "model",
    page: "models",
    title: "Model",
    description: "The model a new chat starts with.",
    path: ["models", "model"],
    control: { kind: "select", options: MODELS.map((m) => ({ value: m.id, label: m.label })) },
    keywords: "claude opus sonnet haiku",
  },
  {
    id: "effort",
    page: "models",
    title: "Effort",
    description: "How hard a new chat thinks.",
    path: ["models", "effort"],
    control: { kind: "select", options: EFFORTS.map((e) => ({ value: e.id, label: e.label })) },
    keywords: "thinking reasoning",
  },
  {
    id: "plan_by_default",
    page: "models",
    title: "Start chats in plan mode",
    description: "New chats plan first and change nothing until you approve.",
    path: ["models", "plan_by_default"],
    control: { kind: "toggle", default: false },
    keywords: "planning",
  },
  {
    id: "branch_prefix",
    page: "git",
    title: "Branch prefix",
    description: "Put in front of generated branch names, in place of your GitHub login.",
    path: ["git", "branch_prefix"],
    control: { kind: "text", placeholder: "your GitHub login", mono: true },
    keywords: "naming",
  },
  {
    id: "branch_pattern",
    page: "git",
    title: "Branch naming rule",
    description:
      "How branches of this project are named, such as {ticket}-{slug} or feature/{slug}. {type} is feat, fix, chore and so on, {slug} lowercase words with dashes, {ticket} an issue key like PROJ-123. Composers follow it. Left empty, {type}/{slug}.",
    path: ["git", "branch_pattern"],
    control: { kind: "text", placeholder: "{type}/{slug}", mono: true },
    keywords: "naming branch pattern ticket convention",
  },
  {
    id: "merge_method",
    page: "git",
    title: "Merge strategy",
    description: "How pull requests are merged.",
    path: ["git", "merge_method"],
    control: {
      kind: "select",
      options: opts(["squash", "Squash"], ["merge", "Merge commit"], ["rebase", "Rebase"]),
      default: "squash",
    },
    keywords: "squash rebase pull request",
  },
  {
    id: "identity",
    page: "git",
    title: "Commit author",
    description: "The name and email git uses for commits, read from your git configuration.",
    path: [],
    control: { kind: "custom" },
    keywords: "name email identity",
  },
  {
    id: "github_account",
    page: "account",
    title: "GitHub",
    description:
      "The GitHub account Circles works as, through the GitHub CLI. Switch account changes the one the CLI uses.",
    path: [],
    control: { kind: "custom" },
    keywords: "login profile github gh sign in account switch",
  },
  {
    id: "claude_account",
    page: "account",
    title: "Claude",
    description: "The Claude account Claude Code is signed in with, and its plan.",
    path: [],
    control: { kind: "custom" },
    keywords: "login profile claude anthropic subscription plan sign in account",
  },
  {
    id: "claude_path",
    page: "agents",
    title: "Claude Code path",
    description: "A `claude` to use instead of the one found on your PATH.",
    path: ["agents", "claude_path"],
    control: { kind: "text", placeholder: "found on PATH", mono: true },
    keywords: "binary executable cli",
  },
  {
    id: "claude_check",
    page: "agents",
    title: "Claude Code version",
    description: "Check that Claude Code runs and which version it is.",
    path: [],
    control: { kind: "custom" },
    keywords: "install verify credentials login",
  },
  {
    id: "permission_mode",
    page: "agents",
    title: "Permission mode",
    description: "What new chats may do without asking.",
    path: ["agents", "permission_mode"],
    control: {
      kind: "select",
      options: opts(
        ["manual", "Ask for everything"],
        ["accept_edits", "Accept edits"],
        ["plan", "Plan only"],
        ["bypass", "Bypass permissions"],
      ),
      default: "manual",
    },
    keywords: "approve allow bypass dangerous",
  },
  {
    id: "extra_args",
    page: "agents",
    title: "Extra arguments",
    description: "Added to every Claude Code start, one per line.",
    path: ["agents", "extra_args"],
    control: { kind: "list", placeholder: "--verbose" },
    keywords: "flags cli",
  },
  {
    id: "env_vars",
    page: "environment",
    title: "Environment variables",
    description: "Set for every agent and terminal. Values are hidden here and never logged.",
    path: ["environment_variables"],
    control: { kind: "custom" },
    keywords: "secrets api key token",
  },
  {
    id: "genui_composers",
    page: "composers",
    title: "Let Composers ask with choices and forms",
    description:
      "A Composer can show a small card (a checklist, a form, a plan to tick) and read your answer, instead of asking in prose. Cards wait for you like an approval does.",
    path: ["genui", "composers"],
    control: { kind: "toggle", default: true },
    keywords: "cards forms choices ask generative ui",
  },
  {
    id: "genui_chats",
    page: "agents",
    title: "Let chats ask with choices and forms",
    description:
      "An ordinary chat can show a card to ask you to choose or fill something in. A card waits ten minutes for an answer. Takes effect for chats that start after this.",
    path: ["genui", "chats"],
    control: { kind: "toggle", default: false },
    keywords: "cards forms choices ask generative ui",
  },
  {
    id: "side_model",
    page: "agents",
    title: "Model for side questions",
    description:
      "/btw asks a quick question about a chat and answers in a pane above the box, without changing the chat. Haiku is fast and cheap. The chat's own model can use the same cache, and costs what it costs.",
    path: ["side", "model"],
    control: {
      kind: "select",
      options: opts(["haiku", "Haiku"], ["chat", "The chat's own model"]),
      default: "haiku",
    },
    keywords: "btw side question pane",
  },
  {
    id: "side_read_files",
    page: "agents",
    title: "Let side questions read files",
    description:
      "Off: a side question answers from the conversation only. On: it may also read files in the chat's folder. It can never change anything either way.",
    path: ["side", "read_files"],
    control: { kind: "toggle", default: false },
    keywords: "btw side question files",
  },
  {
    id: "smart_titles",
    page: "smart",
    title: "Name new chats",
    description:
      "Claude Haiku, through Claude Code, gives a new chat a short title from what you asked. It spends a little of your Claude usage and sends the start of your first message to Anthropic. Your own name for a chat is never replaced.",
    path: ["smart", "titles"],
    control: { kind: "toggle", default: true },
    keywords: "title name chat haiku anthropic automatic",
  },
  {
    id: "smart_first_prompt",
    page: "smart",
    title: "Show an example of what to ask a Composer",
    description:
      "A new Composer's first message box shows an example written from its instructions. Press Tab to use it.",
    path: ["smart", "first_prompt"],
    control: { kind: "toggle", default: true },
    keywords: "composer placeholder example first message",
  },
  {
    id: "smart_suggestions",
    page: "smart",
    title: "Suggest the next message",
    description:
      "After an answer in a chat, the empty message box shows a likely next message. Press Tab to use it. Nothing is sent until you do.",
    path: ["smart", "suggestions"],
    control: { kind: "toggle", default: false },
    keywords: "placeholder suggestion autocomplete next prompt",
  },
  {
    id: "smart_test",
    page: "smart",
    title: "Try it",
    description: "Asks Claude Haiku about an example and shows what it wrote and how long it took.",
    path: [],
    control: { kind: "custom" },
    keywords: "test check model",
  },
  {
    id: "jira_card",
    page: "integrations",
    title: "Jira",
    description: "Connect Jira Cloud so chats and Composers can use it.",
    path: [],
    control: { kind: "custom" },
    keywords: "atlassian tickets issues connect token",
  },
  {
    id: "jira_account",
    page: "jira",
    title: "Account",
    description: "Who Jira is connected as.",
    path: [],
    control: { kind: "custom" },
    keywords: "atlassian site email",
  },
  {
    id: "jira_in_chats",
    page: "jira",
    title: "Let chats use Jira",
    description:
      "Every chat gets the Jira tools while Jira is connected. Reading is free. Changing Jira asks first, like any other change. Takes effect for chats that start after this.",
    path: ["integrations", "jira_in_chats"],
    control: { kind: "toggle", default: true },
    keywords: "atlassian tickets agents chat",
  },
  {
    id: "composers_state",
    page: "composers",
    title: "Pause and stop",
    description: "Pause every Composer so no tool call goes through, or stop all of them at once.",
    path: [],
    control: { kind: "custom" },
    keywords: "kill switch emergency halt",
  },
  {
    id: "daily_run_cap",
    page: "composers",
    title: "Runs started on their own, per day",
    description:
      "The most runs schedules and triggers may start in 24 hours. Past it, they wait for tomorrow.",
    path: ["composers", "daily_run_cap"],
    control: {
      kind: "select",
      options: opts(["5", "5"], ["10", "10"], ["20", "20"], ["50", "50"], ["100", "100"]),
      default: "20",
      numeric: true,
    },
    keywords: "limit unattended cost schedule trigger",
  },
  {
    id: "notify_waiting",
    page: "composers",
    title: "Tell me when something waits",
    description:
      "A notification when a Composer needs your approval while Circles is in the background.",
    path: ["composers", "notify_waiting"],
    control: { kind: "toggle", default: true },
    keywords: "approval inbox alert",
  },
  {
    id: "menu_bar",
    page: "composers",
    title: "Keep running in the menu bar",
    description:
      "Closing the window hides Circles instead of quitting it, so schedules and triggers go on. Quit from the menu bar item. Takes effect the next time Circles starts.",
    path: ["composers", "menu_bar"],
    control: { kind: "toggle", default: false },
    keywords: "background tray unattended",
  },
  {
    id: "start_at_login",
    page: "composers",
    title: "Open Circles at login",
    description: "So schedules and triggers run after you log in.",
    path: ["composers", "start_at_login"],
    control: { kind: "toggle", default: false },
    keywords: "startup boot autostart login item",
  },
  {
    id: "keep_awake",
    page: "composers",
    title: "Keep the Mac awake during runs",
    description:
      "While a run started by a schedule or trigger is going, the Mac does not go to sleep.",
    path: ["composers", "keep_awake"],
    control: { kind: "toggle", default: false },
    keywords: "sleep caffeinate unattended",
  },
  {
    id: "experimental",
    page: "experimental",
    title: "Experimental features",
    description: "Switches for features that are not finished.",
    path: ["experimental"],
    control: { kind: "custom" },
    keywords: "flags beta",
  },
  {
    id: "data_paths",
    page: "advanced",
    title: "Data folders",
    description: "Where Circles keeps its database, logs and settings.",
    path: [],
    control: { kind: "custom" },
    keywords: "reveal finder folder",
  },
  {
    id: "log_level",
    page: "advanced",
    title: "Log level",
    description: "How much is written to the log. Takes effect the next time Circles starts.",
    path: ["advanced", "log_level"],
    control: {
      kind: "select",
      options: opts(
        ["error", "Error"],
        ["warn", "Warn"],
        ["info", "Info"],
        ["debug", "Debug"],
        ["trace", "Trace"],
      ),
      default: "info",
    },
    keywords: "debug logging verbose",
  },
  {
    id: "checkpoint_days",
    page: "advanced",
    title: "Keep checkpoints for",
    description:
      "How long chat checkpoints stay available for reverting. Older ones are removed in the background.",
    path: ["advanced", "checkpoint_days"],
    control: {
      kind: "select",
      options: opts(
        ["7", "7 days"],
        ["14", "14 days"],
        ["30", "30 days"],
        ["90", "90 days"],
        ["365", "1 year"],
      ),
      default: "30",
      numeric: true,
    },
    keywords: "retention history revert storage disk",
  },
  {
    id: "export_logs",
    page: "advanced",
    title: "Export logs",
    description: "Collect the logs into a folder you can attach to a bug report.",
    path: [],
    control: { kind: "custom" },
    keywords: "diagnostics",
  },
  {
    id: "clear_cache",
    page: "advanced",
    title: "Cached data",
    description:
      "Copies of pull requests and other answers from GitHub, kept so screens open at once. Clearing them is safe: they are fetched again.",
    path: [],
    control: { kind: "custom" },
    keywords: "cache storage disk github pull requests",
  },
  {
    id: "diagnostics",
    page: "advanced",
    title: "Copy diagnostics",
    description:
      "Copy versions, tool status and the recent log to paste into a bug report. Secrets are removed.",
    path: [],
    control: { kind: "custom" },
    keywords: "bug report support",
  },
  {
    id: "setup_again",
    page: "advanced",
    title: "First-run setup",
    description: "Show the welcome screen again: tool check and security notice.",
    path: [],
    control: { kind: "custom" },
    keywords: "onboarding welcome dependencies",
  },
  {
    id: "reset",
    page: "advanced",
    title: "Reset app data",
    description:
      "Delete chats, workspaces records and terminal history. Your settings and repositories stay.",
    path: [],
    control: { kind: "custom" },
    keywords: "wipe delete clear database",
  },
];

/** Rows matching `query` in title, description or keywords, all words required. */
export function searchSettings(query: string): SettingDef[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return [];
  return SETTINGS.filter((d) => {
    const hay = `${d.title} ${d.description} ${d.keywords ?? ""}`.toLowerCase();
    return words.every((w) => hay.includes(w));
  });
}
