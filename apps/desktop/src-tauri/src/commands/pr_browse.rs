//! Browsing the pull requests of the repositories you have added.

use tauri::State;

use std::path::PathBuf;

use serde::Deserialize;
use ts_rs::TS;

use crate::error::AppError;
use crate::files::{looks_like_text, TEXT_LIMIT};
use crate::github::browse::{browse_all, BrowseState, PrList, RepoRef, Whose};
use crate::github::browse_detail::PrDetail;
use crate::github::browse_files::PrFiles;
use crate::github::pr::MergeMethod;
use crate::ipc::IpcResult;
use crate::review::changes::FilePair;
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

/// The folder and name of a project that has a repository on disk.
async fn project_folder(state: &AppState, project_id: &str) -> Result<(PathBuf, String), AppError> {
    let project = state.project_service().await.get(project_id)?;
    if !project.path_exists {
        return Err(AppError::NotFound(format!(
            "The folder of {} is not on this Mac any more.",
            project.name
        )));
    }
    Ok((project.path.into(), project.name))
}

/// One pull request in full.
#[tauri::command]
pub async fn pr_browse_detail(
    state: State<'_, AppState>,
    project_id: String,
    number: u32,
) -> IpcResult<PrDetail> {
    let (folder, name) = project_folder(&state, &project_id).await?;
    let gh = state.pr_service().await.gh.clone();
    let me = gh.current_user_in(&folder).await.unwrap_or_default();
    let whose = Whose {
        project_id: &project_id,
        project_name: &name,
        me: &me,
    };
    Ok(gh.browse_pr(&folder, &whose, number).await?)
}

/// The files a pull request changes, and the commits to read them at.
#[tauri::command]
pub async fn pr_browse_files(
    state: State<'_, AppState>,
    project_id: String,
    repo: String,
    number: u32,
) -> IpcResult<PrFiles> {
    let (folder, _) = project_folder(&state, &project_id).await?;
    let gh = state.pr_service().await.gh.clone();
    Ok(gh.browse_pr_files(&folder, &repo, number).await?)
}

/// Which file of which pull request to read, and at which commits.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct PrFileRequest {
    pub project_id: String,
    /// `owner/name`.
    pub repo: String,
    pub base_sha: String,
    pub head_sha: String,
    pub path: String,
    pub previous_path: Option<String>,
    /// `added`, `removed` and the like, as `pr_browse_files` reported it.
    pub status: String,
}

/// Both sides of one file of a pull request. The project's own git is asked first, since it is
/// quicker and costs nothing when it already has the commits; GitHub is asked for what it lacks.
#[tauri::command]
pub async fn pr_browse_file(
    state: State<'_, AppState>,
    request: PrFileRequest,
) -> IpcResult<FilePair> {
    let PrFileRequest {
        project_id,
        repo,
        base_sha,
        head_sha,
        path,
        previous_path,
        status,
    } = request;
    let (folder, _) = project_folder(&state, &project_id).await?;
    let service = state.pr_service().await;
    let (gh, git) = (service.gh.clone(), service.git.clone());
    let read = |sha: String, at: String| {
        let (gh, git, folder, repo) = (gh.clone(), git.clone(), folder.clone(), repo.clone());
        async move {
            if let Ok(Some(text)) = git.show_blob(&folder, &sha, &at).await {
                return Ok::<_, AppError>(Some(text));
            }
            Ok(gh.browse_file_text(&folder, &repo, &sha, &at).await?)
        }
    };
    let original = if status == "added" {
        None
    } else {
        read(base_sha, previous_path.unwrap_or_else(|| path.clone())).await?
    };
    let modified = if status == "removed" {
        None
    } else {
        read(head_sha, path.clone()).await?
    };
    let language = crate::files::language_for(&path).to_string();
    let too_large = [&original, &modified]
        .iter()
        .any(|t| t.as_deref().is_some_and(|t| t.len() as u64 > TEXT_LIMIT));
    let binary = [&original, &modified]
        .iter()
        .any(|t| t.as_deref().is_some_and(|t| !looks_like_text(t.as_bytes())));
    if binary || too_large {
        return Ok(FilePair {
            path,
            original: None,
            modified: None,
            language,
            binary,
            too_large,
        });
    }
    Ok(FilePair {
        path,
        original,
        modified,
        language,
        binary: false,
        too_large: false,
    })
}

fn merge_method(method: &str) -> Result<MergeMethod, AppError> {
    match method {
        "merge" | "squash" | "rebase" => Ok(MergeMethod::parse(Some(method))),
        other => Err(AppError::Invalid(format!("{other} is not a way to merge"))),
    }
}

/// Merges a pull request. `method` is `merge`, `squash` or `rebase`; anything else is refused
/// rather than guessed, because a merge cannot be taken back.
#[tauri::command]
pub async fn pr_browse_merge(
    state: State<'_, AppState>,
    project_id: String,
    number: u32,
    method: String,
) -> IpcResult<()> {
    let method = merge_method(&method)?;
    let (folder, _) = project_folder(&state, &project_id).await?;
    let gh = state.pr_service().await.gh.clone();
    gh.pr_merge(&folder, number, method).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_three_ways_to_merge_are_accepted() {
        for ok in ["merge", "squash", "rebase"] {
            assert!(merge_method(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "Merge",
            "--admin",
            "squash --delete-branch",
            "fast-forward",
        ] {
            assert_eq!(merge_method(bad).unwrap_err().code(), "invalid", "{bad}");
        }
    }
}
