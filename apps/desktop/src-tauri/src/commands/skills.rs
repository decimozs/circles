//! The Skills page: what skills exist on this Mac and in the projects. Read only.

use tauri::State;

use crate::error::AppError;
use crate::ipc::IpcResult;
use crate::skills::{self, SkillDetail, SkillList, Sources};
use crate::state::AppState;

async fn sources(state: &AppState) -> Result<Sources, AppError> {
    let env = crate::commands::app::env_with_settings(state, None).await;
    let projects = state.project_service().await.list()?;
    Ok(Sources {
        home: dirs::home_dir(),
        configured: env.get("CLAUDE_CONFIG_DIR").map(String::from),
        projects: projects
            .into_iter()
            .filter(|p| p.path_exists)
            .map(|p| (p.id, p.name, std::path::PathBuf::from(p.path)))
            .collect(),
    })
}

/// Every skill the page knows, and the places that were looked in.
#[tauri::command]
pub async fn skills_list(state: State<'_, AppState>) -> IpcResult<SkillList> {
    let src = sources(&state).await?;
    tokio::task::spawn_blocking(move || skills::list(&src))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))
}

/// One skill's text, front matter and files. `id` is a skill's real folder from `skills_list`.
#[tauri::command]
pub async fn skills_detail(state: State<'_, AppState>, id: String) -> IpcResult<SkillDetail> {
    let src = sources(&state).await?;
    tokio::task::spawn_blocking(move || skills::detail(&src, &id))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

/// A text file of a skill, by its path inside the skill's folder.
#[tauri::command]
pub async fn skills_file(
    state: State<'_, AppState>,
    id: String,
    path: String,
) -> IpcResult<String> {
    let src = sources(&state).await?;
    tokio::task::spawn_blocking(move || skills::read_file(&src, &id, &path))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

/// Shows a skill's folder in Finder.
#[tauri::command]
pub async fn skills_reveal(state: State<'_, AppState>, id: String) -> IpcResult<()> {
    let src = sources(&state).await?;
    let folder = tokio::task::spawn_blocking(move || skills::folder_of(&src, &id))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))??;
    std::process::Command::new("open")
        .arg("-R")
        .arg(folder)
        .spawn()?;
    Ok(())
}

/// Opens a skill's `SKILL.md` in the text editor macOS uses for text files.
#[tauri::command]
pub async fn skills_open(state: State<'_, AppState>, id: String) -> IpcResult<()> {
    let src = sources(&state).await?;
    let folder = tokio::task::spawn_blocking(move || skills::folder_of(&src, &id))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))??;
    std::process::Command::new("open")
        .arg("-t")
        .arg(folder.join("SKILL.md"))
        .spawn()?;
    Ok(())
}
