use serde::Serialize;
use tauri::State;
use ts_rs::TS;

use crate::commands::session::{refresh_agent, settings_of};
use crate::composer::store::{Approval, ChildRow, Composer, ComposerInput, Trigger, TriggerInput};
use crate::error::AppError;
use crate::harness::from_settings;
use crate::harness::store::{NewSession, PermissionMode, Session, SessionStatus};
use crate::ipc::IpcResult;
use crate::state::AppState;
use crate::tools::audit::AuditEntry;
use crate::tools::{catalog, Tier};
use crate::workspace::Workspace;

/// A tool, for the screen where a person sets what a Composer may do.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ComposerTool {
    pub name: String,
    pub description: String,
    pub tier: Tier,
    pub repeat_safe: bool,
}

#[tauri::command]
pub async fn tool_catalog() -> IpcResult<Vec<ComposerTool>> {
    Ok(catalog::all()
        .into_iter()
        .map(|t| ComposerTool {
            name: t.name.into(),
            description: t.description.into(),
            tier: t.tier,
            repeat_safe: t.repeat_safe,
        })
        .collect())
}

#[tauri::command]
pub async fn composer_list(state: State<'_, AppState>) -> IpcResult<Vec<Composer>> {
    state.composers.store.list()
}

#[tauri::command]
pub async fn composer_get(state: State<'_, AppState>, id: String) -> IpcResult<Composer> {
    state.composers.store.get(&id)
}

#[tauri::command]
pub async fn composer_create(
    state: State<'_, AppState>,
    mut input: ComposerInput,
) -> IpcResult<Composer> {
    if input.mode.is_none() {
        // Like a chat, a new Composer starts in the permission mode the settings name.
        input.mode = crate::commands::app::settings_layers(&state, None)
            .ok()
            .and_then(|layers| crate::settings::load_merged(&layers).ok())
            .and_then(|payload| payload.settings.agents.permission_mode)
            .filter(|m| crate::tools::policy::Mode::parse(m).is_some());
    }
    state.composers.store.create(&input)
}

#[tauri::command]
pub async fn composer_update(
    state: State<'_, AppState>,
    id: String,
    input: ComposerInput,
) -> IpcResult<Composer> {
    state.composers.store.update(&id, &input)
}

#[tauri::command]
pub async fn composer_pin(
    state: State<'_, AppState>,
    id: String,
    pinned: bool,
) -> IpcResult<Composer> {
    state.composers.store.set_pinned(&id, pinned)
}

/// Changes a Composer's mode. It applies to its running runs too, from their next tool call.
#[tauri::command]
pub async fn composer_set_mode(
    state: State<'_, AppState>,
    id: String,
    mode: String,
) -> IpcResult<Composer> {
    state.composers.store.set_mode(&id, &mode)
}

#[tauri::command]
pub async fn composer_set_notes(
    state: State<'_, AppState>,
    id: String,
    notes: String,
) -> IpcResult<Composer> {
    state.composers.store.set_notes(&id, &notes)
}

/// Deletes a Composer with its runs. Workspaces its runs started stay.
#[tauri::command]
pub async fn composer_delete(state: State<'_, AppState>, id: String) -> IpcResult<()> {
    let composer = state.composers.store.get(&id)?;
    state.sessions.stop_workspace(&composer.workspace_id).await;
    state.composers.store.delete(&id)
}

/// A chat of a Composer, with how it began and what waits on a person.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct RunView {
    pub session: Session,
    /// `person`, `schedule` or `trigger`.
    pub started_by: String,
    pub pending_approvals: u32,
}

#[tauri::command]
pub async fn composer_runs(
    state: State<'_, AppState>,
    composer_id: String,
    include_closed: Option<bool>,
) -> IpcResult<Vec<RunView>> {
    let composer = state.composers.store.get(&composer_id)?;
    let sessions = state
        .sessions
        .list_with(&composer.workspace_id, include_closed.unwrap_or(false))?;
    let pending = state
        .composers
        .store
        .approvals(Some(&composer_id), None, true, 500)?;
    sessions
        .into_iter()
        .map(|session| {
            let started_by = state
                .composers
                .store
                .run_info(&session.id)?
                .map(|r| r.started_by)
                .unwrap_or_else(|| "person".into());
            // A card that waits for an answer counts the same as an approval that waits.
            let pending_approvals = pending
                .iter()
                .filter(|a| a.session_id == session.id)
                .count() as u32
                + state.genui.store.pending_in(&session.id).unwrap_or(0) as u32;
            Ok(RunView {
                session,
                started_by,
                pending_approvals,
            })
        })
        .collect()
}

