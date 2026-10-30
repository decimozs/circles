//! Naming a chat that has no name: its first words at once, and Claude Haiku's title in their place a
//! moment later, unless a person named it in the meantime. Used by the app's own chat box and by
//! the chats that Composers start.

use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;

/// The first line of the message, cut to 40 characters. The same as the chat box makes.
pub fn first_words(text: &str) -> String {
    let line = text.trim().lines().next().unwrap_or("").trim();
    if line.chars().count() > 40 {
        let cut: String = line.chars().take(39).collect();
        format!("{cut}…")
    } else {
        line.to_string()
    }
}

/// Names a chat from its first message. Never waits for the model, and never replaces a name a
/// person gave.
pub async fn name_new_chat(app: &AppHandle, session_id: &str, title: &str, text: &str) {
    let state = app.state::<AppState>();
    let store = state.sessions.store().clone();
    match store.set_first_title(session_id, title) {
        Ok(true) => {}
        // It was named already, or there is nothing to name it from.
        _ => return,
    }
    let settings = crate::commands::smart::settings_of(&state);
    if settings.titles == Some(false) {
        return;
    }
    let Some(model) = crate::commands::smart::provider(&state).await else {
        return;
    };
    let Ok(session) = state.sessions.get(session_id) else {
        return;
    };
    let workspace_id = session.workspace_id;
    let session_id = session_id.to_string();
    let text = text.to_string();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(better) = crate::smart::title_for(model.as_ref(), &text).await else {
            tracing::info!("no model title for a new chat, the first words stay");
            return;
        };
        tracing::info!(title = %better, "a model named a chat");
        let state = app.state::<AppState>();
        if state
            .sessions
            .store()
            .replace_auto_title(&session_id, &better)
            .unwrap_or(false)
        {
            let _ = app.emit(
                "session:title-changed",
                serde_json::json!({ "workspace_id": workspace_id, "session_id": session_id }),
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::first_words;

    #[test]
    fn the_first_words_are_the_first_line_cut_short() {
        assert_eq!(first_words("  fix the bug\nand more"), "fix the bug");
        assert_eq!(first_words(""), "");
        let long = "a".repeat(60);
        let cut = first_words(&long);
        assert_eq!(cut.chars().count(), 40);
        assert!(cut.ends_with('…'));
    }
}
