use tauri::State;

use crate::github::GhRepos;
use crate::ipc::IpcResult;
use crate::project::Project;
use crate::state::AppState;

#[tauri::command]
pub async fn project_list(state: State<'_, AppState>) -> IpcResult<Vec<Project>> {
    state.project_service().await.list()
}

/// Adds a repository that is already on disk. `path` may be any folder inside it.
#[tauri::command]
pub async fn project_add_local(state: State<'_, AppState>, path: String) -> IpcResult<Project> {
    state.project_service().await.add_local(&path).await
}

/// Clones `owner/name` (or a GitHub URL) into `~/circles/repos/` and adds it.
#[tauri::command]
pub async fn project_add_github(
    state: State<'_, AppState>,
    reference: String,
) -> IpcResult<Project> {
    state.project_service().await.add_github(&reference).await
}

/// Creates a new repository with an initial commit and adds it.
#[tauri::command]
pub async fn project_quick_start(state: State<'_, AppState>, name: String) -> IpcResult<Project> {
    state.project_service().await.quick_start(&name).await
}

/// Removes the project from Circles. The repository on disk is left untouched.
///
/// `workspaces` says what happens to its workspaces. Not given, the project must have none.
/// `keep` lets them go with their folders left on disk, still working copies of the repository.
/// `delete` removes their folders too, with whatever was not committed in them. Branches are kept
/// either way, because they are in the repository.
#[tauri::command]
pub async fn project_remove(
    state: State<'_, AppState>,
    id: String,
    workspaces: Option<String>,
) -> IpcResult<()> {
    let mode = match workspaces.as_deref() {
        None => return state.project_service().await.remove(&id),
        Some(m @ ("keep" | "delete")) => m.to_string(),
        Some(_) => {
            return Err(crate::error::AppError::Invalid(
                "workspaces must be \"keep\" or \"delete\".".into(),
            ))
        }
    };
    let service = state.workspace_service().await;
    // Stop every agent and terminal first, so nothing is writing into a folder as it goes.
    let all = service.list(Some(&id), true)?;
    for ws in &all {
        state.sessions.stop_workspace(&ws.id).await;
        crate::commands::workspace::forget_checkpoints(&state, &ws.id).await;
    }
    for ws in &all {
        if mode == "delete" {
            service.purge(&ws.id).await?;
        } else {
            service.release(&ws.id).await?;
        }
    }
    state.project_service().await.remove_with_workspaces(&id)
}

#[tauri::command]
pub async fn project_set_base_branch(
    state: State<'_, AppState>,
    id: String,
    branch: String,
) -> IpcResult<Project> {
    state
        .project_service()
        .await
        .set_base_branch(&id, &branch)
        .await
}

/// Repositories of the signed in GitHub account and its organizations, for the project picker.
#[tauri::command]
pub async fn github_list_repos(state: State<'_, AppState>) -> IpcResult<GhRepos> {
    state.project_service().await.github_repos().await
}
