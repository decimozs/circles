//! Usage of Claude Code. The numbers are kept in the app's database and brought up to date from
//! Claude Code's transcripts in the background, so reading them is quick.

use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager, State};

use crate::ipc::IpcResult;
use crate::state::AppState;
use crate::usage::scan;
use crate::usage::store::{status, Running, UsageStore};
use crate::usage::{summarize, UsageSummary};

/// The folders to read: the one set in Settings, and every `~/.claude*` that has history.
async fn folders(state: &AppState) -> Vec<std::path::PathBuf> {
    let env = crate::commands::app::env_with_settings(state, None).await;
    let configured = env.get("CLAUDE_CONFIG_DIR").map(String::from);
    scan::config_dirs(dirs::home_dir().as_deref(), configured.as_deref())
}

/// Tokens and estimated cost since `since_secs` (unix seconds; all time when unset), for the
/// Claude folder `folder` or, when unset, every folder. This reads what is kept, so it answers at
/// once; `usage_refresh` brings it up to date.
#[tauri::command]
pub async fn usage_summary(
    state: State<'_, AppState>,
    since_secs: Option<i64>,
    folder: Option<String>,
) -> IpcResult<UsageSummary> {
    let store = UsageStore::new(state.db.clone());
    let current = folders(&state).await;
    let (records, mut known, skipped, refreshed_at) = {
        let store = store.clone();
        tokio::task::spawn_blocking(move || -> crate::error::AppResult<_> {
            Ok((
                store.records(since_secs, folder.as_deref())?,
                store.folders()?,
                store.skipped()?,
                store.refreshed_at()?,
            ))
        })
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("usage read stopped: {e}")))??
    };
    // Folders that exist now are offered even before anything was read from them.
    for dir in current {
        let name = dir.to_string_lossy().into_owned();
        if !known.contains(&name) {
            known.push(name);
        }
    }
    known.sort();
    let mut summary = summarize(&records, None, None);
    let (refreshing, files_done, files_total) = status();
    summary.folders = known;
    summary.skipped_lines = skipped;
    summary.refreshed_at = refreshed_at;
    summary.refreshing = refreshing;
    summary.files_done = files_done;
    summary.files_total = files_total;
    Ok(summary)
}

/// Reads what is new in Claude Code's transcripts. Only one runs at a time; asking while one runs
/// does nothing. The window is told with `usage:changed` while it goes and when it is done.
pub async fn refresh(app: &AppHandle, state: &AppState) {
    let Some(running) = Running::start() else {
        return;
    };
    let dirs = folders(state).await;
    let store = UsageStore::new(state.db.clone());
    let events = app.clone();
    let _ = events.emit("usage:changed", ());
    let result = tokio::task::spawn_blocking(move || {
        let mut last = Instant::now();
        let outcome = store.refresh(&dirs, &mut |done, total| {
            running.progress(done, total);
            // While there is a lot to read, the numbers grow on screen.
            if last.elapsed() > Duration::from_secs(1) {
                last = Instant::now();
                let _ = events.emit("usage:changed", ());
            }
        });
        drop(running);
        outcome
    })
    .await;
    match result {
        Ok(Ok(stats)) => tracing::info!(
            files = stats.files,
            read = stats.read,
            messages = stats.messages,
            "usage history refreshed"
        ),
        Ok(Err(e)) => tracing::warn!(error = %e, "usage history could not be refreshed"),
        Err(e) => tracing::warn!(error = %e, "usage refresh stopped"),
    }
    let _ = app.emit("usage:changed", ());
}

/// Starts a refresh in the background and returns at once.
#[tauri::command]
pub async fn usage_refresh(app: AppHandle) -> IpcResult<()> {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        refresh(&app, &state).await;
    });
    Ok(())
}

/// Forgets everything kept and reads the transcripts again. What Claude Code has deleted since
/// cannot come back.
#[tauri::command]
pub async fn usage_rebuild(app: AppHandle, state: State<'_, AppState>) -> IpcResult<()> {
    UsageStore::new(state.db.clone()).clear()?;
    usage_refresh(app).await
}
