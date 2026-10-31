//! Usage of Claude Code, read from its own transcripts.

use tauri::State;

use crate::ipc::IpcResult;
use crate::state::AppState;
use crate::usage::{scan, summarize, UsageSummary};

/// Tokens and estimated cost since `since_secs` (unix seconds; all time when unset), for the
/// Claude folder `folder` or, when unset, every folder found.
#[tauri::command]
pub async fn usage_summary(
    state: State<'_, AppState>,
    since_secs: Option<i64>,
    folder: Option<String>,
) -> IpcResult<UsageSummary> {
    let env = crate::commands::app::env_with_settings(&state, None).await;
    let configured = env.get("CLAUDE_CONFIG_DIR").map(String::from);
    let home = dirs::home_dir();
    let mut dirs = scan::config_dirs(home.as_deref(), configured.as_deref());
    let folders: Vec<String> = dirs
        .iter()
        .map(|d| d.to_string_lossy().into_owned())
        .collect();
    if let Some(only) = folder.as_deref() {
        dirs.retain(|d| d.to_string_lossy() == only);
    }
    // Reading a long history takes a while and must not hold up the app.
    let scanned = tokio::task::spawn_blocking(move || scan::scan(&dirs))
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("usage scan stopped: {e}")))?;
    let mut summary = summarize(&scanned.records, since_secs, None);
    summary.folders = folders;
    summary.skipped_lines = scanned.skipped;
    Ok(summary)
}
