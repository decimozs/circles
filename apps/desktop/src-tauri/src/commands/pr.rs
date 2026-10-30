use serde::Serialize;
use tauri::State;
use ts_rs::TS;

use crate::error::AppError;
use crate::github::pr::{
    tail_chars, Check, CheckBucket, Deployment, MergeMethod, PrStatus, ReviewThread,
};
use crate::ipc::IpcResult;
use crate::pr::commits::{self, CommitInfo};
use crate::pr::docs;
use crate::pr::draft::{self, PrDraft};
use crate::pr::{prompts, Readiness};
use crate::review::changes::base_ref;
use crate::state::AppState;

/// What the pull request dialog starts from.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct PrDraftSeed {
    pub title: String,
    pub body: String,
    pub base: String,
}

/// The workspace and its folder, as the commands below need them.
async fn workspace_and_folder(
    state: &AppState,
    workspace_id: &str,
) -> Result<(crate::workspace::Workspace, std::path::PathBuf), AppError> {
    let service = state.workspace_service().await;
    let (ctx, _) = service.context_for(workspace_id)?;
    Ok((service.get(workspace_id)?, ctx.cwd))
}

#[tauri::command]
pub async fn pr_readiness(
    state: State<'_, AppState>,
    workspace_id: String,
) -> IpcResult<Readiness> {
    let (ws, cwd) = workspace_and_folder(&state, &workspace_id).await?;
    state.pr_service().await.readiness(&ws, &cwd).await
}

#[tauri::command]
pub async fn pr_status(
    state: State<'_, AppState>,
    workspace_id: String,
) -> IpcResult<Option<PrStatus>> {
    let (ws, cwd) = workspace_and_folder(&state, &workspace_id).await?;
    Ok(state
        .pr_service()
        .await
        .gh
        .pr_for_branch(&cwd, &ws.branch)
        .await?)
}

#[tauri::command]
pub async fn pr_checks(state: State<'_, AppState>, workspace_id: String) -> IpcResult<Vec<Check>> {
    let (ws, cwd) = workspace_and_folder(&state, &workspace_id).await?;
    let service = state.pr_service().await;
    match service.gh.pr_for_branch(&cwd, &ws.branch).await? {
        Some(pr) => Ok(service.gh.pr_checks(&cwd, pr.number).await?),
        None => Ok(Vec::new()),
    }
}

#[tauri::command]
pub async fn pr_threads(
    state: State<'_, AppState>,
    workspace_id: String,
) -> IpcResult<Vec<ReviewThread>> {
    let (ws, cwd) = workspace_and_folder(&state, &workspace_id).await?;
    let service = state.pr_service().await;
    match service.gh.pr_for_branch(&cwd, &ws.branch).await? {
        Some(pr) => Ok(service.gh.review_threads(&cwd, pr.number).await?),
        None => Ok(Vec::new()),
    }
}

#[tauri::command]
pub async fn pr_thread_resolve(
    state: State<'_, AppState>,
    workspace_id: String,
    thread_id: String,
    resolved: bool,
) -> IpcResult<()> {
    let (_, cwd) = workspace_and_folder(&state, &workspace_id).await?;
    Ok(state
        .pr_service()
        .await
        .gh
        .set_thread_resolved(&cwd, &thread_id, resolved)
        .await?)
}

#[tauri::command]
pub async fn pr_deployments(
    state: State<'_, AppState>,
    workspace_id: String,
) -> IpcResult<Vec<Deployment>> {
    let (ws, cwd) = workspace_and_folder(&state, &workspace_id).await?;
    Ok(state
        .pr_service()
        .await
        .gh
        .deployments(&cwd, &ws.branch)
        .await?)
}

/// The commits and the diff of the branch, which is what a title and description are written from.
async fn draft_inputs(
    state: &AppState,
    workspace_id: &str,
) -> Result<
    (
        crate::workspace::Workspace,
        std::path::PathBuf,
        Vec<String>,
        String,
    ),
    AppError,
> {
    let (ws, cwd) = workspace_and_folder(state, workspace_id).await?;
    let pr = state.pr_service().await;
    let base = base_ref(&pr.git, &cwd, &ws.base_branch).await;
    let range = format!("{base}..HEAD");
    let commits: Vec<String> = pr
        .git
        .log(&cwd, &range, 30)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|c| c.subject)
        .collect();
    let changes = match crate::review::changes::base_rev(
        &pr.git,
        &cwd,
        &ws.base_branch,
        crate::review::changes::ChangeScope::All,
    )
    .await
    {
        Ok(rev) => pr
            .git
            .diff_patch(&cwd, &rev, Some("HEAD"), None)
            .await
            .unwrap_or_default(),
        Err(_) => String::new(),
    };
    Ok((ws, cwd, commits, changes))
}

