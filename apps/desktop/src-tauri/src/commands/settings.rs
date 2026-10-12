use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};
use ts_rs::TS;

use crate::commands::app::settings_layers;
use crate::error::AppError;
use crate::ipc::IpcResult;
use crate::settings::edit;
use crate::settings::{self, LayerInfo, Scope, Settings};
use crate::state::AppState;
use crate::system;

/// What one level of settings holds, as plain data, so the page can tell where a value comes from.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct LayerValues {
    pub scope: Scope,
    #[ts(type = "unknown")]
    pub values: Value,
}

/// The settings that apply, and the levels they come from.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct SettingsView {
    pub settings: Settings,
    /// Lowest to highest precedence.
    pub layers: Vec<LayerInfo>,
    pub values: Vec<LayerValues>,
}

fn scope_from(text: &str) -> Result<Scope, AppError> {
    match text {
        "user" => Ok(Scope::User),
        "project" => Ok(Scope::Project),
        "local" => Ok(Scope::LocalProject),
        other => Err(AppError::Invalid(format!(
            "\"{other}\" is not a level of settings."
        ))),
    }
}

fn view(state: &AppState, project_id: Option<&str>) -> Result<SettingsView, AppError> {
    let layers = settings_layers(state, project_id)?;
    let payload = settings::load_merged(&layers)?;
    let mut values = Vec::new();
    for (scope, path) in &layers {
        let table = settings::read_table(path)?.unwrap_or_default();
        values.push(LayerValues {
            scope: *scope,
            values: serde_json::to_value(&table).unwrap_or(Value::Null),
        });
    }
    Ok(SettingsView {
        settings: payload.settings,
        layers: payload.layers,
        values,
    })
}

#[tauri::command]
pub async fn settings_view(
    state: State<'_, AppState>,
    project_id: Option<String>,
) -> IpcResult<SettingsView> {
    view(&state, project_id.as_deref())
}

/// Changes one setting in the file of a level. `value` unset removes it. The result is checked
/// first and a value that does not pass is refused with a message, leaving the file as it was.
#[tauri::command]
pub async fn settings_set(
    app: AppHandle,
    state: State<'_, AppState>,
    scope: String,
    project_id: Option<String>,
    path: Vec<String>,
    value: Option<Value>,
) -> IpcResult<SettingsView> {
    let layers = settings_layers(&state, project_id.as_deref())?;
    edit::edit(&layers, scope_from(&scope)?, &path, value.as_ref())?;
    // Only the name of the setting is logged, since its value may be a secret.
    tracing::info!(setting = %path.join("."), scope = %scope, "setting changed");
    if path == ["genui", "composers"] {
        state
            .genui
            .set_composers(value.as_ref().and_then(Value::as_bool).unwrap_or(true));
    }
    if path == ["genui", "chats"] {
        state
            .genui
            .set_chats(value.as_ref().and_then(Value::as_bool).unwrap_or(false));
    }
    if path == ["integrations", "jira_in_chats"] {
        let allowed = value.as_ref().and_then(Value::as_bool).unwrap_or(true);
        state.integrations.set_chats(allowed);
    }
    if path == ["composers", "start_at_login"] {
        let wanted = value.as_ref().and_then(Value::as_bool);
        apply_login_item(&app, wanted);
    }
    view(&state, project_id.as_deref())
}

/// Makes Circles open at login, or not, as the setting says. A failure is only logged.
pub(crate) fn apply_login_item(app: &AppHandle, wanted: Option<bool>) {
    use tauri_plugin_autostart::ManagerExt;
    let wanted = wanted.unwrap_or(false);
    let launch = app.autolaunch();
    let is = launch.is_enabled().unwrap_or(false);
    if wanted == is {
        return;
    }
    let done = if wanted {
        launch.enable()
    } else {
        launch.disable()
    };
    if let Err(e) = done {
        tracing::warn!(error = %e, "cannot change the login item");
    }
}

/// Emits `settings:changed` when one of the settings files of the project changes.
#[tauri::command]
pub async fn settings_watch(
    app: AppHandle,
    state: State<'_, AppState>,
    project_id: Option<String>,
) -> IpcResult<()> {
    let files: Vec<PathBuf> = settings_layers(&state, project_id.as_deref())?
        .into_iter()
        .map(|(_, path)| path)
        .collect();
    state.settings_watcher.watch(&files, move || {
        let _ = app.emit("settings:changed", ());
    })
}

#[tauri::command]
pub async fn settings_unwatch(state: State<'_, AppState>) -> IpcResult<()> {
    state.settings_watcher.stop();
    Ok(())
}

