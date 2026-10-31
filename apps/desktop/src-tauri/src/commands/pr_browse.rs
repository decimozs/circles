//! Browsing the pull requests of the repositories you have added.

use tauri::State;

use crate::github::browse::{browse_all, BrowseState, PrList, RepoRef};
use crate::ipc::IpcResult;
use crate::state::AppState;

/// Most pull requests asked for from one repository.
const PER_REPO: u32 = 50;

/// The pull requests of one project, or of every project with a GitHub remote when `project_id`
/// is not given.
#[tauri::command]
pub async fn pr_browse_list(
    state: State<'_, AppState>,
    project_id: Option<String>,
    which: BrowseState,
) -> IpcResult<PrList> {
    let projects = state.project_service().await.list()?;
    let repos: Vec<RepoRef> = projects
        .into_iter()
        .filter(|p| project_id.as_deref().is_none_or(|id| id == p.id))
        .filter(|p| {
            p.path_exists
                && p.remote_url
                    .as_deref()
                    .is_some_and(|u| u.contains("github.com"))
        })
        .map(|p| RepoRef {
            project_id: p.id,
            project_name: p.name,
            path: p.path.into(),
        })
        .collect();
    let gh = state.pr_service().await.gh.clone();
    Ok(browse_all(&gh, repos, which, PER_REPO).await)
}
