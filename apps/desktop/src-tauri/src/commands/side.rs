//! Side questions: a question about a chat, answered in a small pane. See `crate::side`.

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;

use crate::error::AppError;
use crate::ipc::IpcResult;
use crate::side::{self, SideEvent, SideRequest};
use crate::state::AppState;

#[derive(Serialize, Clone)]
struct SidePayload<'a> {
    session_id: &'a str,
    /// `started`, `delta`, `done` or `failed`.
    kind: &'a str,
    text: String,
    /// For `started`: the chat's own context is used, and not only the question.
    with_context: bool,
}

fn emit(app: &AppHandle, session_id: &str, kind: &str, text: String, with_context: bool) {
    let _ = app.emit(
        "side:event",
        SidePayload {
            session_id,
            kind,
            text,
            with_context,
        },
    );
}

/// Asks a side question about a chat. Returns at once. The answer arrives as `side:event`.
#[tauri::command]
pub async fn session_side_question(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    question: String,
) -> IpcResult<()> {
    let question = question.trim().to_string();
    if question.is_empty() {
        return Err(AppError::Invalid("Write the question after /btw.".into()));
    }
    if question.chars().count() > side::QUESTION_LIMIT {
        return Err(AppError::Invalid(format!(
            "A side question can be at most {} characters.",
            side::QUESTION_LIMIT
        )));
    }
    let session = state.sessions.get(&session_id)?;
    let workspace = state.workspace_service().await.get(&session.workspace_id)?;
    let settings = crate::commands::app::settings_layers(&state, None)
        .ok()
        .and_then(|layers| crate::settings::load_merged(&layers).ok())
        .map(|payload| payload.settings.side)
        .unwrap_or_default();
    let env = state.env.get().await;
    let program = env
        .which("claude")
        .ok_or_else(|| AppError::ClaudeMissing("Claude Code (`claude`) was not found.".into()))?;
    let model = match settings.model.as_deref().unwrap_or("haiku") {
        "chat" => session.model.clone(),
        _ => Some("haiku".to_string()),
    };
    // Secrets do not go into a question.
    let question = state.redactor.redact(&question);
    let request = SideRequest {
        program,
        cwd: std::path::PathBuf::from(&workspace.path),
        resume: session.claude_session_id.clone(),
        model,
        question,
        read_files: settings.read_files == Some(true),
    };
    let with_context = request.resume.is_some();
    let (cancel, number) = state.side.begin(&session_id);
    emit(&app, &session_id, "started", String::new(), with_context);

    let (tx, mut rx) = mpsc::unbounded_channel();
    tauri::async_runtime::spawn(side::run(request, env, tx, cancel));
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                SideEvent::Delta(text) => emit(&handle, &session_id, "delta", text, with_context),
                SideEvent::Done(text) => {
                    emit(&handle, &session_id, "done", text, with_context);
                    break;
                }
                SideEvent::Failed(message) => {
                    emit(&handle, &session_id, "failed", message, with_context);
                    break;
                }
            }
        }
        use tauri::Manager;
        handle
            .state::<AppState>()
            .side
            .finished(&session_id, number);
    });
    Ok(())
}

/// Stops the side question of a chat.
#[tauri::command]
pub async fn session_side_stop(state: State<'_, AppState>, session_id: String) -> IpcResult<()> {
    state.side.stop(&session_id);
    Ok(())
}
