//! Titles and suggestions made by a small model. Nothing here waits on the model before a chat can
//! go on: the caller already has a title, and the model's one replaces it later.

use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, State};
use ts_rs::TS;

use crate::ipc::IpcResult;
use crate::settings::SmartSettings;
use crate::smart::claude::ClaudeHaiku;
use crate::smart::TextModel;
use crate::state::AppState;

pub(crate) fn settings_of(state: &AppState) -> SmartSettings {
    crate::commands::app::settings_layers(state, None)
        .ok()
        .and_then(|layers| crate::settings::load_merged(&layers).ok())
        .map(|payload| payload.settings.smart)
        .unwrap_or_default()
}

/// The model to use for these settings, if there is one.
pub(crate) async fn provider(
    state: &AppState,
    settings: &SmartSettings,
) -> Option<Arc<dyn TextModel>> {
    match settings.provider.as_deref().unwrap_or("embedded") {
        "off" => None,
        "claude" => {
            let env = state.env.get().await;
            let program = env.which("claude")?;
            Some(Arc::new(ClaudeHaiku { program, env }))
        }
        _ => state.smart.embedded().map(|m| m as Arc<dyn TextModel>),
    }
}

/// Names a chat that has no name: first from its first words, at once, and then, in the
/// background, by the model, unless a person has named it in the meantime.
#[tauri::command]
pub async fn session_auto_title(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    title: String,
    text: String,
) -> IpcResult<()> {
    // The chat must exist.
    state.sessions.get(&session_id)?;
    crate::smart::naming::name_new_chat(&app, &session_id, &title, &text).await;
    Ok(())
}

/// A likely next message, or nothing. `asked` and `answer` are the last exchange.
#[tauri::command]
pub async fn smart_suggest(
    state: State<'_, AppState>,
    asked: String,
    answer: String,
) -> IpcResult<Option<String>> {
    let settings = settings_of(&state);
    if settings.suggestions != Some(true) {
        return Ok(None);
    }
    let Some(model) = provider(&state, &settings).await else {
        return Ok(None);
    };
    let redactor = &state.redactor;
    let asked = redactor.redact(&asked);
    let answer = redactor.redact(&answer);
    Ok(crate::smart::suggestion_for(model.as_ref(), &asked, &answer).await)
}

/// An example first message for a Composer, from its name and what it is for. Shown as the
/// placeholder of its first message box. It does not depend on the setting for suggestions.
#[tauri::command]
pub async fn smart_first_prompt(
    state: State<'_, AppState>,
    name: String,
    purpose: String,
) -> IpcResult<Option<String>> {
    let settings = settings_of(&state);
    // Its own switch, on by default: it is how a new Composer shows what it is for.
    if settings.first_prompt == Some(false) {
        return Ok(None);
    }
    let Some(model) = provider(&state, &settings).await else {
        return Ok(None);
    };
    let purpose = state.redactor.redact(&purpose);
    Ok(crate::smart::first_prompt_for(model.as_ref(), &name, &purpose).await)
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct SmartStatus {
    /// The model file is in place.
    pub embedded_available: bool,
    pub provider: String,
}

#[tauri::command]
pub async fn smart_status(state: State<'_, AppState>) -> IpcResult<SmartStatus> {
    let settings = settings_of(&state);
    Ok(SmartStatus {
        embedded_available: state.smart.embedded().is_some(),
        provider: settings.provider.unwrap_or_else(|| "embedded".into()),
    })
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct SmartTest {
    pub title: Option<String>,
    pub suggestion: Option<String>,
    #[ts(type = "number")]
    pub millis: u64,
    pub provider: String,
}

/// Tries the model on a fixed example, so the person can see it works and how long it takes.
#[tauri::command]
pub async fn smart_test(state: State<'_, AppState>) -> IpcResult<SmartTest> {
    let settings = settings_of(&state);
    let name = settings
        .provider
        .clone()
        .unwrap_or_else(|| "embedded".into());
    let Some(model) = provider(&state, &settings).await else {
        return Ok(SmartTest {
            title: None,
            suggestion: None,
            millis: 0,
            provider: name,
        });
    };
    let start = std::time::Instant::now();
    let title = crate::smart::title_for(
        model.as_ref(),
        "fix the failing test in the checkout flow and open a pull request",
    )
    .await;
    let suggestion = crate::smart::suggestion_for(
        model.as_ref(),
        "fix the failing test in the checkout flow",
        "The discount was applied after tax. I moved it before tax and the test passes.",
    )
    .await;
    Ok(SmartTest {
        title,
        suggestion,
        millis: start.elapsed().as_millis() as u64,
        provider: name,
    })
}