/// Opens the settings file of a level in the text editor, making it first when it is not there.
#[tauri::command]
pub async fn settings_open_file(
    state: State<'_, AppState>,
    scope: String,
    project_id: Option<String>,
) -> IpcResult<String> {
    let scope = scope_from(&scope)?;
    let layers = settings_layers(&state, project_id.as_deref())?;
    let path = layers
        .iter()
        .find(|(layer, _)| *layer == scope)
        .map(|(_, path)| path.clone())
        .ok_or_else(|| AppError::Invalid("That level of settings needs a project.".into()))?;
    if !path.exists() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let starter = match scope {
            Scope::Project => settings::PROJECT_TEMPLATE,
            Scope::User => "# Circles settings for you. They apply to every project.\n",
            Scope::LocalProject => "# Circles settings for this project, only on this Mac.\n",
        };
        std::fs::write(&path, starter)?;
    }
    system::open_text(&path)?;
    Ok(path.display().to_string())
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct GitIdentity {
    pub name: Option<String>,
    pub email: Option<String>,
}

/// The name and email git signs commits with, for display.
#[tauri::command]
pub async fn git_identity(state: State<'_, AppState>) -> IpcResult<GitIdentity> {
    let git = crate::git::Git::new(state.env.get().await);
    let home = state.paths.home_dir.clone();
    Ok(GitIdentity {
        name: git.config_get(&home, "user.name").await.unwrap_or(None),
        email: git.config_get(&home, "user.email").await.unwrap_or(None),
    })
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ClaudeCheck {
    pub found: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    /// What to do when it is not found or does not run.
    pub problem: Option<String>,
}

/// Looks for Claude Code, the one chosen in the settings or the one on the PATH, and asks its
/// version.
#[tauri::command]
pub async fn claude_check(state: State<'_, AppState>) -> IpcResult<ClaudeCheck> {
    check_claude(&state).await
}

pub(crate) async fn check_claude(state: &AppState) -> IpcResult<ClaudeCheck> {
    let chosen = settings::load_merged(&settings_layers(state, None)?)?
        .settings
        .agents
        .claude_path;
    let env = state.env.get().await;
    let path = match chosen {
        Some(path) => Some(PathBuf::from(path)),
        None => env.which("claude"),
    };
    let Some(path) = path else {
        return Ok(ClaudeCheck {
            found: false,
            path: None,
            version: None,
            problem: Some(
                "Claude Code was not found. Install it from https://claude.com/claude-code, then run `claude` once to sign in."
                    .into(),
            ),
        });
    };
    let mut command = tokio::process::Command::new(&path);
    env.apply(&mut command);
    command
        .arg("--version")
        .stdin(Stdio::null())
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(10), command.output()).await;
    let shown = path.display().to_string();
    Ok(match output {
        Ok(Ok(out)) if out.status.success() => ClaudeCheck {
            found: true,
            path: Some(shown),
            version: String::from_utf8_lossy(&out.stdout)
                .lines()
                .next()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty()),
            problem: None,
        },
        _ => ClaudeCheck {
            found: false,
            path: Some(shown),
            version: None,
            problem: Some(
                "It was found but did not answer. Run `claude` in a terminal to see why.".into(),
            ),
        },
    })
}

#[tauri::command]
pub async fn sound_play() -> IpcResult<()> {
    system::play_sound()
}

#[tauri::command]
pub async fn notify_test() -> IpcResult<()> {
    system::notify("Circles", "This is how a notification looks.")
}

/// Shows the folder the app keeps its data in.
#[tauri::command]
pub async fn data_reveal(state: State<'_, AppState>) -> IpcResult<()> {
    system::reveal(&state.paths.data_dir)
}

/// Copies the logs into a new folder in Downloads and shows it.
#[tauri::command]
pub async fn logs_export(state: State<'_, AppState>) -> IpcResult<String> {
    let downloads = dirs::download_dir().unwrap_or_else(|| state.paths.home_dir.clone());
    let stamp = chrono_stamp();
    let folder = system::export_logs(&state.paths, &downloads, &stamp)?;
    let _ = system::reveal(&folder);
    Ok(folder.display().to_string())
}

fn chrono_stamp() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    seconds.to_string()
}

/// Starts the app over: ends every process, deletes what it stored and restarts. Needs the word
/// "reset".
#[tauri::command]
pub async fn app_reset_data(
    app: AppHandle,
    state: State<'_, AppState>,
    confirm: String,
) -> IpcResult<()> {
    if confirm.trim() != system::RESET_WORD {
        return Err(AppError::Invalid(format!(
            "Type \"{}\" to confirm.",
            system::RESET_WORD
        )));
    }
    state.kill_everything();
    system::reset_data(&state.paths, &confirm)?;
    app.restart()
}
