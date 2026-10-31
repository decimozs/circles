//! The copies of GitHub answers kept so screens open at once.

use serde::Serialize;
use tauri::State;
use ts_rs::TS;

use crate::cache::CacheStore;
use crate::ipc::IpcResult;
use crate::state::AppState;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct CacheInfo {
    pub entries: u32,
    #[ts(type = "number")]
    pub bytes: u64,
}

#[tauri::command]
pub async fn cache_info(state: State<'_, AppState>) -> IpcResult<CacheInfo> {
    let (entries, bytes) = CacheStore::new(state.db.clone()).info()?;
    Ok(CacheInfo { entries, bytes })
}

/// Deletes every copy. They are fetched again when needed.
#[tauri::command]
pub async fn cache_clear(state: State<'_, AppState>) -> IpcResult<()> {
    CacheStore::new(state.db.clone()).clear()?;
    Ok(())
}