/// A title and description from the commits, to start the dialog with.
#[tauri::command]
pub async fn pr_draft_seed(
    state: State<'_, AppState>,
    workspace_id: String,
) -> IpcResult<PrDraftSeed> {
    let (ws, _, commits, _) = draft_inputs(&state, &workspace_id).await?;
    Ok(PrDraftSeed {
        title: prompts::fallback_title(&ws.branch, &commits),
        body: prompts::fallback_body(&commits),
        base: ws.base_branch,
    })
}

/// Has Claude write the title and description from the committed changes. No chat is made.
#[tauri::command]
pub async fn pr_draft(
    state: State<'_, AppState>,
    workspace_id: String,
    model: Option<String>,
) -> IpcResult<PrDraft> {
    let (ws, cwd, commits, changes) = draft_inputs(&state, &workspace_id).await?;
    let settings = state
        .workspace_service()
        .await
        .settings_for(&workspace_id)?;
    let docs = docs::read(&cwd);
    let prompt = prompts::draft_pr(
        &ws.base_branch,
        &commits,
        &changes,
        &settings.instructions,
        &docs,
    );
    let env = state
        .env
        .get()
        .await
        .with_overrides(settings.environment_variables.clone());
    let model = model
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| "sonnet".into());
    let answer = draft::run_once(&env, &cwd, &prompt, &model).await?;
    draft::parse_draft(&answer).ok_or_else(|| {
        AppError::Invalid("The agent did not answer in the expected form. Write it by hand.".into())
    })
}

#[tauri::command]
pub async fn pr_create(
    state: State<'_, AppState>,
    workspace_id: String,
    title: String,
    body: String,
    draft: Option<bool>,
) -> IpcResult<PrStatus> {
    let (ws, cwd) = workspace_and_folder(&state, &workspace_id).await?;
    state
        .pr_service()
        .await
        .create_pr(&ws, &cwd, &title, &body, draft.unwrap_or(false))
        .await
}

#[tauri::command]
pub async fn pr_merge(
    state: State<'_, AppState>,
    workspace_id: String,
    override_blockers: Option<bool>,
) -> IpcResult<PrStatus> {
    let service = state.workspace_service().await;
    let settings = service.settings_for(&workspace_id)?;
    let (ws, cwd) = workspace_and_folder(&state, &workspace_id).await?;
    state
        .pr_service()
        .await
        .merge(
            &ws,
            &cwd,
            MergeMethod::parse(settings.git.merge_method.as_deref()),
            override_blockers.unwrap_or(false),
        )
        .await
}

/// Sends a task to a chat as a message.
async fn send_task(
    state: &AppState,
    workspace_id: &str,
    session_id: &str,
    text: &str,
) -> Result<(), AppError> {
    let (ctx, _) = state.workspace_service().await.context_for(workspace_id)?;
    crate::commands::session::refresh_agent(state, workspace_id).await;
    state.sessions.send(&ctx, session_id, text).await?;
    Ok(())
}

/// Hands the logs of the failing checks to the agent and asks it to fix them.
#[tauri::command]
pub async fn agent_fix_checks(
    state: State<'_, AppState>,
    workspace_id: String,
    session_id: String,
) -> IpcResult<()> {
    let (ws, cwd) = workspace_and_folder(&state, &workspace_id).await?;
    let settings = state
        .workspace_service()
        .await
        .settings_for(&workspace_id)?;
    let service = state.pr_service().await;
    let pr = service
        .gh
        .pr_for_branch(&cwd, &ws.branch)
        .await?
        .ok_or_else(|| AppError::Invalid("There is no pull request yet.".into()))?;
    let failing: Vec<Check> = service
        .gh
        .pr_checks(&cwd, pr.number)
        .await?
        .into_iter()
        .filter(|c| c.bucket == CheckBucket::Fail)
        .collect();
    if failing.is_empty() {
        return Err(AppError::Invalid("No check is failing.".into()));
    }
    let mut with_logs = Vec::new();
    let mut fetched: std::collections::HashMap<u64, Option<String>> =
        std::collections::HashMap::new();
    for check in failing {
        let log = match check.run_id {
            Some(run) => match fetched.get(&run) {
                Some(known) => known.clone(),
                None => {
                    let log = service
                        .gh
                        .failed_log(&cwd, run)
                        .await
                        .ok()
                        .map(|text| tail_chars(&text, prompts::LOG_LIMIT));
                    fetched.insert(run, log.clone());
                    log
                }
            },
            None => None,
        };
        with_logs.push((check, log));
    }
    send_task(
        &state,
        &workspace_id,
        &session_id,
        &prompts::fix_checks(pr.number, &with_logs, &settings.instructions),
    )
    .await
}