/// Starts a run: a new chat for the Composer, with the first message when there is one.
pub(crate) async fn start_run(
    state: &AppState,
    composer: &Composer,
    message: Option<&str>,
    started_by: &str,
    trigger_id: Option<&str>,
    title: Option<String>,
) -> Result<Session, AppError> {
    let service = state.workspace_service().await;
    let (ctx, _) = service.context_for(&composer.workspace_id)?;
    refresh_agent(state, &composer.workspace_id).await;
    let settings = settings_of(state, &composer.workspace_id).await;
    // A run with no name of its own gets a city, like a workspace does.
    let title = title.or_else(|| {
        let cities = crate::workspace::names::CITIES;
        Some(cities[rand::random_range(0..cities.len())].to_string())
    });
    let session = state.sessions.create(
        &composer.workspace_id,
        from_settings::new_session_defaults(
            &settings,
            NewSession {
                title,
                model: composer.model.clone(),
                effort: composer.effort.clone(),
                // The Composer has no built-in tools to ask about. Circles' tools ask for
                // themselves.
                permission_mode: Some(PermissionMode::Manual),
                fork_of: None,
            },
        ),
    )?;
    state
        .composers
        .store
        .record_run(&session.id, &composer.id, started_by, trigger_id)?;
    if let Some(text) = message.filter(|m| !m.trim().is_empty()) {
        state.sessions.send(&ctx, &session.id, text).await?;
    }
    state.sessions.get(&session.id)
}

#[tauri::command]
pub async fn composer_run_start(
    state: State<'_, AppState>,
    composer_id: String,
    message: Option<String>,
) -> IpcResult<Session> {
    let composer = state.composers.store.get(&composer_id)?;
    start_run(&state, &composer, message.as_deref(), "person", None, None).await
}

#[tauri::command]
pub async fn composer_approvals(
    state: State<'_, AppState>,
    composer_id: Option<String>,
    session_id: Option<String>,
    only_pending: Option<bool>,
) -> IpcResult<Vec<Approval>> {
    state.composers.store.approvals(
        composer_id.as_deref(),
        session_id.as_deref(),
        only_pending.unwrap_or(false),
        200,
    )
}

#[tauri::command]
pub async fn approval_answer(
    state: State<'_, AppState>,
    id: String,
    allow: bool,
    for_run: Option<bool>,
) -> IpcResult<()> {
    state
        .composers
        .dispatcher()?
        .answer(&id, allow, for_run.unwrap_or(false))
}

/// One thing a run started, with what it is now.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct FleetItem {
    pub child: ChildRow,
    pub workspace: Option<Workspace>,
    pub chat: Option<Session>,
}

#[tauri::command]
pub async fn composer_fleet(
    state: State<'_, AppState>,
    composer_id: String,
    session_id: Option<String>,
) -> IpcResult<Vec<FleetItem>> {
    let children = match &session_id {
        Some(run) => state.composers.store.children_of_run(run)?,
        None => state.composers.store.children_of_composer(&composer_id)?,
    };
    let service = state.workspace_service().await;
    Ok(children
        .into_iter()
        .map(|child| {
            let workspace = child
                .workspace_id
                .as_deref()
                .and_then(|id| service.get(id).ok());
            let chat = child
                .chat_id
                .as_deref()
                .and_then(|id| state.sessions.get(id).ok());
            FleetItem {
                child,
                workspace,
                chat,
            }
        })
        .collect())
}

/// Stops one child agent of a run.
#[tauri::command]
pub async fn composer_stop_child(state: State<'_, AppState>, chat_id: String) -> IpcResult<()> {
    state.sessions.interrupt(&chat_id).ok();
    state.sessions.stop(&chat_id).await
}

#[tauri::command]
pub async fn audit_list(
    state: State<'_, AppState>,
    composer_id: Option<String>,
    session_id: Option<String>,
    search: Option<String>,
    limit: Option<u32>,
) -> IpcResult<Vec<AuditEntry>> {
    state.composers.audit.list(
        composer_id.as_deref(),
        session_id.as_deref(),
        search.as_deref(),
        limit.unwrap_or(300).min(5000),
    )
}

/// Writes the audit log as a CSV file in the app folder and returns its path.
#[tauri::command]
pub async fn audit_export(
    state: State<'_, AppState>,
    composer_id: Option<String>,
) -> IpcResult<String> {
    let csv = state.composers.audit.export_csv(composer_id.as_deref())?;
    let dir = state.paths.data_dir.join("exports");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("composer-audit-{}.csv", crate::ipc::now_ms()));
    std::fs::write(&path, csv)?;
    Ok(path.display().to_string())
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ComposersStatus {
    pub paused: bool,
    /// Runs working or waiting right now.
    pub running: u32,
    pub waiting_approvals: u32,
}

