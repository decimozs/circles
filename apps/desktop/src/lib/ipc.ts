import type { ActionResult } from "@circles/bindings/ActionResult";
import type { AppInfo } from "@circles/bindings/AppInfo";
import type { Approval } from "@circles/bindings/Approval";
import type { AttachInfo } from "@circles/bindings/AttachInfo";
import type { Attachment } from "@circles/bindings/Attachment";
import type { AttachSnapshot } from "@circles/bindings/AttachSnapshot";
import type { AuditEntry } from "@circles/bindings/AuditEntry";
import type { BranchInfo } from "@circles/bindings/BranchInfo";
import type { BranchState } from "@circles/bindings/BranchState";
import type { ChangedFile } from "@circles/bindings/ChangedFile";
import type { Check } from "@circles/bindings/Check";
import type { Checkpoint } from "@circles/bindings/Checkpoint";
import type { ClaudeCheck } from "@circles/bindings/ClaudeCheck";
import type { Comment } from "@circles/bindings/Comment";
import type { CommitInfo } from "@circles/bindings/CommitInfo";
import type { Composer } from "@circles/bindings/Composer";
import type { ComposerInput } from "@circles/bindings/ComposerInput";
import type { ComposersStatus } from "@circles/bindings/ComposersStatus";
import type { ComposerTool } from "@circles/bindings/ComposerTool";
import type { CreateOptions } from "@circles/bindings/CreateOptions";
import type { Dependency } from "@circles/bindings/Dependency";
import type { Deployment } from "@circles/bindings/Deployment";
import type { Editor } from "@circles/bindings/Editor";
import type { ErrorPayload } from "@circles/bindings/ErrorPayload";
import type { EventRow } from "@circles/bindings/EventRow";
import type { FileChange } from "@circles/bindings/FileChange";
import type { FileContent } from "@circles/bindings/FileContent";
import type { FileHit } from "@circles/bindings/FileHit";
import type { FilePair } from "@circles/bindings/FilePair";
import type { FleetItem } from "@circles/bindings/FleetItem";
import type { FsEntry } from "@circles/bindings/FsEntry";
import type { GhRepo } from "@circles/bindings/GhRepo";
import type { GitGraph } from "@circles/bindings/GitGraph";
import type { GitIdentity } from "@circles/bindings/GitIdentity";
import type { JiraStatus } from "@circles/bindings/JiraStatus";
import type { Message } from "@circles/bindings/Message";
import type { NewComment } from "@circles/bindings/NewComment";
import type { PendingPrompt } from "@circles/bindings/PendingPrompt";
import type { PermissionDecision } from "@circles/bindings/PermissionDecision";
import type { PrDraft } from "@circles/bindings/PrDraft";
import type { PrDraftSeed } from "@circles/bindings/PrDraftSeed";
import type { Project } from "@circles/bindings/Project";
import type { PrStatus } from "@circles/bindings/PrStatus";
import type { PullRequest } from "@circles/bindings/PullRequest";
import type { RateLimit } from "@circles/bindings/RateLimit";
import type { Readiness } from "@circles/bindings/Readiness";
import type { ReconcileReport } from "@circles/bindings/ReconcileReport";
import type { RevertOutcome } from "@circles/bindings/RevertOutcome";
import type { RevertPreview } from "@circles/bindings/RevertPreview";
import type { ReviewThread } from "@circles/bindings/ReviewThread";
import type { RunView } from "@circles/bindings/RunView";
import type { ScriptsInfo } from "@circles/bindings/ScriptsInfo";
import type { Section } from "@circles/bindings/Section";
import type { Session } from "@circles/bindings/Session";
import type { SessionOptions } from "@circles/bindings/SessionOptions";
import type { SettingsPayload } from "@circles/bindings/SettingsPayload";
import type { SettingsView } from "@circles/bindings/SettingsView";
import type { SetupItem } from "@circles/bindings/SetupItem";
import type { SharedComposer } from "@circles/bindings/SharedComposer";
import type { SlashCommand } from "@circles/bindings/SlashCommand";
import type { SmartStatus } from "@circles/bindings/SmartStatus";
import type { SmartTest } from "@circles/bindings/SmartTest";
import type { StoredCard } from "@circles/bindings/StoredCard";
import type { StreamItem } from "@circles/bindings/StreamItem";
import type { Terminal } from "@circles/bindings/Terminal";
import type { Trigger } from "@circles/bindings/Trigger";
import type { TriggerInput } from "@circles/bindings/TriggerInput";
import type { UndoableRevert } from "@circles/bindings/UndoableRevert";
import type { Workspace } from "@circles/bindings/Workspace";
import type { WorkspaceEvent } from "@circles/bindings/WorkspaceEvent";
import type { WorkspaceLink } from "@circles/bindings/WorkspaceLink";
import { type Channel, invoke } from "@tauri-apps/api/core";

