use std::sync::atomic::Ordering;
use std::sync::Arc;

use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, State};

use crate::error::AppError;
use crate::ipc::IpcResult;
use crate::scripts::ScriptsInfo;
use crate::state::AppState;
use crate::terminal::tauri::ChannelSink;
use crate::terminal::{AttachInfo, Terminal};

#[tauri::command]
pub async fn terminal_list(
    state: State<'_, AppState>,
    workspace_id: String,
) -> IpcResult<Vec<Terminal>> {
    state.terminals.list(&workspace_id)
}

#[tauri::command]
pub async fn terminal_create(
    state: State<'_, AppState>,
    workspace_id: String,
    title: Option<String>,
) -> IpcResult<Terminal> {
    state.terminals.create(&workspace_id, title.as_deref())
}

#[tauri::command]
pub async fn terminal_rename(
    state: State<'_, AppState>,
    id: String,
    title: String,
) -> IpcResult<Terminal> {
    state.terminals.rename(&id, &title)
}

/// Closes a free terminal tab and ends its shell.
#[tauri::command]
pub async fn terminal_delete(state: State<'_, AppState>, id: String) -> IpcResult<()> {
    state.terminals.delete(&id).await
}

/// Connects the view to a tab. Output arrives on `on_event`: raw bytes for the terminal, and JSON
/// control messages (`exit`, `reset`).
#[tauri::command]
pub async fn terminal_attach(
    state: State<'_, AppState>,
    id: String,
    cols: u16,
    rows: u16,
    on_event: Channel<InvokeResponseBody>,
) -> IpcResult<AttachInfo> {
    let workspace_id = state.terminals.workspace_of(&id)?;
    let service = state.workspace_service().await;
    let (ctx, _) = service.context_for(&workspace_id)?;
    state.terminals.attach(
        &service.env,
        &ctx,
        &id,
        cols,
        rows,
        Arc::new(ChannelSink(on_event)),
    )
}

#[tauri::command]
pub async fn terminal_detach(state: State<'_, AppState>, id: String) -> IpcResult<()> {
    state.terminals.detach(&id);
    Ok(())
}

#[tauri::command]
pub async fn terminal_write(state: State<'_, AppState>, id: String, data: String) -> IpcResult<()> {
    state.terminals.write(&id, data.as_bytes())
}

#[tauri::command]
pub async fn terminal_resize(
    state: State<'_, AppState>,
    id: String,
    cols: u16,
    rows: u16,
) -> IpcResult<()> {
    state.terminals.resize(&id, cols, rows)
}

/// Ends the process in a tab and keeps the tab and its output.
#[tauri::command]
pub async fn terminal_stop(state: State<'_, AppState>, id: String) -> IpcResult<Terminal> {
    state.terminals.stop(&id).await;
    state.terminals.get(&id)
}

/// Replaces the ended shell of a free terminal with a new one when a view next attaches.
#[tauri::command]
pub async fn terminal_restart(state: State<'_, AppState>, id: String) -> IpcResult<()> {
    state.terminals.restart_shell(&id).await
}

/// The scripts configured for a workspace.
#[tauri::command]
pub async fn scripts_get(
    state: State<'_, AppState>,
    workspace_id: String,
) -> IpcResult<ScriptsInfo> {
    let (_, info) = state.workspace_service().await.context_for(&workspace_id)?;
    Ok(info)
}

/// Runs a run script in the Run tab. Without a name, the default script runs.
#[tauri::command]
pub async fn script_run(
    state: State<'_, AppState>,
    workspace_id: String,
    name: Option<String>,
) -> IpcResult<Terminal> {
    let service = state.workspace_service().await;
    let (ctx, info) = service.context_for(&workspace_id)?;
    let script = match &name {
        Some(name) => info.run.iter().find(|s| &s.name == name),
        None => info.run.first(),
    }
    .ok_or_else(|| match &name {
        Some(name) => AppError::NotFound(format!("run script \"{name}\"")),
        None => AppError::Invalid(
            "This project has no run script. Add one under [scripts] in .circles/settings.toml."
                .into(),
        ),
    })?
    .clone();
    state
        .terminals
        .run_script(&service.env, &ctx, &script, info.run_mode)
        .await
}

#[tauri::command]
pub async fn script_stop(state: State<'_, AppState>, workspace_id: String) -> IpcResult<Terminal> {
    state.terminals.stop_script(&workspace_id).await
}

/// Runs the setup script again in the Setup tab.
#[tauri::command]
pub async fn script_run_setup(
    state: State<'_, AppState>,
    workspace_id: String,
) -> IpcResult<Terminal> {
    let service = state.workspace_service().await;
    let (ctx, info) = service.context_for(&workspace_id)?;
    let command = info
        .setup
        .ok_or_else(|| AppError::Invalid("This project has no setup script.".into()))?;
    state
        .terminals
        .run_setup(&service.env, &ctx, &command)
        .await?;
    let tab = state
        .terminals
        .list(&workspace_id)?
        .into_iter()
        .find(|t| t.kind == crate::terminal::TerminalKind::Setup)
        .ok_or_else(|| AppError::Internal("the Setup tab is missing".into()))?;
    Ok(tab)
}

/// How many terminals, scripts and agents are busy, for the quit confirmation.
#[tauri::command]
pub async fn terminals_running(state: State<'_, AppState>) -> IpcResult<u32> {
    Ok(state.running_work() as u32)
}

/// Quits after the user confirmed. Everything that is still running is ended.
#[tauri::command]
pub async fn app_quit(app: AppHandle, state: State<'_, AppState>) -> IpcResult<()> {
    state.quit_confirmed.store(true, Ordering::SeqCst);
    state.kill_everything();
    app.exit(0);
    Ok(())
}

/// Creates `.circles/settings.toml` from the template if needed and opens it in the default
/// text editor. Returns its path.
#[tauri::command]
pub async fn project_open_settings(
    state: State<'_, AppState>,
    project_id: String,
) -> IpcResult<String> {
    let project = state.project_service().await.get(&project_id)?;
    let file = crate::settings::ensure_project_settings(std::path::Path::new(&project.path))?;
    std::process::Command::new("/usr/bin/open")
        .arg("-t")
        .arg(&file)
        .spawn()?;
    Ok(file.display().to_string())
}