fn status_of(state: &AppState) -> Result<ComposersStatus, AppError> {
    let dispatcher = state.composers.dispatcher()?;
    let mut running = 0;
    for composer in state.composers.store.list()? {
        running += state
            .sessions
            .list_with(&composer.workspace_id, false)?
            .iter()
            .filter(|s| matches!(s.status, SessionStatus::Working | SessionStatus::Blocked))
            .count() as u32;
    }
    Ok(ComposersStatus {
        paused: dispatcher.paused.load(std::sync::atomic::Ordering::SeqCst),
        running,
        waiting_approvals: state
            .composers
            .store
            .approvals(None, None, true, 1000)?
            .len() as u32,
    })
}

#[tauri::command]
pub async fn composers_status(state: State<'_, AppState>) -> IpcResult<ComposersStatus> {
    status_of(&state)
}

/// Pauses or resumes every Composer. While paused, every tool call is refused.
#[tauri::command]
pub async fn composers_pause(
    state: State<'_, AppState>,
    paused: bool,
) -> IpcResult<ComposersStatus> {
    state.composers.dispatcher()?.set_paused(paused);
    status_of(&state)
}

/// The kill switch: pauses everything, says no to what waits, and stops every Composer run and
/// every agent a Composer started.
pub(crate) async fn stop_everything(state: &AppState) -> Result<(), AppError> {
    state.composers.dispatcher()?.set_paused(true);
    for composer in state.composers.store.list()? {
        state.sessions.stop_workspace(&composer.workspace_id).await;
        for child in state.composers.store.children_of_composer(&composer.id)? {
            if let Some(chat) = child.chat_id {
                state.sessions.interrupt(&chat).ok();
                state.sessions.stop(&chat).await.ok();
            }
        }
    }
    state.composers.awake.set(false);
    Ok(())
}

#[tauri::command]
pub async fn composers_stop_all(state: State<'_, AppState>) -> IpcResult<ComposersStatus> {
    stop_everything(&state).await?;
    status_of(&state)
}

#[tauri::command]
pub async fn trigger_list(
    state: State<'_, AppState>,
    composer_id: Option<String>,
) -> IpcResult<Vec<Trigger>> {
    state.composers.store.triggers(composer_id.as_deref())
}

#[tauri::command]
pub async fn trigger_create(
    state: State<'_, AppState>,
    composer_id: String,
    input: TriggerInput,
) -> IpcResult<Trigger> {
    state.composers.store.create_trigger(&composer_id, &input)
}

#[tauri::command]
pub async fn trigger_update(
    state: State<'_, AppState>,
    id: String,
    input: TriggerInput,
) -> IpcResult<Trigger> {
    state.composers.store.update_trigger(&id, &input)
}

#[tauri::command]
pub async fn trigger_delete(state: State<'_, AppState>, id: String) -> IpcResult<()> {
    state.composers.store.delete_trigger(&id)
}

/// Composers shared in the repositories of your projects.
#[tauri::command]
pub async fn composer_shared_list(
    state: State<'_, AppState>,
) -> IpcResult<Vec<crate::composer::shared::SharedComposer>> {
    let projects = state.project_service().await.list()?;
    Ok(projects
        .iter()
        .filter(|p| p.path_exists)
        .flat_map(|p| crate::composer::shared::scan(&p.id, &p.name, std::path::Path::new(&p.path)))
        .collect())
}

/// Writes a Composer into a project's repository, without what it may do on its own.
#[tauri::command]
pub async fn composer_export(
    state: State<'_, AppState>,
    id: String,
    project_id: String,
) -> IpcResult<String> {
    let composer = state.composers.store.get(&id)?;
    let project = state.project_service().await.get(&project_id)?;
    let path = crate::composer::shared::export(&composer, std::path::Path::new(&project.path))?;
    Ok(path.display().to_string())
}

/// What a Composer made from a shared file starts with.
#[tauri::command]
pub async fn composer_shared_input(
    shared: crate::composer::shared::SharedComposer,
) -> IpcResult<ComposerInput> {
    Ok(crate::composer::shared::input_from(&shared))
}

/// Which workspaces a Composer started, for the badge on them.
#[tauri::command]
pub async fn composer_workspace_links(
    state: State<'_, AppState>,
) -> IpcResult<Vec<crate::composer::store::WorkspaceLink>> {
    state.composers.store.workspace_links()
}

/// The skills and commands of the user that a Composer's instructions can name with `/`.
#[tauri::command]
pub async fn composer_skills() -> IpcResult<Vec<crate::chat::SlashCommand>> {
    Ok(crate::chat::user_skills(dirs::home_dir().as_deref()))
}