/** Error thrown by every backend call. Mirrors `ErrorPayload` from the Rust side. */
export class IpcError extends Error {
  readonly code: string;
  readonly details: unknown;

  constructor(payload: ErrorPayload) {
    super(payload.message);
    this.name = "IpcError";
    this.code = payload.code;
    this.details = payload.details;
  }
}

function isErrorPayload(value: unknown): value is ErrorPayload {
  return (
    typeof value === "object" &&
    value !== null &&
    "code" in value &&
    "message" in value &&
    typeof (value as ErrorPayload).code === "string"
  );
}

/** Typed `invoke` that always rejects with an `IpcError`. */
export async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    if (isErrorPayload(error)) throw new IpcError(error);
    const message = error instanceof Error ? error.message : String(error);
    throw new IpcError({ code: "internal", message, details: null });
  }
}

/** Machine readable cause for `git` and `gh` errors, for example `dirty_tree` or `not_authenticated`. */
export function errorKind(error: unknown): string | null {
  if (!(error instanceof IpcError)) return null;
  const details = error.details as { kind?: unknown } | null;
  return typeof details?.kind === "string" ? details.kind : null;
}

/** The shell command that fixes a `gh` problem, when the backend knows one. */
export function errorFix(error: unknown): string | null {
  if (!(error instanceof IpcError)) return null;
  const details = error.details as { fix?: unknown } | null;
  return typeof details?.fix === "string" ? details.fix : null;
}

