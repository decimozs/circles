//! The real tools: what each tool does in Circles, through the same services the app uses.

use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use super::dispatch::ToolRuntime;
use super::store::{Composer, HOME_PROJECT_ID};
use crate::commands::session::settings_of;
use crate::error::{AppError, AppResult};
use crate::git::Git;
use crate::github::pr::MergeMethod;
use crate::harness::from_settings;
use crate::harness::store::{NewSession, SessionStatus};
use crate::review::changes::{self, ChangeScope};
use crate::state::AppState;
use crate::workspace::model::CreateSource;
use crate::workspace::{CreateOptions, Workspace};

fn invalid(message: impl Into<String>) -> AppError {
    AppError::Invalid(message.into())
}

pub struct AppRuntime {
    pub app: AppHandle,
}

impl AppRuntime {
    fn state(&self) -> tauri::State<'_, AppState> {
        self.app.state::<AppState>()
    }

    /// A workspace a Composer may work on: never the hidden one a Composer lives in.
    async fn workspace(&self, id: &str) -> AppResult<Workspace> {
        let ws = self.state().workspace_service().await.get(id)?;
        if ws.project_id == HOME_PROJECT_ID {
            return Err(invalid("That is a Composer's own space, not a workspace."));
        }
        Ok(ws)
    }

    /// The workspace of a chat, refusing the chats of Composers themselves.
    async fn chat_workspace(&self, chat_id: &str) -> AppResult<Workspace> {
        let workspace_id = self.state().sessions.store().workspace_of(chat_id)?;
        self.workspace(&workspace_id).await
    }
}

fn workspace_json(w: &Workspace) -> Value {
    json!({
        "workspace_id": w.id,
        "project_id": w.project_id,
        "name": w.city,
        "title": w.title,
        "branch": w.branch,
        "base_branch": w.base_branch,
        "state": serde_json::to_value(w.state).unwrap_or(Value::Null),
        "error": w.error,
        "pr_number": w.pr_number,
        "pr_url": w.pr_url,
        "pinned": w.pinned,
    })
}

fn text_of(content: &Value) -> String {
    let text = content
        .get("text")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| content.to_string());
    if text.chars().count() > 1500 {
        format!("{}…", text.chars().take(1500).collect::<String>())
    } else {
        text
    }
}

fn str_arg<'a>(input: &'a Value, name: &str) -> AppResult<&'a str> {
    input[name]
        .as_str()
        .ok_or_else(|| invalid(format!("\"{name}\" is required.")))
}

#[async_trait]
impl ToolRuntime for AppRuntime {
    async fn project_of(&self, _tool: &str, input: &Value) -> Option<String> {
        if let Some(project) = input["project_id"].as_str() {
            return Some(project.to_string());
        }
        let state = self.state();
        if let Some(ws) = input["workspace_id"].as_str() {
            return state
                .workspace_service()
                .await
                .get(ws)
                .ok()
                .map(|w| w.project_id);
        }
        if let Some(chat) = input["chat_id"].as_str() {
            let ws = state.sessions.store().workspace_of(chat).ok()?;
            return state
                .workspace_service()
                .await
                .get(&ws)
                .ok()
                .map(|w| w.project_id);
        }
        None
    }

    async fn branch_pattern(&self, project_id: &str) -> Option<String> {
        self.state()
            .workspace_service()
            .await
            .branch_pattern_for_project(project_id)
    }

