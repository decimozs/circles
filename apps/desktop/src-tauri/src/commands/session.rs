use std::sync::Arc;

use serde::Deserialize;
use tauri::ipc::Channel;
use tauri::State;
use ts_rs::TS;

use crate::chat::Attachment;
use crate::error::AppError;
use crate::harness::from_settings;
use crate::harness::parser::RateLimit;
use crate::harness::permissions::PermissionDecision;
use crate::harness::sessions::{AttachSnapshot, StreamItem};
use crate::harness::store::{EventRow, Message, NewSession, PermissionMode, Session};
use crate::harness::tauri::ChannelView;
use crate::ipc::IpcResult;
use crate::settings::Settings;
use crate::state::AppState;

fn mode_from(id: Option<&str>) -> Result<Option<PermissionMode>, AppError> {
    match id {
        None => Ok(None),
        Some(id) => PermissionMode::parse(id)
            .map(Some)
            .ok_or_else(|| AppError::Invalid(format!("\"{id}\" is not a permission mode."))),
    }
}

/// What the chat tab is created with.
#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(export)]
pub struct SessionOptions {
    pub title: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    /// `manual`, `accept_edits`, `plan` or `bypass`.
    pub permission_mode: Option<String>,
    /// Start as a copy of this chat's conversation.
    pub fork_from_session: Option<String>,
}

#[tauri::command]
pub async fn session_create(
    state: State<'_, AppState>,
    workspace_id: String,
    options: SessionOptions,
) -> IpcResult<Session> {
    let fork_of = match &options.fork_from_session {
        Some(source) => {
            let source = state.sessions.get(source)?;
            Some(source.claude_session_id.ok_or_else(|| {
                AppError::Invalid(
                    "That chat has no conversation yet, so there is nothing to copy.".into(),
                )
            })?)
        }
        None => None,
    };
    let defaults = settings_of(&state, &workspace_id).await;
    state.sessions.create(
        &workspace_id,
        from_settings::new_session_defaults(
            &defaults,
            NewSession {
                title: options.title,
                model: options.model,
                effort: options.effort,
                permission_mode: mode_from(options.permission_mode.as_deref())?,
                fork_of,
            },
        ),
    )
}

/// The settings that apply to a workspace. A settings file that cannot be read never stops a chat:
/// the defaults are used and the problem is logged.
pub(crate) async fn settings_of(state: &AppState, workspace_id: &str) -> Settings {
    match state.workspace_service().await.settings_for(workspace_id) {
        Ok(settings) => settings,
        Err(e) => {
            tracing::warn!(error = %e, "cannot read the settings, using the defaults");
            Settings::default()
        }
    }
}

/// Tells the sessions how Claude Code is to be started, from the current settings.
pub(crate) async fn refresh_agent(state: &AppState, workspace_id: &str) {
    let settings = settings_of(state, workspace_id).await;
    state
        .sessions
        .set_agent_settings(from_settings::agent_settings(&settings));
}

#[tauri::command]
pub async fn session_list(
    state: State<'_, AppState>,
    workspace_id: String,
    include_closed: Option<bool>,
) -> IpcResult<Vec<Session>> {
    state
        .sessions
        .list_with(&workspace_id, include_closed.unwrap_or(false))
}

/// Brings a closed chat back as an open tab.
#[tauri::command]
pub async fn session_reopen(state: State<'_, AppState>, id: String) -> IpcResult<Session> {
    state.sessions.reopen(&id)
}

#[tauri::command]
pub async fn session_reorder(
    state: State<'_, AppState>,
    workspace_id: String,
    ids: Vec<String>,
) -> IpcResult<()> {
    state.sessions.reorder(&workspace_id, &ids)
}

#[tauri::command]
pub async fn session_get(state: State<'_, AppState>, id: String) -> IpcResult<Session> {
    state.sessions.get(&id)
}

