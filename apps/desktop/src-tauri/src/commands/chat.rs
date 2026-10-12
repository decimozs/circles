use tauri::State;

use crate::chat::{self, Attachment, FileHit, SlashCommand};
use crate::ipc::IpcResult;
use crate::state::AppState;

/// Files and folders of the workspace that match `query`, for `@` mentions.
#[tauri::command]
pub async fn workspace_search_files(
    state: State<'_, AppState>,
    workspace_id: String,
    query: String,
    limit: Option<u32>,
) -> IpcResult<Vec<FileHit>> {
    let (ctx, _) = state.workspace_service().await.context_for(&workspace_id)?;
    let limit = limit.unwrap_or(20).min(100) as usize;
    let hits = tokio::task::spawn_blocking(move || chat::search_files(&ctx.cwd, &query, limit))
        .await
        .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    Ok(hits)
}

/// Slash commands the composer offers: built-ins, project and user commands, and skills.
#[tauri::command]
pub async fn slash_commands(
    state: State<'_, AppState>,
    workspace_id: String,
) -> IpcResult<Vec<SlashCommand>> {
    let (ctx, _) = state.workspace_service().await.context_for(&workspace_id)?;
    Ok(chat::slash_commands(&ctx.cwd, dirs::home_dir().as_deref()))
}

/// Stores a file the user attached or pasted, inside the workspace's `.context` folder.
#[tauri::command]
pub async fn attachment_save(
    state: State<'_, AppState>,
    workspace_id: String,
    name: String,
    data_base64: String,
) -> IpcResult<Attachment> {
    let (ctx, _) = state.workspace_service().await.context_for(&workspace_id)?;
    chat::save_attachment(&ctx.cwd, &name, &data_base64)
}