/// Hands the unresolved review threads to the agent.
#[tauri::command]
pub async fn agent_respond_review(
    state: State<'_, AppState>,
    workspace_id: String,
    session_id: String,
) -> IpcResult<()> {
    let (ws, cwd) = workspace_and_folder(&state, &workspace_id).await?;
    let settings = state
        .workspace_service()
        .await
        .settings_for(&workspace_id)?;
    let service = state.pr_service().await;
    let pr = service
        .gh
        .pr_for_branch(&cwd, &ws.branch)
        .await?
        .ok_or_else(|| AppError::Invalid("There is no pull request yet.".into()))?;
    let threads: Vec<ReviewThread> = service
        .gh
        .review_threads(&cwd, pr.number)
        .await?
        .into_iter()
        .filter(|t| !t.resolved)
        .collect();
    if threads.is_empty() {
        return Err(AppError::Invalid(
            "No review thread is waiting for an answer.".into(),
        ));
    }
    send_task(
        &state,
        &workspace_id,
        &session_id,
        &prompts::respond_to_review(pr.number, &threads, &settings.instructions),
    )
    .await
}

/// Asks the agent to bring in the base branch and settle the conflicts.
#[tauri::command]
pub async fn agent_resolve_conflicts(
    state: State<'_, AppState>,
    workspace_id: String,
    session_id: String,
) -> IpcResult<()> {
    let (ws, _) = workspace_and_folder(&state, &workspace_id).await?;
    let settings = state
        .workspace_service()
        .await
        .settings_for(&workspace_id)?;
    send_task(
        &state,
        &workspace_id,
        &session_id,
        &prompts::resolve_conflicts(&ws.base_branch, &settings.instructions),
    )
    .await
}

/// Has Claude group the uncommitted changes into atomic commits, then makes them. Claude only
/// proposes the grouping, git is run here.
#[tauri::command]
pub async fn commit_with_claude(
    state: State<'_, AppState>,
    workspace_id: String,
    model: Option<String>,
) -> IpcResult<Vec<CommitInfo>> {
    let (_, cwd) = workspace_and_folder(&state, &workspace_id).await?;
    let settings = state
        .workspace_service()
        .await
        .settings_for(&workspace_id)?;
    let service = state.pr_service().await;
    let pending = commits::gather(&service.git, &cwd).await?;
    if pending.files.is_empty() {
        return Err(AppError::Invalid("There is nothing to commit.".into()));
    }
    let docs = docs::read(&cwd);
    let prompt = prompts::commit_plan(&pending.files, &pending.diff, &settings.instructions, &docs);
    let env = state
        .env
        .get()
        .await
        .with_overrides(settings.environment_variables.clone());
    let model = model
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| "sonnet".into());
    let answer = draft::run_once(&env, &cwd, &prompt, &model).await?;
    let plan = commits::parse_plan(&answer).ok_or_else(|| {
        AppError::Invalid(
            "The agent did not answer in the expected form. Commit everything at once instead."
                .into(),
        )
    })?;
    let paths: Vec<String> = pending.files.iter().map(|(p, _)| p.clone()).collect();
    let plan = commits::normalize(plan, &paths);
    commits::apply(&service.git, &cwd, &plan, &pending.origins).await
}

/// Commits every change in one commit with the given message.
#[tauri::command]
pub async fn commit_everything(
    state: State<'_, AppState>,
    workspace_id: String,
    message: String,
) -> IpcResult<Vec<CommitInfo>> {
    let message = prompts::without_attribution(&message);
    if message.is_empty() {
        return Err(AppError::Invalid("Write a commit message first.".into()));
    }
    let (_, cwd) = workspace_and_folder(&state, &workspace_id).await?;
    let service = state.pr_service().await;
    let pending = commits::gather(&service.git, &cwd).await?;
    if pending.files.is_empty() {
        return Err(AppError::Invalid("There is nothing to commit.".into()));
    }
    service.git.commit_all(&cwd, &message).await?;
    let sha = service
        .git
        .rev_parse(&cwd, "HEAD")
        .await
        .unwrap_or_default();
    Ok(vec![CommitInfo {
        sha: sha.chars().take(7).collect(),
        message,
        files: pending.files.into_iter().map(|(p, _)| p).collect(),
    }])
}