    async fn run(
        &self,
        composer: &Composer,
        _session_id: &str,
        tool: &str,
        input: Value,
    ) -> AppResult<Value> {
        let state = self.state();
        let allowed = |project: &str| composer.policy().allows_project(project);
        match tool {
            "list_projects" => {
                let projects = state.project_service().await.list()?;
                let workspaces = state.workspace_service().await;
                let listed: Vec<Value> = projects
                    .iter()
                    .filter(|p| allowed(&p.id))
                    .map(|p| {
                        // The rule for naming a branch in this project, and a name that follows it.
                        let rule = workspaces
                            .branch_pattern_for_project(&p.id)
                            .filter(|r| crate::tools::branch::validate_pattern(r).is_ok())
                            .unwrap_or_else(|| crate::tools::branch::DEFAULT_PATTERN.to_string());
                        json!({
                            "project_id": p.id,
                            "name": p.name,
                            "path": p.path,
                            "base_branch": p.default_branch,
                            "remote": p.remote_url,
                            "live_workspaces": p.workspace_count,
                            "branch_naming": {
                                "pattern": rule,
                                "example": crate::tools::branch::example(&rule),
                            },
                        })
                    })
                    .collect();
                Ok(json!({ "projects": listed }))
            }
            "list_pull_requests" => {
                let project = str_arg(&input, "project_id")?;
                let prs = state
                    .workspace_service()
                    .await
                    .list_pull_requests(project)
                    .await?;
                Ok(json!({ "pull_requests": prs.iter().map(|p| json!({
                    "number": p.number,
                    "title": p.title,
                    "branch": p.head_ref,
                    "author": p.author,
                    "from_a_fork": p.is_cross_repository,
                    "updated_at": p.updated_at,
                })).collect::<Vec<_>>() }))
            }
            "list_workspaces" => {
                let project = input["project_id"].as_str();
                let list = state.workspace_service().await.list(
                    project,
                    input["include_archived"].as_bool().unwrap_or(false),
                )?;
                Ok(
                    json!({ "workspaces": list.iter().filter(|w| allowed(&w.project_id)).map(workspace_json).collect::<Vec<_>>() }),
                )
            }
            "get_workspace" => {
                let ws = self.workspace(str_arg(&input, "workspace_id")?).await?;
                let chats = state.sessions.list_with(&ws.id, false)?;
                let mut value = workspace_json(&ws);
                value["chats"] = json!(chats
                    .iter()
                    .map(|c| json!({
                        "chat_id": c.id,
                        "title": c.title,
                        "status": serde_json::to_value(c.status).unwrap_or(Value::Null),
                        "model": c.model,
                        "turns": c.turns,
                        "cost_usd": c.total_cost_usd,
                        "last_error": c.last_error,
                    }))
                    .collect::<Vec<_>>());
                Ok(value)
            }
            "get_chat_tail" => {
                let chat = str_arg(&input, "chat_id")?;
                self.chat_workspace(chat).await?;
                let count = input["count"].as_u64().unwrap_or(10).clamp(1, 50) as usize;
                let session = state.sessions.get(chat)?;
                let messages = state.sessions.messages(chat, None)?;
                let from = messages.len().saturating_sub(count);
                Ok(json!({
                    "status": serde_json::to_value(session.status).unwrap_or(Value::Null),
                    "last_error": session.last_error,
                    "messages": messages[from..].iter().map(|m| json!({
                        "role": serde_json::to_value(m.role).unwrap_or(Value::Null),
                        "kind": serde_json::to_value(m.kind).unwrap_or(Value::Null),
                        "text": text_of(&m.content),
                    })).collect::<Vec<_>>(),
                }))
            }
            "get_changes" => {
                let ws = self.workspace(str_arg(&input, "workspace_id")?).await?;
                let service = state.workspace_service().await;
                let (ctx, _) = service.context_for(&ws.id)?;
                let scope = match input["scope"].as_str() {
                    Some("uncommitted") => ChangeScope::Uncommitted,
                    _ => ChangeScope::All,
                };
                let files = changes::changed_files(
                    &Git::new(service.env.clone()),
                    &ctx.cwd,
                    &ws.base_branch,
                    scope,
                )
                .await?;
                Ok(json!({ "files": serde_json::to_value(files).unwrap_or(Value::Null) }))
            }
            "get_pr_status" => {
                let ws = self.workspace(str_arg(&input, "workspace_id")?).await?;
                let (ctx, _) = state.workspace_service().await.context_for(&ws.id)?;
                let readiness = state.pr_service().await.readiness(&ws, &ctx.cwd).await?;
                serde_json::to_value(readiness).map_err(|e| AppError::Internal(e.to_string()))
            }
            "wait_for_chats" => {
                let ids: Vec<String> = input["chat_ids"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                for id in &ids {
                    self.chat_workspace(id).await?;
                }
                let seconds = input["timeout_seconds"]
                    .as_u64()
                    .unwrap_or(120)
                    .clamp(1, 600);
                let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
                loop {
                    let mut working = false;
                    let mut chats = Vec::new();
                    for id in &ids {
                        let s = state.sessions.get(id)?;
                        working |=
                            matches!(s.status, SessionStatus::Working | SessionStatus::Blocked);
                        chats.push(json!({
                            "chat_id": id,
                            "status": serde_json::to_value(s.status).unwrap_or(Value::Null),
                            "last_error": s.last_error,
                        }));
                    }
                    let timed_out = tokio::time::Instant::now() >= deadline;
                    if !working || timed_out {
                        return Ok(json!({ "chats": chats, "timed_out": working && timed_out }));
                    }
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
            "create_workspace" => {
                let project = str_arg(&input, "project_id")?;
                let source = if let Some(number) = input["pull_request"].as_u64() {
                    CreateSource::PullRequest {
                        number: number as u32,
                    }
                } else if let Some(name) = input["branch"].as_str() {
                    CreateSource::ExistingBranch {
                        name: name.to_string(),
                    }
                } else {
                    CreateSource::NewBranch {
                        base_branch: input["base_branch"].as_str().map(String::from),
                    }
                };
                let ws = state
                    .workspace_service()
                    .await
                    .create_with(
                        project,
                        CreateOptions {
                            source,
                            branch_name: input["branch_name"].as_str().map(String::from),
                            ..Default::default()
                        },
                    )
                    .await?;
                Ok(workspace_json(&ws))
            }
            "rename_branch" => {
                let ws = self.workspace(str_arg(&input, "workspace_id")?).await?;
                if ws.pr_number.is_some() {
                    return Err(invalid(
                        "This branch has a pull request, so renaming it would break that.",
                    ));
                }
                let service = state.workspace_service().await;
                let (ctx, _) = service.context_for(&ws.id)?;
                if Git::new(service.env.clone())
                    .remote_branch_exists(&ctx.cwd, "origin", &ws.branch)
                    .await
                    .unwrap_or(false)
                {
                    return Err(invalid(
                        "This branch is already pushed, so renaming it would break the remote.",
                    ));
                }
                let renamed = service
                    .rename_branch(&ws.id, str_arg(&input, "name")?)
                    .await?;
                Ok(workspace_json(&renamed))
            }
            "start_chat" => {
                let ws = self.workspace(str_arg(&input, "workspace_id")?).await?;
                let service = state.workspace_service().await;
                let (ctx, _) = service.context_for(&ws.id)?;
                // A merged branch cannot take another pull request, so follow-up work goes on a new
                // workspace. Starting there anyway has to be said.
                if ws.pr_number.is_some() && input["reuse_merged"].as_bool() != Some(true) {
                    let merged = state
                        .pr_service()
                        .await
                        .readiness(&ws, &ctx.cwd)
                        .await
                        .ok()
                        .and_then(|r| r.pr)
                        .is_some_and(|pr| pr.state == crate::github::pr::PrState::Merged);
                    if merged {
                        return Err(AppError::Invalid(format!(
                            "The pull request of {} was already merged, so this branch cannot take \
                             another one. Create a new workspace from the base branch for this work. \
                             If it needs files that exist only in this workspace, such as changes \
                             that were never committed, call start_chat again with `reuse_merged` set.",
                            ws.title.as_deref().unwrap_or(&ws.city)
                        )));
                    }
                }
                let settings = settings_of(&state, &ws.id).await;
                state
                    .sessions
                    .set_agent_settings(from_settings::agent_settings(&settings));
                let session = state.sessions.create(
                    &ws.id,
                    from_settings::new_session_defaults(
                        &settings,
                        NewSession {
                            title: None,
                            model: input["model"].as_str().map(String::from),
                            effort: None,
                            permission_mode: None,
                            fork_of: None,
                        },
                    ),
                )?;
                let task = str_arg(&input, "task")?;
                state.sessions.send(&ctx, &session.id, task).await?;
                // A chat a Composer starts is named like any other: its first words, then a
                // model's title.
                crate::smart::naming::name_new_chat(
                    &self.app,
                    &session.id,
                    &crate::smart::naming::first_words(task),
                    task,
                )
                .await;
                Ok(json!({ "chat_id": session.id, "workspace_id": ws.id }))
            }
            "send_message" => {
                let chat = str_arg(&input, "chat_id")?;
                let ws = self.chat_workspace(chat).await?;
                let (ctx, _) = state.workspace_service().await.context_for(&ws.id)?;
                state
                    .sessions
                    .send(&ctx, chat, str_arg(&input, "text")?)
                    .await?;
                Ok(json!({ "sent": true }))
            }
            "interrupt_chat" => {
                let chat = str_arg(&input, "chat_id")?;
                self.chat_workspace(chat).await?;
                state.sessions.interrupt(chat)?;
                Ok(json!({ "interrupted": true }))
            }
            "add_local_project" => {
                let p = state
                    .project_service()
                    .await
                    .add_local(str_arg(&input, "path")?)
                    .await?;
                Ok(json!({ "project_id": p.id, "name": p.name }))
            }
            "clone_github_project" => {
                let p = state
                    .project_service()
                    .await
                    .add_github(str_arg(&input, "repository")?)
                    .await?;
                Ok(json!({ "project_id": p.id, "name": p.name }))
            }
            "quick_start_project" => {
                let p = state
                    .project_service()
                    .await
                    .quick_start(str_arg(&input, "name")?)
                    .await?;
                Ok(json!({ "project_id": p.id, "name": p.name }))
            }
            "create_pr" => {
                let ws = self.workspace(str_arg(&input, "workspace_id")?).await?;
                let (ctx, _) = state.workspace_service().await.context_for(&ws.id)?;
                let pr = state
                    .pr_service()
                    .await
                    .create_pr(
                        &ws,
                        &ctx.cwd,
                        str_arg(&input, "title")?,
                        input["body"].as_str().unwrap_or(""),
                        input["draft"].as_bool().unwrap_or(false),
                    )
                    .await?;
                serde_json::to_value(pr).map_err(|e| AppError::Internal(e.to_string()))
            }
            "merge_pr" => {
                let ws = self.workspace(str_arg(&input, "workspace_id")?).await?;
                let service = state.workspace_service().await;
                let settings = service.settings_for(&ws.id)?;
                let (ctx, _) = service.context_for(&ws.id)?;
                let pr = state
                    .pr_service()
                    .await
                    .merge(
                        &ws,
                        &ctx.cwd,
                        MergeMethod::parse(settings.git.merge_method.as_deref()),
                        input["override_blockers"].as_bool().unwrap_or(false),
                    )
                    .await?;
                serde_json::to_value(pr).map_err(|e| AppError::Internal(e.to_string()))
            }
            "archive_workspace" => {
                let ws = self.workspace(str_arg(&input, "workspace_id")?).await?;
                let archived = state
                    .workspace_service()
                    .await
                    .archive(&ws.id, input["commit_dirty"].as_bool().unwrap_or(false))
                    .await?;
                state.sessions.stop_workspace(&ws.id).await;
                crate::commands::workspace::forget_checkpoints(&state, &ws.id).await;
                Ok(workspace_json(&archived))
            }
            "delete_workspace" => {
                let ws = self.workspace(str_arg(&input, "workspace_id")?).await?;
                state.sessions.stop_workspace(&ws.id).await;
                crate::commands::workspace::forget_checkpoints(&state, &ws.id).await;
                state
                    .workspace_service()
                    .await
                    .delete(&ws.id, input["delete_branch"].as_bool().unwrap_or(false))
                    .await?;
                Ok(json!({ "deleted": ws.id }))
            }
            "remove_project" => {
                let id = str_arg(&input, "project_id")?;
                if id == HOME_PROJECT_ID {
                    return Err(invalid("That is not a project."));
                }
                state.project_service().await.remove(id)?;
                Ok(json!({ "removed": id }))
            }
            other => Err(invalid(format!("\"{other}\" has no implementation."))),
        }
    }
}
