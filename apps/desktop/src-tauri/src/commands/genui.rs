//! Cards agents show in the chat: the person's answers, and what is waiting.

use serde_json::Value;
use tauri::State;

use crate::genui::store::StoredCard;
use crate::ipc::IpcResult;
use crate::state::AppState;

/// A person's answer to a card. Checked against the card, and counted once.
#[tauri::command]
pub async fn genui_answer(state: State<'_, AppState>, uid: String, values: Value) -> IpcResult<()> {
    state.genui.answer(&uid, &values)
}

/// Closes a card without answering it.
#[tauri::command]
pub async fn genui_dismiss(state: State<'_, AppState>, uid: String) -> IpcResult<()> {
    state.genui.dismiss(&uid)
}

/// The cards of a chat, oldest first, with their answers.
#[tauri::command]
pub async fn genui_cards(
    state: State<'_, AppState>,
    session_id: String,
) -> IpcResult<Vec<StoredCard>> {
    state.genui.store.list(&session_id, 50)
}

/// Every card that waits for a person.
#[tauri::command]
pub async fn genui_pending(state: State<'_, AppState>) -> IpcResult<Vec<StoredCard>> {
    state.genui.store.pending()
}
