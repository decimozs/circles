use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::checkpoint::snapshot::FileChange;
use crate::checkpoint::{Checkpoint, RevertOutcome, RevertPreview, UndoableRevert};
use crate::error::AppError;
use crate::harness::store::{NewSession, Session};
use crate::ipc::IpcResult;
use crate::state::AppState;

/// Emitted as `session:reverted` after a chat was reverted or a revert undone, so its view starts
/// again from what is stored.
#[derive(Debug, Clone, Serialize)]
struct SessionReverted {
    session_id: String,
    workspace_id: String,
}

async fn folder_of(state: &AppState, workspace_id: &str) -> Result<std::path::PathBuf, AppError> {
    let (ctx, _) = state.workspace_service().await.context_for(workspace_id)?;
    Ok(ctx.cwd)
}

fn session_of(checkpoint: &Checkpoint) -> Result<String, AppError> {
    checkpoint
        .session_id
        .clone()
        .ok_or_else(|| AppError::Invalid("That checkpoint belongs to no chat.".into()))
}

/// The turns of a chat that can be reverted to.
#[tauri::command]
pub async fn checkpoint_list(
    state: State<'_, AppState>,
    session_id: String,
) -> IpcResult<Vec<Checkpoint>> {
    state.checkpoints().await.list(&session_id)
}

#[tauri::command]
pub async fn checkpoint_preview(
    state: State<'_, AppState>,
    checkpoint_id: String,
) -> IpcResult<RevertPreview> {
    let service = state.checkpoints().await;
    let checkpoint = service.get(&checkpoint_id)?;
    let cwd = folder_of(&state, &checkpoint.workspace_id).await?;
    service.preview(&checkpoint_id, &cwd).await
}

/// Goes back to before a turn: the files and the conversation. Refused while a turn runs.
#[tauri::command]
pub async fn checkpoint_revert(
    app: AppHandle,
    state: State<'_, AppState>,
    checkpoint_id: String,
) -> IpcResult<RevertOutcome> {
    let service = state.checkpoints().await;
    let checkpoint = service.get(&checkpoint_id)?;
    let session_id = session_of(&checkpoint)?;
    if state.sessions.get(&session_id)?.status.is_active() {
        return Err(AppError::Invalid(
            "A turn is running in this chat. Stop it first, then revert.".into(),
        ));
    }
    let cwd = folder_of(&state, &checkpoint.workspace_id).await?;
    // The agent's process holds the old conversation, so it ends. The next message starts a new
    // one that is told what came before.
    state.sessions.stop(&session_id).await?;
    let outcome = service.revert(&checkpoint_id, &cwd).await?;
    let _ = app.emit(
        "session:reverted",
        SessionReverted {
            session_id,
            workspace_id: checkpoint.workspace_id,
        },
    );
    Ok(outcome)
}

#[tauri::command]
pub async fn checkpoint_last_revert(
    state: State<'_, AppState>,
    session_id: String,
) -> IpcResult<Option<UndoableRevert>> {
    state.checkpoints().await.last_revert(&session_id)
}

/// Takes the last revert of a chat back.
#[tauri::command]
pub async fn checkpoint_undo_revert(
    app: AppHandle,
    state: State<'_, AppState>,
    revert_id: String,
    session_id: String,
) -> IpcResult<Vec<FileChange>> {
    let service = state.checkpoints().await;
    let session = state.sessions.get(&session_id)?;
    if session.status.is_active() {
        return Err(AppError::Invalid("A turn is running in this chat.".into()));
    }
    let cwd = folder_of(&state, &session.workspace_id).await?;
    state.sessions.stop(&session_id).await?;
    let files = service.undo(&revert_id, &cwd).await?;
    let _ = app.emit(
        "session:reverted",
        SessionReverted {
            session_id,
            workspace_id: session.workspace_id,
        },
    );
    Ok(files)
}

/// Starts a new chat from the conversation up to the end of `turn`. From the last turn it is a
/// fork of the agent's session. From an earlier one the agent starts a new session and is told the
/// conversation so far.
#[tauri::command]
pub async fn session_branch(
    state: State<'_, AppState>,
    session_id: String,
    turn: i64,
) -> IpcResult<Session> {
    let service = state.checkpoints().await;
    let source = state.sessions.get(&session_id)?;
    let turns = service.list(&session_id)?;
    if !turns.iter().any(|c| c.turn == turn) {
        return Err(AppError::Invalid("That turn does not exist.".into()));
    }
    let next = turns
        .iter()
        .find(|c| c.turn == turn + 1)
        .and_then(|c| c.message_seq);
    let title = format!(
        "{} (branch)",
        source.title.clone().unwrap_or_else(|| "Chat".into())
    );
    match next {
        None => {
            let claude = source.claude_session_id.clone().ok_or_else(|| {
                AppError::Invalid(
                    "That chat has no conversation yet, so there is nothing to copy.".into(),
                )
            })?;
            state.sessions.create(
                &source.workspace_id,
                NewSession {
                    title: Some(title),
                    model: source.model.clone(),
                    effort: source.effort.clone(),
                    permission_mode: Some(source.permission_mode),
                    fork_of: Some(claude),
                },
            )
        }
        Some(before) => {
            let created = state.sessions.create(
                &source.workspace_id,
                NewSession {
                    title: Some(title),
                    model: source.model.clone(),
                    effort: source.effort.clone(),
                    permission_mode: Some(source.permission_mode),
                    fork_of: None,
                },
            )?;
            let context = service.context_before(&session_id, before)?;
            state
                .sessions
                .store()
                .set_replay_context(&created.id, context.as_deref())?;
            Ok(created)
        }
    }
}
