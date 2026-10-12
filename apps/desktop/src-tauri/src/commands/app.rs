use std::path::PathBuf;

use rusqlite::params;
use serde::Serialize;
use tauri::State;
use ts_rs::TS;

use crate::env::{EnvSource, TOOLS};
use crate::error::AppError;
use crate::ipc::IpcResult;
use crate::paths::project_settings_file;
use crate::settings::{self, Scope, SettingsPayload};
use crate::state::AppState;

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct PathsInfo {
    pub data_dir: String,
    pub db_file: String,
    pub settings_file: String,
    pub logs_dir: String,
    pub home_dir: String,
    pub workspaces_dir: String,
    pub repos_dir: String,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ToolInfo {
    pub name: String,
    pub path: Option<String>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct EnvInfo {
    pub source: EnvSource,
    pub var_count: u32,
    pub path_entries: Vec<String>,
    pub tools: Vec<ToolInfo>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct DbInfo {
    pub schema_version: u32,
    pub journal_mode: String,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct AppInfo {
    pub version: String,
    /// Name of this Mac, for the sidebar and Home headers.
    pub machine_name: String,
    pub platform: String,
    pub arch: String,
    pub paths: PathsInfo,
    pub env: EnvInfo,
    pub db: DbInfo,
}

#[tauri::command]
pub async fn get_app_info(state: State<'_, AppState>) -> IpcResult<AppInfo> {
    let env = state.env.get().await;
    let p = &state.paths;
    let show = |path: &PathBuf| path.display().to_string();

    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        machine_name: crate::machine::machine_name(),
        platform: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        paths: PathsInfo {
            data_dir: show(&p.data_dir),
            db_file: show(&p.db_file),
            settings_file: show(&p.settings_file),
            logs_dir: show(&p.logs_dir),
            home_dir: show(&p.home_dir),
            workspaces_dir: show(&p.workspaces_dir),
            repos_dir: show(&p.repos_dir),
        },
        env: EnvInfo {
            source: env.source().clone(),
            var_count: env.var_count() as u32,
            path_entries: env.path_entries(),
            tools: TOOLS
                .iter()
                .map(|name| ToolInfo {
                    name: (*name).to_string(),
                    path: env.which(name).map(|p| p.display().to_string()),
                })
                .collect(),
        },
        db: DbInfo {
            schema_version: state.db.schema_version()?,
            journal_mode: state.db.journal_mode()?,
        },
    })
}

/// The files the settings of a project come from, lowest level first. Without a `project_id` only
/// the user file applies.
pub(crate) fn settings_layers(
    state: &AppState,
    project_id: Option<&str>,
) -> Result<Vec<(Scope, PathBuf)>, AppError> {
    let mut layers = vec![(Scope::User, state.paths.settings_file.clone())];
    if let Some(id) = project_id {
        let repo_path: String = state
            .db
            .read(|c| {
                c.query_row(
                    "SELECT path FROM projects WHERE id = ?1",
                    params![id],
                    |r| r.get(0),
                )
            })
            .map_err(|e| match e {
                AppError::Db(rusqlite::Error::QueryReturnedNoRows) => {
                    AppError::NotFound(format!("project {id}"))
                }
                other => other,
            })?;
        layers.push((
            Scope::Project,
            project_settings_file(&PathBuf::from(repo_path)),
        ));
        layers.push((Scope::LocalProject, state.paths.local_project_settings(id)));
    }
    Ok(layers)
}

/// Merged settings: user, then the project's shared file, then the local override.
/// Without a `project_id` only the user file applies.
#[tauri::command]
pub async fn settings_get(
    state: State<'_, AppState>,
    project_id: Option<String>,
) -> IpcResult<SettingsPayload> {
    settings::load_merged(&settings_layers(&state, project_id.as_deref())?)
}
