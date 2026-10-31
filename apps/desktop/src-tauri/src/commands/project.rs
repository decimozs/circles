use tauri::State;

use crate::error::AppError;
use crate::github::GhRepos;
use crate::ipc::IpcResult;
use crate::pr::docs::{self, Learned};
use crate::pr::draft;
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

/// Has Claude read the repository's template, convention files, recent commits and branch names,
/// and propose how to keep its conventions in the settings. Nothing is saved: the person decides.
#[tauri::command]
pub async fn project_learn_conventions(
    state: State<'_, AppState>,
    project_id: String,
    model: Option<String>,
) -> IpcResult<Learned> {
    let project = state.project_service().await.get(&project_id)?;
    let root = std::path::PathBuf::from(&project.path);
    let git = state.pr_service().await.git;
    let repo_docs = docs::read(&root);
    let commits: Vec<String> = git
        .log(&root, "HEAD", 40)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|c| c.subject)
        .collect();
    let branches = git.recent_branches(&root, 30).await.unwrap_or_default();
    let evidence = docs::Evidence {
        docs: &repo_docs,
        commits: &commits,
        branches: &branches,
    };
    if evidence.is_empty() {
        return Err(AppError::Invalid(
            "There is nothing to learn from yet: no pull request template, AGENTS.md, CLAUDE.md or CONTRIBUTING.md, and no commits or branches.".into(),
        ));
    }
    let env = crate::commands::app::env_with_settings(&state, Some(&project_id)).await;
    let model = model
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| "sonnet".into());
    // Run away from the repository and without the person's own settings: everything it needs is
    // in the prompt, and only the prompt should shape the answer.
    let answer = draft::run_once_with(
        &env,
        &std::env::temp_dir(),
        &docs::learn_prompt(&evidence),
        &model,
        &["--setting-sources", "project,local"],
    )
    .await?;
    docs::parse_learned(&answer).ok_or_else(|| {
        AppError::Invalid(
            "The agent did not answer in the expected form. Try again, or write the settings by hand.".into(),
        )
    })
}

/// Repositories of the signed in GitHub account and its organizations, for the project picker.
#[tauri::command]
pub async fn github_list_repos(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> IpcResult<GhRepos> {
    // Listing takes a second or two and changes rarely, so what was kept is shown at once and
    // asked again in the background when it is ten minutes old.
    let service = state.project_service().await;
    let cache = crate::cache::CacheStore::new(state.db.clone());
    crate::cache::swr::serve(
        &app,
        &cache,
        crate::commands::account::REPOS_KEY,
        10 * 60 * 1000,
        "github-repos",
        move || {
            let service = service.clone();
            async move { service.github_repos().await }
        },
    )
    .await
}
