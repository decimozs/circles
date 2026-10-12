use tauri::State;

use crate::ipc::IpcResult;
use crate::state::AppState;
use crate::workspace::{
    BranchInfo, BranchState, CreateOptions, PullRequest, ReconcileReport, Section, Workspace,
    WorkspaceEvent,
};

#[tauri::command]
pub async fn workspace_list(
    state: State<'_, AppState>,
    project_id: Option<String>,
    include_archived: bool,
) -> IpcResult<Vec<Workspace>> {
    state
        .workspace_service()
        .await
        .list(project_id.as_deref(), include_archived)
}

/// Where every workspace's branch stands, for the sidebar.
#[tauri::command]
pub async fn workspace_branch_states(state: State<'_, AppState>) -> IpcResult<Vec<BranchState>> {
    state.workspace_service().await.branch_states().await
}

#[tauri::command]
pub async fn workspace_get(state: State<'_, AppState>, id: String) -> IpcResult<Workspace> {
    state.workspace_service().await.get(&id)
}

/// The lines shown at the top of the workspace chat ("branched from origin/main", warnings).
#[tauri::command]
pub async fn workspace_events(
    state: State<'_, AppState>,
    id: String,
) -> IpcResult<Vec<WorkspaceEvent>> {
    state.workspace_service().await.events(&id)
}

/// Creates a workspace from a new branch, an existing branch or a pull request. The options also
/// carry the first prompt, the model and effort, and files to attach.
#[tauri::command]
pub async fn workspace_create(
    state: State<'_, AppState>,
    project_id: String,
    options: CreateOptions,
) -> IpcResult<Workspace> {
    state
        .workspace_service()
        .await
        .create_with(&project_id, options)
        .await
}

/// Branches the Create dialog offers, most recently committed first.
#[tauri::command]
pub async fn project_branches(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<Vec<BranchInfo>> {
    state
        .workspace_service()
        .await
        .list_branches(&project_id)
        .await
}

/// Open pull requests of the project's GitHub repository.
#[tauri::command]
pub async fn github_list_prs(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<Vec<PullRequest>> {
    state
        .workspace_service()
        .await
        .list_pull_requests(&project_id)
        .await
}

/// Archives a workspace. Without `commit_dirty`, uncommitted changes refuse the archive with a
/// `dirty_tree` error so the UI can offer to commit them.
#[tauri::command]
pub async fn workspace_archive(
    state: State<'_, AppState>,
    id: String,
    commit_dirty: bool,
) -> IpcResult<Workspace> {
    let archived = state
        .workspace_service()
        .await
        .archive(&id, commit_dirty)
        .await?;
    // The chats' processes would otherwise keep running in a folder that no longer exists.
    state.sessions.stop_workspace(&id).await;
    forget_checkpoints(&state, &id).await;
    Ok(archived)
}

#[tauri::command]
pub async fn workspace_restore(state: State<'_, AppState>, id: String) -> IpcResult<Workspace> {
    state.workspace_service().await.restore(&id).await
}

#[tauri::command]
pub async fn workspace_rename_branch(
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> IpcResult<Workspace> {
    state
        .workspace_service()
        .await
        .rename_branch(&id, &name)
        .await
}

#[tauri::command]
pub async fn workspace_pin(
    state: State<'_, AppState>,
    id: String,
    pinned: bool,
) -> IpcResult<Workspace> {
    state.workspace_service().await.set_pinned(&id, pinned)
}

#[tauri::command]
pub async fn workspace_set_title(
    state: State<'_, AppState>,
    id: String,
    title: Option<String>,
) -> IpcResult<Workspace> {
    state
        .workspace_service()
        .await
        .set_title(&id, title.as_deref())
}

/// Deletes an archived or errored workspace record. The branch is kept unless `delete_branch`.
#[tauri::command]
pub async fn workspace_delete(
    state: State<'_, AppState>,
    id: String,
    delete_branch: bool,
) -> IpcResult<()> {
    state.sessions.stop_workspace(&id).await;
    forget_checkpoints(&state, &id).await;
    state
        .workspace_service()
        .await
        .delete(&id, delete_branch)
        .await
}

/// Removes the checkpoints of a workspace that is going away. The refs live in the project's
/// repository. A failure here is logged and never stops the archive.
pub(crate) async fn forget_checkpoints(state: &AppState, workspace_id: &str) {
    let service = state.workspace_service().await;
    let repo = service
        .get(workspace_id)
        .and_then(|ws| service.projects.get(&ws.project_id))
        .map(|project| std::path::PathBuf::from(project.path));
    match repo {
        Ok(repo) => {
            if let Err(e) = state
                .checkpoints()
                .await
                .prune_workspace(workspace_id, &repo)
                .await
            {
                tracing::warn!(error = %e, "cannot remove the checkpoints of a workspace");
            }
        }
        Err(e) => tracing::warn!(error = %e, "cannot find the repository to clean up checkpoints"),
    }
}

/// Records that the user opened a workspace.
#[tauri::command]
pub async fn workspace_open(state: State<'_, AppState>, id: String) -> IpcResult<()> {
    state.workspace_service().await.touch_opened(&id)
}

#[tauri::command]
pub async fn workspace_reconcile(state: State<'_, AppState>) -> IpcResult<ReconcileReport> {
    state.workspace_service().await.reconcile().await
}

#[tauri::command]
pub async fn section_list(state: State<'_, AppState>) -> IpcResult<Vec<Section>> {
    state.workspace_service().await.list_sections()
}

#[tauri::command]
pub async fn section_create(state: State<'_, AppState>, name: String) -> IpcResult<Section> {
    state.workspace_service().await.create_section(&name)
}

#[tauri::command]
pub async fn section_rename(
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> IpcResult<Section> {
    state.workspace_service().await.rename_section(&id, &name)
}

#[tauri::command]
pub async fn section_delete(state: State<'_, AppState>, id: String) -> IpcResult<()> {
    state.workspace_service().await.delete_section(&id)
}

#[tauri::command]
pub async fn section_reorder(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> IpcResult<Vec<Section>> {
    state.workspace_service().await.reorder_sections(&ids)
}

#[tauri::command]
pub async fn workspace_set_section(
    state: State<'_, AppState>,
    workspace_id: String,
    section_id: Option<String>,
) -> IpcResult<Workspace> {
    state
        .workspace_service()
        .await
        .set_section(&workspace_id, section_id.as_deref())
}

/// The message typed in the Create dialog. Returned once, then cleared, so the chat sends it a
/// single time.
#[tauri::command]
pub async fn workspace_take_pending_prompt(
    state: State<'_, AppState>,
    id: String,
) -> IpcResult<Option<crate::workspace::PendingPrompt>> {
    state.workspace_service().await.take_pending_prompt(&id)
}
