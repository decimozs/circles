use tauri::{AppHandle, Emitter, State};

use crate::integrations::JiraStatus;
use crate::ipc::IpcResult;
use crate::state::AppState;

#[tauri::command]
pub async fn jira_status(state: State<'_, AppState>) -> IpcResult<JiraStatus> {
    state.integrations.jira_status()
}

/// Checks the token with Jira and saves it in the Keychain. Also reconnects: the same call with a new token.
#[tauri::command]
pub async fn jira_connect(
    app: AppHandle,
    state: State<'_, AppState>,
    site: String,
    email: String,
    token: String,
) -> IpcResult<JiraStatus> {
    let status = state
        .integrations
        .jira_connect(&site, &email, &token)
        .await?;
    let _ = app.emit("integrations:changed", ());
    Ok(status)
}

#[tauri::command]
pub async fn jira_disconnect(app: AppHandle, state: State<'_, AppState>) -> IpcResult<JiraStatus> {
    let status = state.integrations.jira_disconnect()?;
    // Nothing is left to use, so no Composer keeps it switched on.
    state.composers.store.clear_integration("jira")?;
    let _ = app.emit("integrations:changed", ());
    Ok(status)
}

/// Signs in with Atlassian in the browser. Returns when the person has finished there, or the
/// sign-in failed, timed out or was cancelled.
#[tauri::command]
pub async fn jira_sign_in(app: AppHandle, state: State<'_, AppState>) -> IpcResult<JiraStatus> {
    use tauri_plugin_opener::OpenerExt;
    let opener = app.clone();
    let open = move |url: &str| {
        opener
            .opener()
            .open_url(url, None::<&str>)
            .map_err(|e| e.to_string())
    };
    let status = state.integrations.jira_sign_in(&open).await?;
    let _ = app.emit("integrations:changed", ());
    Ok(status)
}

#[tauri::command]
pub async fn jira_cancel_sign_in(state: State<'_, AppState>) -> IpcResult<()> {
    state.integrations.jira_cancel_sign_in();
    Ok(())
}

/// After signing in with an account that reaches several Jira sites.
#[tauri::command]
pub async fn jira_choose_site(
    app: AppHandle,
    state: State<'_, AppState>,
    cloud_id: String,
) -> IpcResult<JiraStatus> {
    let status = state.integrations.jira_choose_site(&cloud_id)?;
    let _ = app.emit("integrations:changed", ());
    Ok(status)
}