#[tauri::command]
pub async fn session_rename(
    state: State<'_, AppState>,
    id: String,
    title: String,
) -> IpcResult<Session> {
    state.sessions.rename(&id, &title)
}

/// Chat messages after `after_seq`, oldest first.
#[tauri::command]
pub async fn session_messages(
    state: State<'_, AppState>,
    id: String,
    after_seq: Option<i64>,
) -> IpcResult<Vec<Message>> {
    state.sessions.messages(&id, after_seq)
}

/// Raw events after `after_seq`, for the developer view.
#[tauri::command]
pub async fn session_events(
    state: State<'_, AppState>,
    id: String,
    after_seq: Option<i64>,
    limit: Option<u32>,
) -> IpcResult<Vec<EventRow>> {
    state.sessions.events(&id, after_seq, limit.unwrap_or(500))
}

/// Connects a view. Live items arrive on `on_event`. History is read with `session_messages`.
#[tauri::command]
pub async fn session_attach(
    state: State<'_, AppState>,
    id: String,
    on_event: Channel<StreamItem>,
) -> IpcResult<AttachSnapshot> {
    state.sessions.attach(&id, Arc::new(ChannelView(on_event)))
}

#[tauri::command]
pub async fn session_detach(state: State<'_, AppState>, id: String) -> IpcResult<()> {
    state.sessions.detach(&id);
    Ok(())
}

/// Sends a message. While the chat is working, this steers the current turn.
#[tauri::command]
pub async fn session_send(
    state: State<'_, AppState>,
    id: String,
    text: String,
    attachments: Option<Vec<Attachment>>,
) -> IpcResult<Message> {
    let workspace_id = state.sessions.store().workspace_of(&id)?;
    let (ctx, _) = state.workspace_service().await.context_for(&workspace_id)?;
    refresh_agent(&state, &workspace_id).await;
    state
        .sessions
        .send_with(&ctx, &id, &text, &attachments.unwrap_or_default())
        .await
}

/// Starts the process for a chat that has none, without sending anything.
#[tauri::command]
pub async fn session_resume(state: State<'_, AppState>, id: String) -> IpcResult<Session> {
    let workspace_id = state.sessions.store().workspace_of(&id)?;
    let (ctx, _) = state.workspace_service().await.context_for(&workspace_id)?;
    refresh_agent(&state, &workspace_id).await;
    state.sessions.resume(&ctx, &id).await
}

#[tauri::command]
pub async fn session_interrupt(state: State<'_, AppState>, id: String) -> IpcResult<()> {
    state.sessions.interrupt(&id)
}

/// Ends the process. The conversation is kept and the next message resumes it.
#[tauri::command]
pub async fn session_stop(state: State<'_, AppState>, id: String) -> IpcResult<()> {
    state.sessions.stop(&id).await
}

/// Closes the chat tab.
#[tauri::command]
pub async fn session_close(state: State<'_, AppState>, id: String) -> IpcResult<()> {
    state.sessions.close(&id).await
}

#[tauri::command]
pub async fn session_update(
    state: State<'_, AppState>,
    id: String,
    model: Option<String>,
    effort: Option<String>,
    permission_mode: Option<String>,
) -> IpcResult<Session> {
    let mode = mode_from(permission_mode.as_deref())?;
    state
        .sessions
        .update_options(&id, model.as_deref(), effort.as_deref(), mode)
        .await
}

#[tauri::command]
pub async fn permission_reply(
    state: State<'_, AppState>,
    session_id: String,
    request_id: String,
    decision: PermissionDecision,
) -> IpcResult<()> {
    state
        .sessions
        .reply_permission(&session_id, &request_id, decision)
}

/// The latest rate limit Claude Code reported, if any.
#[tauri::command]
pub async fn claude_rate_limit(state: State<'_, AppState>) -> IpcResult<Option<RateLimit>> {
    Ok(state.sessions.rate_limit())
}