export const ipc = {
  getAppInfo: () => call<AppInfo>("get_app_info"),
  settingsGet: (projectId?: string) =>
    call<SettingsPayload>("settings_get", { projectId: projectId ?? null }),

  projectList: () => call<Project[]>("project_list"),
  projectAddLocal: (path: string) => call<Project>("project_add_local", { path }),
  projectAddGithub: (reference: string) => call<Project>("project_add_github", { reference }),
  projectQuickStart: (name: string) => call<Project>("project_quick_start", { name }),
  projectRemove: (id: string, workspaces?: "keep" | "delete") =>
    call<null>("project_remove", { id, workspaces: workspaces ?? null }),
  projectSetBaseBranch: (id: string, branch: string) =>
    call<Project>("project_set_base_branch", { id, branch }),
  githubListRepos: () => call<GhRepo[]>("github_list_repos"),

  workspaceList: (projectId?: string) =>
    call<Workspace[]>("workspace_list", { projectId: projectId ?? null, includeArchived: true }),
  workspaceGet: (id: string) => call<Workspace>("workspace_get", { id }),
  workspaceBranchStates: () => call<BranchState[]>("workspace_branch_states"),
  workspaceEvents: (id: string) => call<WorkspaceEvent[]>("workspace_events", { id }),
  workspaceCreate: (projectId: string, options: CreateOptions) =>
    call<Workspace>("workspace_create", { projectId, options }),
  projectBranches: (projectId: string) => call<BranchInfo[]>("project_branches", { projectId }),
  githubListPrs: (projectId: string) => call<PullRequest[]>("github_list_prs", { projectId }),
  workspaceArchive: (id: string, commitDirty: boolean) =>
    call<Workspace>("workspace_archive", { id, commitDirty }),
  workspaceRestore: (id: string) => call<Workspace>("workspace_restore", { id }),
  workspaceRenameBranch: (id: string, name: string) =>
    call<Workspace>("workspace_rename_branch", { id, name }),
  workspacePin: (id: string, pinned: boolean) => call<Workspace>("workspace_pin", { id, pinned }),
  workspaceSetTitle: (id: string, title: string | null) =>
    call<Workspace>("workspace_set_title", { id, title }),
  workspaceDelete: (id: string, deleteBranch: boolean) =>
    call<null>("workspace_delete", { id, deleteBranch }),
  workspaceTakePendingPrompt: (id: string) =>
    call<PendingPrompt | null>("workspace_take_pending_prompt", { id }),
  workspaceOpen: (id: string) => call<null>("workspace_open", { id }),
  workspaceReconcile: () => call<ReconcileReport>("workspace_reconcile"),
  workspaceSetSection: (workspaceId: string, sectionId: string | null) =>
    call<Workspace>("workspace_set_section", { workspaceId, sectionId }),

  terminalList: (workspaceId: string) => call<Terminal[]>("terminal_list", { workspaceId }),
  terminalCreate: (workspaceId: string, title?: string) =>
    call<Terminal>("terminal_create", { workspaceId, title: title ?? null }),
  terminalRename: (id: string, title: string) => call<Terminal>("terminal_rename", { id, title }),
  terminalDelete: (id: string) => call<null>("terminal_delete", { id }),
  terminalAttach: (id: string, cols: number, rows: number, onEvent: Channel<unknown>) =>
    call<AttachInfo>("terminal_attach", { id, cols, rows, onEvent }),
  terminalDetach: (id: string) => call<null>("terminal_detach", { id }),
  terminalWrite: (id: string, data: string) => call<null>("terminal_write", { id, data }),
  terminalResize: (id: string, cols: number, rows: number) =>
    call<null>("terminal_resize", { id, cols, rows }),
  terminalRestart: (id: string) => call<null>("terminal_restart", { id }),
  terminalStop: (id: string) => call<Terminal>("terminal_stop", { id }),
  scriptsGet: (workspaceId: string) => call<ScriptsInfo>("scripts_get", { workspaceId }),
  scriptRun: (workspaceId: string, name?: string) =>
    call<Terminal>("script_run", { workspaceId, name: name ?? null }),
  scriptStop: (workspaceId: string) => call<Terminal>("script_stop", { workspaceId }),
  scriptRunSetup: (workspaceId: string) => call<Terminal>("script_run_setup", { workspaceId }),
  terminalsRunning: () => call<number>("terminals_running"),
  appQuit: () => call<null>("app_quit"),
  projectOpenSettings: (projectId: string) => call<string>("project_open_settings", { projectId }),

  sessionList: (workspaceId: string, includeClosed = false) =>
    call<Session[]>("session_list", { workspaceId, includeClosed }),
  sessionReopen: (id: string) => call<Session>("session_reopen", { id }),
  sessionReorder: (workspaceId: string, ids: string[]) =>
    call<null>("session_reorder", { workspaceId, ids }),
  searchFiles: (workspaceId: string, query: string, limit = 20) =>
    call<FileHit[]>("workspace_search_files", { workspaceId, query, limit }),
  slashCommands: (workspaceId: string) => call<SlashCommand[]>("slash_commands", { workspaceId }),
  attachmentSave: (workspaceId: string, name: string, dataBase64: string) =>
    call<Attachment>("attachment_save", { workspaceId, name, dataBase64 }),
  sessionCreate: (workspaceId: string, options: Partial<SessionOptions>) =>
    call<Session>("session_create", {
      workspaceId,
      options: {
        title: null,
        model: null,
        effort: null,
        permission_mode: null,
        fork_from_session: null,
        ...options,
      },
    }),
  sessionGet: (id: string) => call<Session>("session_get", { id }),
  sessionRename: (id: string, title: string) => call<Session>("session_rename", { id, title }),
  sessionMessages: (id: string, afterSeq?: number) =>
    call<Message[]>("session_messages", { id, afterSeq: afterSeq ?? null }),
  sessionEvents: (id: string, afterSeq?: number, limit?: number) =>
    call<EventRow[]>("session_events", { id, afterSeq: afterSeq ?? null, limit: limit ?? null }),
  sessionAttach: (id: string, onEvent: Channel<StreamItem>) =>
    call<AttachSnapshot>("session_attach", { id, onEvent }),
  sessionDetach: (id: string) => call<null>("session_detach", { id }),
  sessionSend: (id: string, text: string, attachments?: Attachment[]) =>
    call<Message>("session_send", { id, text, attachments: attachments ?? null }),
  sessionResume: (id: string) => call<Session>("session_resume", { id }),
  sessionInterrupt: (id: string) => call<null>("session_interrupt", { id }),
  sessionStop: (id: string) => call<null>("session_stop", { id }),
  sessionClose: (id: string) => call<null>("session_close", { id }),
  sessionUpdate: (
    id: string,
    options: { model?: string; effort?: string; permissionMode?: string },
  ) =>
    call<Session>("session_update", {
      id,
      model: options.model ?? null,
      effort: options.effort ?? null,
      permissionMode: options.permissionMode ?? null,
    }),
  permissionReply: (sessionId: string, requestId: string, decision: PermissionDecision) =>
    call<null>("permission_reply", { sessionId, requestId, decision }),
  claudeRateLimit: () => call<RateLimit | null>("claude_rate_limit"),

  fsList: (workspaceId: string, path = "", showIgnored = false) =>
    call<FsEntry[]>("fs_list", { workspaceId, path, showIgnored }),
  fsRead: (workspaceId: string, path: string) =>
    call<FileContent>("fs_read", { workspaceId, path }),
  fsWrite: (workspaceId: string, path: string, text: string) =>
    call<null>("fs_write", { workspaceId, path, text }),
  fsReveal: (workspaceId: string, path?: string) =>
    call<null>("fs_reveal", { workspaceId, path: path ?? null }),
  changesList: (workspaceId: string, scope: "all" | "uncommitted") =>
    call<ChangedFile[]>("changes_list", { workspaceId, scope }),
  diffFiles: (
    workspaceId: string,
    scope: "all" | "uncommitted",
    path: string,
    oldPath?: string | null,
  ) => call<FilePair>("diff_files", { workspaceId, scope, path, oldPath: oldPath ?? null }),
  filesWatch: (workspaceId: string) => call<null>("files_watch", { workspaceId }),
  filesUnwatch: (workspaceId: string) => call<null>("files_unwatch", { workspaceId }),
  editorList: () => call<Editor[]>("editor_list"),
  editorOpen: (workspaceId: string, editor: string, path?: string) =>
    call<null>("editor_open", { workspaceId, editor, path: path ?? null }),
  commentList: (workspaceId: string) => call<Comment[]>("comment_list", { workspaceId }),
  commentCreate: (workspaceId: string, comment: NewComment) =>
    call<Comment>("comment_create", { workspaceId, comment }),
  commentSetStatus: (id: string, resolved: boolean) =>
    call<Comment>("comment_set_status", { id, resolved }),
  commentDelete: (id: string) => call<null>("comment_delete", { id }),
  commentSend: (workspaceId: string, sessionId: string, ids: string[]) =>
    call<null>("comment_send", { workspaceId, sessionId, ids }),

  checkpointList: (sessionId: string) => call<Checkpoint[]>("checkpoint_list", { sessionId }),
  checkpointPreview: (checkpointId: string) =>
    call<RevertPreview>("checkpoint_preview", { checkpointId }),
  checkpointRevert: (checkpointId: string) =>
    call<RevertOutcome>("checkpoint_revert", { checkpointId }),
  checkpointLastRevert: (sessionId: string) =>
    call<UndoableRevert | null>("checkpoint_last_revert", { sessionId }),
  checkpointUndoRevert: (revertId: string, sessionId: string) =>
    call<FileChange[]>("checkpoint_undo_revert", { revertId, sessionId }),
  sessionBranch: (sessionId: string, turn: number) =>
    call<Session>("session_branch", { sessionId, turn }),
  settingsView: (projectId?: string) =>
    call<SettingsView>("settings_view", { projectId: projectId ?? null }),
  settingsSet: (
    scope: "user" | "project" | "local",
    projectId: string | undefined,
    path: string[],
    value: unknown,
  ) =>
    call<SettingsView>("settings_set", {
      scope,
      projectId: projectId ?? null,
      path,
      value: value === undefined ? null : value,
    }),
  settingsWatch: (projectId?: string) =>
    call<null>("settings_watch", { projectId: projectId ?? null }),
  settingsUnwatch: () => call<null>("settings_unwatch"),
  settingsOpenFile: (scope: "user" | "project" | "local", projectId?: string) =>
    call<string>("settings_open_file", { scope, projectId: projectId ?? null }),
  gitIdentity: () => call<GitIdentity>("git_identity"),
  claudeCheck: () => call<ClaudeCheck>("claude_check"),
  soundPlay: () => call<null>("sound_play"),
  notifyTest: () => call<null>("notify_test"),
  dataReveal: () => call<null>("data_reveal"),
  logsExport: () => call<string>("logs_export"),
  appResetData: (confirm: string) => call<null>("app_reset_data", { confirm }),
  gitGraph: (workspaceId: string) => call<GitGraph>("git_graph", { workspaceId }),
  toolCatalog: () => call<ComposerTool[]>("tool_catalog"),
  composerList: () => call<Composer[]>("composer_list"),
  composerGet: (id: string) => call<Composer>("composer_get", { id }),
  composerCreate: (input: ComposerInput) => call<Composer>("composer_create", { input }),
  composerUpdate: (id: string, input: ComposerInput) =>
    call<Composer>("composer_update", { id, input }),
  composerPin: (id: string, pinned: boolean) => call<Composer>("composer_pin", { id, pinned }),
  sessionAutoTitle: (sessionId: string, title: string, text: string) =>
    call<null>("session_auto_title", { sessionId, title, text }),
  genuiAnswer: (uid: string, values: Record<string, unknown>) =>
    call<null>("genui_answer", { uid, values }),
  genuiDismiss: (uid: string) => call<null>("genui_dismiss", { uid }),
  genuiCards: (sessionId: string) => call<StoredCard[]>("genui_cards", { sessionId }),
  genuiPending: () => call<StoredCard[]>("genui_pending"),
  setupStatus: () => call<SetupItem[]>("setup_status"),
  setupAction: (id: string) => call<ActionResult>("setup_action", { id }),
  sessionSideQuestion: (sessionId: string, question: string) =>
    call<null>("session_side_question", { sessionId, question }),
  sessionSideStop: (sessionId: string) => call<null>("session_side_stop", { sessionId }),
  smartSuggest: (asked: string, answer: string) =>
    call<string | null>("smart_suggest", { asked, answer }),
  smartFirstPrompt: (name: string, purpose: string) =>
    call<string | null>("smart_first_prompt", { name, purpose }),
  smartStatus: () => call<SmartStatus>("smart_status"),
  smartTest: () => call<SmartTest>("smart_test"),
  jiraStatus: () => call<JiraStatus>("jira_status"),
  jiraConnect: (site: string, email: string, token: string) =>
    call<JiraStatus>("jira_connect", { site, email, token }),
  jiraSignIn: () => call<JiraStatus>("jira_sign_in"),
  jiraCancelSignIn: () => call<null>("jira_cancel_sign_in"),
  jiraChooseSite: (cloudId: string) => call<JiraStatus>("jira_choose_site", { cloudId }),
  jiraDisconnect: () => call<JiraStatus>("jira_disconnect"),
  composerSkills: () => call<SlashCommand[]>("composer_skills"),
  composerSetMode: (id: string, mode: string) => call<Composer>("composer_set_mode", { id, mode }),
  composerSetNotes: (id: string, notes: string) =>
    call<Composer>("composer_set_notes", { id, notes }),
  composerDelete: (id: string) => call<null>("composer_delete", { id }),
  composerRuns: (composerId: string, includeClosed = false) =>
    call<RunView[]>("composer_runs", { composerId, includeClosed }),
  composerRunStart: (composerId: string, message?: string) =>
    call<Session>("composer_run_start", { composerId, message: message ?? null }),
  composerApprovals: (filter: { composerId?: string; sessionId?: string; onlyPending?: boolean }) =>
    call<Approval[]>("composer_approvals", {
      composerId: filter.composerId ?? null,
      sessionId: filter.sessionId ?? null,
      onlyPending: filter.onlyPending ?? false,
    }),
  approvalAnswer: (id: string, allow: boolean, forRun = false) =>
    call<null>("approval_answer", { id, allow, forRun }),
  composerFleet: (composerId: string, sessionId?: string) =>
    call<FleetItem[]>("composer_fleet", { composerId, sessionId: sessionId ?? null }),
  composerStopChild: (chatId: string) => call<null>("composer_stop_child", { chatId }),
  auditList: (filter: {
    composerId?: string;
    sessionId?: string;
    search?: string;
    limit?: number;
  }) =>
    call<AuditEntry[]>("audit_list", {
      composerId: filter.composerId ?? null,
      sessionId: filter.sessionId ?? null,
      search: filter.search ?? null,
      limit: filter.limit ?? null,
    }),
  auditExport: (composerId?: string) =>
    call<string>("audit_export", { composerId: composerId ?? null }),
  composersStatus: () => call<ComposersStatus>("composers_status"),
  composersPause: (paused: boolean) => call<ComposersStatus>("composers_pause", { paused }),
  composersStopAll: () => call<ComposersStatus>("composers_stop_all"),
  composerWorkspaceLinks: () => call<WorkspaceLink[]>("composer_workspace_links"),
  composerSharedList: () => call<SharedComposer[]>("composer_shared_list"),
  composerExport: (id: string, projectId: string) =>
    call<string>("composer_export", { id, projectId }),
  composerSharedInput: (shared: SharedComposer) =>
    call<ComposerInput>("composer_shared_input", { shared }),
  triggerList: (composerId?: string) =>
    call<Trigger[]>("trigger_list", { composerId: composerId ?? null }),
  triggerCreate: (composerId: string, input: TriggerInput) =>
    call<Trigger>("trigger_create", { composerId, input }),
  triggerUpdate: (id: string, input: TriggerInput) =>
    call<Trigger>("trigger_update", { id, input }),
  triggerDelete: (id: string) => call<null>("trigger_delete", { id }),
  dependencyCheck: () => call<Dependency[]>("dependency_check"),
  diagnosticsReport: () => call<string>("diagnostics_report"),
  prReadiness: (workspaceId: string) => call<Readiness>("pr_readiness", { workspaceId }),
  prStatus: (workspaceId: string) => call<PrStatus | null>("pr_status", { workspaceId }),
  prChecks: (workspaceId: string) => call<Check[]>("pr_checks", { workspaceId }),
  prThreads: (workspaceId: string) => call<ReviewThread[]>("pr_threads", { workspaceId }),
  prThreadResolve: (workspaceId: string, threadId: string, resolved: boolean) =>
    call<null>("pr_thread_resolve", { workspaceId, threadId, resolved }),
  prDeployments: (workspaceId: string) => call<Deployment[]>("pr_deployments", { workspaceId }),
  prDraftSeed: (workspaceId: string) => call<PrDraftSeed>("pr_draft_seed", { workspaceId }),
  prDraft: (workspaceId: string, model?: string) =>
    call<PrDraft>("pr_draft", { workspaceId, model: model ?? null }),
  commitWithClaude: (workspaceId: string, model?: string) =>
    call<CommitInfo[]>("commit_with_claude", { workspaceId, model: model ?? null }),
  commitEverything: (workspaceId: string, message: string) =>
    call<CommitInfo[]>("commit_everything", { workspaceId, message }),
  prCreate: (workspaceId: string, title: string, body: string, draft = false) =>
    call<PrStatus>("pr_create", { workspaceId, title, body, draft }),
  prMerge: (workspaceId: string, overrideBlockers = false) =>
    call<PrStatus>("pr_merge", { workspaceId, overrideBlockers }),
  agentFixChecks: (workspaceId: string, sessionId: string) =>
    call<null>("agent_fix_checks", { workspaceId, sessionId }),
  agentRespondReview: (workspaceId: string, sessionId: string) =>
    call<null>("agent_respond_review", { workspaceId, sessionId }),
  agentResolveConflicts: (workspaceId: string, sessionId: string) =>
    call<null>("agent_resolve_conflicts", { workspaceId, sessionId }),

  sectionList: () => call<Section[]>("section_list"),
  sectionCreate: (name: string) => call<Section>("section_create", { name }),
  sectionRename: (id: string, name: string) => call<Section>("section_rename", { id, name }),
  sectionDelete: (id: string) => call<null>("section_delete", { id }),
  sectionReorder: (ids: string[]) => call<Section[]>("section_reorder", { ids }),
};
