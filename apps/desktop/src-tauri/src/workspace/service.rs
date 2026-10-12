//! Workspace lifecycle: create, archive, restore, rename, delete, sections, and the startup
//! reconcile. A workspace is a git worktree on its own branch. Circles never deletes a branch
//! or uncommitted work unless the user explicitly asks.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use rusqlite::{params, OptionalExtension};

use super::copy;
use super::events::{EventSink, Locks};
use super::model::*;
use super::names;
use super::state::{can_transition, WorkspaceState};
use crate::env::ResolvedEnv;
use crate::error::{AppError, AppResult};
use crate::git::{GitError, GitErrorKind};
use crate::github::PrInfo;
use crate::harness::sessions::AgentState;
use crate::ipc::{new_id, now_ms};
use crate::paths::project_settings_file;
use crate::ports::{self, SystemProbe};
use crate::project::{Project, ProjectService};
use crate::pty::ExitInfo;
use crate::scripts::{self, ScriptsInfo};
use crate::settings::{self, Scope, Settings};
use crate::terminal::{Context, Terminals};

const GH_LOGIN_KEY: &str = "github_login";
const GH_IDENTITY_KEY: &str = "github_identity";
const GH_LOGIN_TIMEOUT: Duration = Duration::from_secs(8);
const CONTEXT_DIR: &str = ".context";

/// The archive script may run this long before it is ended and the archive goes on.
const ARCHIVE_SCRIPT_LIMIT: Duration = Duration::from_secs(120);

#[derive(Clone)]
pub struct WorkspaceService {
    pub projects: ProjectService,
    pub sink: Arc<dyn EventSink>,
    pub locks: Arc<Locks>,
    pub terminals: Arc<Terminals>,
    /// Login environment for scripts and terminals.
    pub env: Arc<ResolvedEnv>,
}

fn invalid(message: impl Into<String>) -> AppError {
    AppError::Invalid(message.into())
}

/// Tracks what a failed creation has already done, so it can be undone.
struct Progress {
    worktree_added: bool,
    /// The branch was created by this creation, so a rollback may delete it. Branches that
    /// already existed are never deleted.
    created_branch: bool,
    branch: String,
}

/// How a creation obtains its branch.
enum Checkout {
    /// A new branch off `base`, started from origin when possible.
    New { base: String },
    /// An existing local branch.
    Local,
    /// A local branch created from `origin/<branch>` that tracks it.
    Remote,
    /// A pull request from a fork, fetched into a local branch.
    Fork { number: u32 },
}

struct Plan {
    checkout: Checkout,
    /// The branch the diff is measured against.
    base: String,
    branch: String,
    title: Option<String>,
    /// Event line for the chat, for checkouts that do not start from a base.
    intro: String,
}

impl WorkspaceService {
    // Reads

    pub fn list(
        &self,
        project_id: Option<&str>,
        include_archived: bool,
    ) -> AppResult<Vec<Workspace>> {
        self.projects.db.read(|c| {
            let sql = format!(
                "SELECT {WORKSPACE_COLUMNS} FROM workspaces
                 WHERE (?1 IS NULL OR project_id = ?1) AND (?2 OR state != 'archived')
                   AND project_id != 'composers'
                 ORDER BY created_at DESC, rowid DESC"
            );
            let mut stmt = c.prepare(&sql)?;
            let rows = stmt.query_map(params![project_id, include_archived], workspace_from_row)?;
            rows.collect()
        })
    }

    /// Where each workspace's branch stands: its pull request, and how many commits it has. One
    /// `gh` call per project, so a sidebar of many workspaces stays cheap. A project that is not
    /// on GitHub, or where `gh` cannot answer, still gets its commit counts.
    pub async fn branch_states(&self) -> AppResult<Vec<BranchState>> {
        let workspaces = self.list(None, false)?;
        let mut by_project: std::collections::BTreeMap<String, Vec<Workspace>> = Default::default();
        for ws in workspaces {
            if ws.state.has_worktree() {
                by_project
                    .entry(ws.project_id.clone())
                    .or_default()
                    .push(ws);
            }
        }
        let mut out = Vec::new();
        for (project_id, list) in by_project {
            let Ok(project) = self.projects.get(&project_id) else {
                continue;
            };
            let repo = self.repo_path(&project);
            let on_github = matches!(
                self.projects.git.remote_url(&repo, "origin").await,
                Ok(Some(url)) if url.contains("github.com")
            );
            let prs = if on_github {
                match tokio::time::timeout(GH_LOGIN_TIMEOUT, self.projects.gh.pr_states(&repo, 100))
                    .await
                {
                    Ok(Ok(prs)) => prs,
                    _ => Vec::new(),
                }
            } else {
                Vec::new()
            };
            for ws in list {
                // The newest pull request of a branch stands for it.
                let pr = prs.iter().find(|p| p.head_ref == ws.branch);
                let ahead = if Path::new(&ws.path).is_dir() {
                    self.projects
                        .git
                        .ahead_behind(Path::new(&ws.path), &ws.base_branch, &ws.branch)
                        .await
                        .map(|(ahead, _)| ahead)
                        .unwrap_or(0)
                } else {
                    0
                };
                out.push(BranchState {
                    workspace_id: ws.id,
                    pr_number: pr.map(|p| p.number),
                    pr_state: pr.map(|p| p.state),
                    is_draft: pr.is_some_and(|p| p.is_draft),
                    ahead,
                });
            }
        }
        Ok(out)
    }

    pub fn get(&self, id: &str) -> AppResult<Workspace> {
        self.projects
            .db
            .read(|c| {
                c.query_row(
                    &format!("SELECT {WORKSPACE_COLUMNS} FROM workspaces WHERE id = ?1"),
                    params![id],
                    workspace_from_row,
                )
            })
            .map_err(|e| match e {
                AppError::Db(rusqlite::Error::QueryReturnedNoRows) => {
                    AppError::NotFound(format!("workspace {id}"))
                }
                other => other,
            })
    }

    pub fn events(&self, workspace_id: &str) -> AppResult<Vec<WorkspaceEvent>> {
        self.projects.db.read(|c| {
            let mut stmt = c.prepare(
                "SELECT id, workspace_id, kind, message, created_at FROM workspace_events
                 WHERE workspace_id = ?1 ORDER BY rowid",
            )?;
            let rows = stmt.query_map(params![workspace_id], |r| {
                Ok(WorkspaceEvent {
                    id: r.get(0)?,
                    workspace_id: r.get(1)?,
                    kind: r.get(2)?,
                    message: r.get(3)?,
                    created_at: r.get(4)?,
                })
            })?;
            rows.collect()
        })
    }

    /// Moves the workspace between working, waiting and idle as its chats do. States where an agent
    /// has no business, such as setting up or archived, are left alone.
    pub fn apply_agent_state(&self, id: &str, agent: AgentState) -> AppResult<()> {
        let current = self.get(id)?.state;
        let target = match (agent, current) {
            (
                AgentState::Working,
                WorkspaceState::Ready
                | WorkspaceState::Idle
                | WorkspaceState::Blocked
                | WorkspaceState::Working,
            ) => WorkspaceState::Working,
            (
                AgentState::Blocked,
                WorkspaceState::Ready
                | WorkspaceState::Idle
                | WorkspaceState::Working
                | WorkspaceState::Blocked,
            ) => WorkspaceState::Blocked,
            (AgentState::Idle, WorkspaceState::Working | WorkspaceState::Blocked) => {
                WorkspaceState::Idle
            }
            _ => return Ok(()),
        };
        self.set_state(id, target, None)?;
        Ok(())
    }

    /// The message typed in the Create dialog, with the model and effort chosen there. Returned
    /// once: the chat sends it and it is cleared.
    pub fn take_pending_prompt(&self, id: &str) -> AppResult<Option<PendingPrompt>> {
        let ws = self.get(id)?;
        let Some(prompt) = ws.pending_prompt else {
            return Ok(None);
        };
        self.projects.db.write(|c| {
            c.execute(
                "UPDATE workspaces SET pending_prompt = NULL, pending_model = NULL, pending_effort = NULL WHERE id = ?1",
                params![id],
            )
        })?;
        Ok(Some(PendingPrompt {
            prompt,
            model: ws.pending_model,
            effort: ws.pending_effort,
        }))
    }

    pub fn touch_opened(&self, id: &str) -> AppResult<()> {
        let n = self.projects.db.write(|c| {
            c.execute(
                "UPDATE workspaces SET last_opened_at = ?1 WHERE id = ?2",
                params![now_ms(), id],
            )
        })?;
        if n == 0 {
            Err(AppError::NotFound(format!("workspace {id}")))
        } else {
            Ok(())
        }
    }

    // Internals

    fn add_event(&self, workspace_id: &str, kind: &str, message: &str) {
        let result = self.projects.db.write(|c| {
            c.execute(
                "INSERT INTO workspace_events (id, workspace_id, kind, message, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![new_id(), workspace_id, kind, message, now_ms()],
            )
        });
        if let Err(e) = result {
            tracing::warn!(error = %e, "cannot record workspace event");
        }
    }

    /// The single place a workspace changes state. Validates the move, stores it, announces it.
    fn set_state(&self, id: &str, to: WorkspaceState, error: Option<&str>) -> AppResult<Workspace> {
        let current = self.get(id)?;
        if current.state == to {
            return Ok(current);
        }
        if !can_transition(current.state, to) {
            return Err(invalid(format!(
                "A workspace cannot move from {} to {}.",
                current.state.as_str(),
                to.as_str()
            )));
        }
        let now = now_ms();
        self.projects.db.write(|c| {
            c.execute(
                "UPDATE workspaces SET state = ?1, error = ?2, updated_at = ?3,
                   archived_at = CASE WHEN ?1 = 'archived' THEN ?3 ELSE NULL END
                 WHERE id = ?4",
                params![to.as_str(), error, now, id],
            )
        })?;
        self.sink.state_changed(&StateChange {
            workspace_id: id.to_string(),
            project_id: current.project_id.clone(),
            from: Some(current.state),
            to,
            error: error.map(str::to_string),
        });
        self.get(id)
    }

    fn kv_get(&self, key: &str) -> Option<String> {
        self.projects
            .db
            .read(|c| {
                c.query_row("SELECT value FROM kv WHERE key = ?1", params![key], |r| {
                    r.get(0)
                })
                .optional()
            })
            .ok()
            .flatten()
    }

    fn kv_set(&self, key: &str, value: &str) {
        let _ = self.projects.db.write(|c| {
            c.execute(
                "INSERT INTO kv (key, value, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                params![key, value, now_ms()],
            )
        });
    }

    /// Makes commits in a repository that Circles cloned under the GitHub account that owns it.
    ///
    /// Git picks the author from `~/.gitconfig`, and a person with a work and a personal account
    /// switches between them by folder. A clone in Circles' own folder matches neither, so it got
    /// the default one, whichever account owned the repository. The identity is written to the
    /// repository's own config, which every workspace of it shares. A repository the person
    /// added from their own folder, or one that already sets an identity, is left alone.
    async fn ensure_identity(&self, repo: &Path) {
        if !repo.starts_with(&self.projects.paths.repos_dir) {
            return;
        }
        let git = &self.projects.git;
        if matches!(git.config_get_local(repo, "user.email").await, Ok(Some(_))) {
            return;
        }
        let on_github = matches!(
            git.remote_url(repo, "origin").await,
            Ok(Some(url)) if url.contains("github.com")
        );
        if !on_github {
            return;
        }
        let key = format!("{GH_IDENTITY_KEY}:{}", repo.display());
        let account = match self
            .kv_get(&key)
            .and_then(|v| crate::github::parse_account(&v))
        {
            Some(account) => account,
            None => {
                match tokio::time::timeout(GH_LOGIN_TIMEOUT, self.projects.gh.account_in(repo))
                    .await
                {
                    Ok(Ok(account)) => {
                        self.kv_set(&key, &format!("{}\t{}", account.0, account.1));
                        account
                    }
                    _ => return,
                }
            }
        };
        let (login, id) = account;
        let _ = git.config_set_local(repo, "user.name", &login).await;
        let _ = git
            .config_set_local(
                repo,
                "user.email",
                &crate::github::noreply_email(&login, id),
            )
            .await;
    }

    /// The prefix chosen in the settings, if there is one, else the usual one.
    async fn branch_prefix_for(&self, project: &Project, repo: &Path) -> String {
        let chosen = self
            .load_settings(project)
            .ok()
            .and_then(|s| s.git.branch_prefix)
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty());
        match chosen {
            Some(prefix) => names::branch_prefix(&prefix),
            None => self.branch_prefix(repo).await,
        }
    }

    /// Prefix for generated branch names: the GitHub login (cached), else the git user name.
    async fn branch_prefix(&self, repo: &Path) -> String {
        // A repository on GitHub is worked on as the account that can see it, which with a work
        // and a personal account is not always the active one. Each repository remembers its own.
        let on_github = self
            .projects
            .git
            .remote_url(repo, "origin")
            .await
            .ok()
            .flatten()
            .is_some_and(|url| url.contains("github.com"));
        if on_github {
            let key = format!("{GH_LOGIN_KEY}:{}", repo.display());
            if let Some(login) = self.kv_get(&key) {
                return names::branch_prefix(&login);
            }
            if let Ok(Ok(login)) =
                tokio::time::timeout(GH_LOGIN_TIMEOUT, self.projects.gh.current_user_in(repo)).await
            {
                if !login.is_empty() {
                    self.kv_set(&key, &login);
                    return names::branch_prefix(&login);
                }
            }
        }
        if let Some(login) = self.kv_get(GH_LOGIN_KEY) {
            return names::branch_prefix(&login);
        }
        match tokio::time::timeout(GH_LOGIN_TIMEOUT, self.projects.gh.current_user()).await {
            Ok(Ok(login)) if !login.is_empty() => {
                self.kv_set(GH_LOGIN_KEY, &login);
                return names::branch_prefix(&login);
            }
            Ok(Err(e)) => tracing::debug!(error = %e, "gh login unavailable for branch prefix"),
            _ => {}
        }
        match self.projects.git.config_get(repo, "user.name").await {
            Ok(Some(name)) => names::branch_prefix(&name),
            _ => names::branch_prefix(""),
        }
    }

    async fn fresh_branch_name(&self, repo: &Path, prefix: &str) -> AppResult<String> {
        for _ in 0..10 {
            let suffix = names::random_suffix(&mut |n| rand::random_range(0..n));
            let name = format!("{prefix}/{suffix}");
            if !self.projects.git.branch_exists(repo, &name).await? {
                return Ok(name);
            }
        }
        Err(AppError::Internal(
            "could not find an unused branch name".into(),
        ))
    }

    /// The merged settings that apply to a workspace.
    pub fn settings_for(&self, workspace_id: &str) -> AppResult<Settings> {
        let ws = self.get(workspace_id)?;
        let project = self.projects.get(&ws.project_id)?;
        self.load_settings(&project)
    }

    /// The branch naming rule a project sets, if it sets one.
    pub fn branch_pattern_for_project(&self, project_id: &str) -> Option<String> {
        let project = self.projects.get(project_id).ok()?;
        let settings = self.load_settings(&project).ok()?;
        settings
            .git
            .branch_pattern
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
    }

    fn load_settings(&self, project: &Project) -> AppResult<Settings> {
        let paths = &self.projects.paths;
        let layers = vec![
            (Scope::User, paths.settings_file.clone()),
            (
                Scope::Project,
                project_settings_file(Path::new(&project.path)),
            ),
            (
                Scope::LocalProject,
                paths.local_project_settings(&project.id),
            ),
        ];
        Ok(settings::load_merged(&layers)?.settings)
    }

    fn copy_patterns(&self, project: &Project, settings: &Settings) -> Vec<String> {
        let mut patterns = settings.files_to_copy.clone();
        if let Ok(text) = std::fs::read_to_string(Path::new(&project.path).join(".worktreeinclude"))
        {
            patterns.extend(copy::parse_include_file(&text));
        }
        patterns
    }

    /// Copies the gitignored files that match the patterns. Returns how many files were copied.
    async fn copy_local_files(
        &self,
        project: &Project,
        settings: &Settings,
        dest: &Path,
    ) -> AppResult<u32> {
        let patterns = self.copy_patterns(project, settings);
        if patterns.is_empty() {
            return Ok(0);
        }
        let repo = Path::new(&project.path);
        let entries = self.projects.git.ignored_entries(repo).await?;
        let selected = copy::select_entries(repo, &entries, &patterns)?;
        copy::copy_entries(repo, dest, &selected)
    }

    /// Creates `.context/` and keeps it out of git without touching the repository's `.gitignore`.
    async fn prepare_context_dir(&self, worktree: &Path) -> AppResult<()> {
        std::fs::create_dir_all(worktree.join(CONTEXT_DIR))?;
        let exclude = self.projects.git.info_exclude_path(worktree).await?;
        ensure_line(&exclude, &format!("/{CONTEXT_DIR}/"))?;
        Ok(())
    }

    fn context_backup_dir(&self, workspace_id: &str) -> PathBuf {
        self.projects
            .paths
            .data_dir
            .join("context")
            .join(workspace_id)
    }

    /// A path Circles created and may delete: inside `~/circles/workspaces`, no `..`.
    fn is_managed(&self, path: &Path) -> bool {
        path.starts_with(&self.projects.paths.workspaces_dir)
            && path != self.projects.paths.workspaces_dir
            && !path.components().any(|c| c == Component::ParentDir)
    }

    fn remove_managed_dir(&self, path: &Path) {
        if self.is_managed(path) && path.exists() {
            if let Err(e) = std::fs::remove_dir_all(path) {
                tracing::warn!(path = %path.display(), error = %e, "cannot remove workspace folder");
            }
        }
    }

    fn repo_path(&self, project: &Project) -> PathBuf {
        PathBuf::from(&project.path)
    }

    // Create

    /// Creates a workspace with a new branch off the project's base (or `base_override`).
    pub async fn create(
        &self,
        project_id: &str,
        base_override: Option<&str>,
    ) -> AppResult<Workspace> {
        self.create_with(
            project_id,
            CreateOptions {
                source: CreateSource::NewBranch {
                    base_branch: base_override.map(str::to_string),
                },
                ..Default::default()
            },
        )
        .await
    }

    /// Creates a workspace: a worktree on a new, existing or pull request branch. On failure the
    /// git side is rolled back and the workspace is kept as `errored` with the reason.
    pub async fn create_with(&self, project_id: &str, opts: CreateOptions) -> AppResult<Workspace> {
        let project = self.projects.get(project_id)?;
        let repo = self.repo_path(&project);
        if !repo.is_dir() {
            return Err(invalid(format!(
                "The project folder no longer exists: {}",
                project.path
            )));
        }

        // Mistakes the user can fix, such as a branch that is in use elsewhere, fail here and
        // leave no workspace behind.
        let plan = self
            .plan(&project, &repo, &opts.source, opts.branch_name.as_deref())
            .await?;
        self.ensure_identity(&repo).await;
        let ws = self.insert_creating(&project, &plan, &opts)?;
        let _workspace_lock = self.locks.acquire(&ws.id).await;

        self.add_event(
            &ws.id,
            "created",
            &format!(
                "You're in a new copy of {} called {}",
                project.name,
                ws.city.to_lowercase()
            ),
        );

        let mut progress = Progress {
            worktree_added: false,
            created_branch: false,
            branch: plan.branch.clone(),
        };
        match self
            .provision(&ws, &project, &plan, &opts, &mut progress)
            .await
        {
            Ok(()) => self.get(&ws.id),
            Err(e) => {
                tracing::warn!(workspace = %ws.city, error = %e, "workspace creation failed, rolling back");
                self.rollback_creation(&ws, &repo, &progress).await;
                self.add_event(&ws.id, "error", &e.to_string());
                let _ = self.set_state(&ws.id, WorkspaceState::Errored, Some(&e.to_string()));
                Err(e)
            }
        }
    }

    fn announce(&self, ws: &Workspace, stage: &str) {
        self.sink.progress(&ProgressEvent {
            workspace_id: ws.id.clone(),
            project_id: ws.project_id.clone(),
            stage: stage.to_string(),
        });
    }

    async fn plan(
        &self,
        project: &Project,
        repo: &Path,
        source: &CreateSource,
        branch_name: Option<&str>,
    ) -> AppResult<Plan> {
        match source {
            CreateSource::NewBranch { base_branch } => {
                let base = base_branch
                    .as_deref()
                    .map(str::trim)
                    .filter(|b| !b.is_empty())
                    .unwrap_or(&project.default_branch)
                    .to_string();
                crate::git::not_an_option(&[&base])?;
                let branch = match branch_name.map(str::trim).filter(|n| !n.is_empty()) {
                    Some(name) => {
                        crate::git::not_an_option(&[name])?;
                        if !self.projects.git.is_valid_branch_name(repo, name).await? {
                            return Err(invalid(format!("\"{name}\" is not a valid branch name.")));
                        }
                        if self.projects.git.branch_exists(repo, name).await? {
                            return Err(invalid(format!(
                                "The branch \"{name}\" already exists. Choose another name."
                            )));
                        }
                        name.to_string()
                    }
                    None => {
                        let prefix = self.branch_prefix_for(project, repo).await;
                        self.fresh_branch_name(repo, &prefix).await?
                    }
                };
                Ok(Plan {
                    checkout: Checkout::New { base: base.clone() },
                    base,
                    branch,
                    title: None,
                    intro: String::new(),
                })
            }
            CreateSource::ExistingBranch { name } => {
                self.plan_existing(project, repo, name.trim(), None).await
            }
            CreateSource::PullRequest { number } => {
                let pr = self.projects.gh.pr_view(repo, *number).await?;
                if pr.is_cross_repository {
                    let local = format!("pr-{number}");
                    self.ensure_not_checked_out(repo, &local).await?;
                    Ok(Plan {
                        checkout: Checkout::Fork { number: *number },
                        base: project.default_branch.clone(),
                        branch: local,
                        title: Some(format!("#{number} {}", pr.title)),
                        intro: format!(
                            "Checked out pull request #{number} from a fork: {}",
                            pr.title
                        ),
                    })
                } else {
                    self.plan_existing(project, repo, &pr.head_ref, Some(&pr))
                        .await
                }
            }
        }
    }

    async fn plan_existing(
        &self,
        project: &Project,
        repo: &Path,
        name: &str,
        pr: Option<&PrInfo>,
    ) -> AppResult<Plan> {
        if name.is_empty() {
            return Err(invalid("Choose a branch."));
        }
        let git = &self.projects.git;
        self.ensure_not_checked_out(repo, name).await?;

        let local = git.branch_exists(repo, name).await?;
        if !local {
            if git.remote_url(repo, "origin").await?.is_some() {
                // A failed fetch is not fatal if origin already has the branch from before.
                let _ = git.fetch(repo, "origin", Some(name)).await;
            }
            if git
                .rev_parse(repo, &format!("refs/remotes/origin/{name}"))
                .await
                .is_err()
            {
                return Err(no_such_branch(name));
            }
        }

        let intro = match (pr, local) {
            (Some(pr), _) => format!(
                "Checked out pull request #{} ({}) on branch {name}",
                pr.number, pr.title
            ),
            (None, true) => format!("Checked out the existing branch {name}"),
            (None, false) => format!("Checked out {name} from origin/{name}"),
        };
        Ok(Plan {
            checkout: if local {
                Checkout::Local
            } else {
                Checkout::Remote
            },
            base: project.default_branch.clone(),
            branch: name.to_string(),
            title: pr.map(|pr| format!("#{} {}", pr.number, pr.title)),
            intro,
        })
    }

    /// A branch can only be checked out in one worktree.
    async fn ensure_not_checked_out(&self, repo: &Path, branch: &str) -> AppResult<()> {
        let list = self.projects.git.worktree_list(repo).await?;
        match list
            .into_iter()
            .find(|w| w.branch.as_deref() == Some(branch))
        {
            Some(w) => Err(GitError {
                kind: GitErrorKind::BranchCheckedOut,
                message: format!("The branch {branch} is already checked out at {}.", w.path),
                stderr: String::new(),
                path: Some(w.path),
            }
            .into()),
            None => Ok(()),
        }
    }

    /// Branches the Create dialog offers, most recently committed first.
    pub async fn list_branches(&self, project_id: &str) -> AppResult<Vec<BranchInfo>> {
        let project = self.projects.get(project_id)?;
        let repo = self.repo_path(&project);
        let checked_out: HashMap<String, String> = self
            .projects
            .git
            .worktree_list(&repo)
            .await?
            .into_iter()
            .filter_map(|w| w.branch.map(|b| (b, w.path)))
            .collect();
        Ok(self
            .projects
            .git
            .branch_refs(&repo)
            .await?
            .into_iter()
            .map(|b| BranchInfo {
                checked_out_at: checked_out.get(&b.name).cloned(),
                name: b.name,
                local: b.local,
                remote: b.remote,
            })
            .collect())
    }

    /// Open pull requests of the project's GitHub repository.
    pub async fn list_pull_requests(&self, project_id: &str) -> AppResult<Vec<PullRequest>> {
        let project = self.projects.get(project_id)?;
        let prs = self
            .projects
            .gh
            .pr_list(&self.repo_path(&project), 50)
            .await?;
        Ok(prs
            .into_iter()
            .map(|p| PullRequest {
                number: p.number,
                title: p.title,
                head_ref: p.head_ref,
                author: p.author,
                is_cross_repository: p.is_cross_repository,
                updated_at: p.updated_at,
            })
            .collect())
    }

    /// Reserves a city and a folder, and stores the workspace as `creating`, in one transaction.
    fn insert_creating(
        &self,
        project: &Project,
        plan: &Plan,
        opts: &CreateOptions,
    ) -> AppResult<Workspace> {
        let root = self
            .projects
            .paths
            .workspaces_dir
            .join(names::project_dir_name(&project.name));
        let id = new_id();
        let now = now_ms();
        let clean = |v: &Option<String>| {
            v.as_deref()
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_string)
        };
        let (prompt, model, effort) =
            (clean(&opts.prompt), clean(&opts.model), clean(&opts.effort));

        self.projects.db.write(|c| {
            let used: HashSet<String> = {
                let mut stmt = c.prepare("SELECT city FROM workspaces WHERE project_id = ?1")?;
                let rows = stmt.query_map(params![project.id], |r| r.get::<_, String>(0))?;
                rows.map(|r| r.map(|city| names::city_slug(&city)))
                    .collect::<Result<_, _>>()?
            };
            let city = names::pick_city(&used, &|slug| !root.join(slug).exists(), &mut |n| {
                rand::random_range(0..n)
            });
            let path = root.join(names::city_slug(&city));
            c.execute(
                "INSERT INTO workspaces
                   (id, project_id, city, title, branch, base_branch, path, state,
                    pending_prompt, pending_model, pending_effort, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'creating', ?8, ?9, ?10, ?11, ?11)",
                params![
                    id,
                    project.id,
                    city,
                    plan.title,
                    plan.branch,
                    plan.base,
                    path.to_string_lossy(),
                    prompt,
                    model,
                    effort,
                    now
                ],
            )?;
            Ok(())
        })?;

        self.sink.state_changed(&StateChange {
            workspace_id: id.clone(),
            project_id: project.id.clone(),
            from: None,
            to: WorkspaceState::Creating,
            error: None,
        });
        self.get(&id)
    }

    /// Where a new branch starts: `origin/<base>` after a fetch, falling back to the local base.
    /// Returns the start point and the event to record.
    async fn resolve_start(
        &self,
        repo: &Path,
        base: &str,
        branch: &str,
    ) -> AppResult<(String, &'static str, String)> {
        let git = &self.projects.git;
        let local_exists = git.branch_exists(repo, base).await?;

        if git.remote_url(repo, "origin").await?.is_none() {
            if !local_exists {
                return Err(branch_missing(base));
            }
            return Ok((
                base.to_string(),
                "info",
                format!("Branched {branch} from {base} (this repository has no remote)"),
            ));
        }

        match git.fetch(repo, "origin", Some(base)).await {
            Ok(())
                if git
                    .rev_parse(repo, &format!("refs/remotes/origin/{base}"))
                    .await
                    .is_ok() =>
            {
                Ok((
                    format!("origin/{base}"),
                    "info",
                    format!("Branched {branch} from origin/{base}"),
                ))
            }
            outcome => {
                let reason = match outcome {
                    Err(e) => e.message,
                    Ok(()) => format!("origin has no branch named {base}"),
                };
                if local_exists {
                    Ok((
                        base.to_string(),
                        "warning",
                        format!("Could not update from origin ({reason}). Branched {branch} from local {base}."),
                    ))
                } else if git
                    .rev_parse(repo, &format!("refs/remotes/origin/{base}"))
                    .await
                    .is_ok()
                {
                    Ok((
                        format!("origin/{base}"),
                        "warning",
                        format!("Could not update from origin ({reason}). Branched {branch} from the last fetched origin/{base}."),
                    ))
                } else {
                    Err(branch_missing(base))
                }
            }
        }
    }

    async fn provision(
        &self,
        ws: &Workspace,
        project: &Project,
        plan: &Plan,
        opts: &CreateOptions,
        progress: &mut Progress,
    ) -> AppResult<()> {
        let repo = self.repo_path(project);
        let path = PathBuf::from(&ws.path);
        let git = &self.projects.git;
        let settings = self.load_settings(project)?;

        // Serialise worktree changes per repository: git locks its own metadata, and a clear
        // order gives clear errors.
        let _project_lock = self.locks.acquire(&format!("project:{}", project.id)).await;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let (kind, message) = match &plan.checkout {
            Checkout::New { base } => {
                self.announce(ws, "fetching");
                let (start, kind, message) =
                    self.resolve_start(&repo, base, &progress.branch).await?;
                self.announce(ws, "creating_worktree");
                let mut attempts = 0;
                loop {
                    match git
                        .worktree_add_new_branch(&repo, &path, &progress.branch, &start)
                        .await
                    {
                        Ok(()) => break,
                        Err(e) if e.kind == GitErrorKind::BranchExists && attempts < 5 => {
                            attempts += 1;
                            let prefix = progress
                                .branch
                                .rsplit_once('/')
                                .map(|(p, _)| p.to_string())
                                .unwrap_or_default();
                            progress.branch = self.fresh_branch_name(&repo, &prefix).await?;
                        }
                        Err(e) => return Err(e.into()),
                    }
                }
                progress.worktree_added = true;
                progress.created_branch = true;
                // The branch name may have changed on a retry.
                self.projects.db.write(|c| {
                    c.execute(
                        "UPDATE workspaces SET branch = ?1 WHERE id = ?2",
                        params![progress.branch, ws.id],
                    )
                })?;
                (kind, message.replace(&ws.branch, &progress.branch))
            }
            Checkout::Local => {
                self.announce(ws, "creating_worktree");
                git.worktree_add_existing(&repo, &path, &progress.branch)
                    .await?;
                progress.worktree_added = true;
                ("info", plan.intro.clone())
            }
            Checkout::Remote => {
                self.announce(ws, "creating_worktree");
                let remote = format!("origin/{}", progress.branch);
                git.worktree_add_tracking(&repo, &path, &progress.branch, &remote)
                    .await?;
                progress.worktree_added = true;
                progress.created_branch = true;
                ("info", plan.intro.clone())
            }
            Checkout::Fork { number } => {
                self.announce(ws, "fetching");
                let existed = git.branch_exists(&repo, &progress.branch).await?;
                git.fetch_pull_head(&repo, *number, &progress.branch)
                    .await?;
                self.announce(ws, "creating_worktree");
                git.worktree_add_existing(&repo, &path, &progress.branch)
                    .await?;
                progress.worktree_added = true;
                progress.created_branch = !existed;
                ("info", plan.intro.clone())
            }
        };
        self.add_event(&ws.id, kind, &message);

        ports::allocate(&self.projects.db, &ws.id, &SystemProbe)?;
        self.announce(ws, "copying_files");
        self.prepare_context_dir(&path).await?;
        self.copy_local_files(project, &settings, &path).await?;
        self.attach_files(ws, &path, &opts.attachments);

        self.announce(ws, "finishing");
        self.begin_work(ws, project, &settings).await?;
        Ok(())
    }

    // Scripts

    fn context(
        &self,
        ws: &Workspace,
        project: &Project,
        settings: &Settings,
        port: Option<u16>,
    ) -> Context {
        Context {
            workspace_id: ws.id.clone(),
            project_id: project.id.clone(),
            cwd: PathBuf::from(&ws.path),
            env: scripts::workspace_env(ws, project, port, settings),
        }
    }

    /// What a terminal or script needs to run in `workspace_id`, and the scripts configured for it.
    /// Gives a workspace created before ports existed its port block.
    pub fn context_for(&self, workspace_id: &str) -> AppResult<(Context, ScriptsInfo)> {
        let ws = self.get(workspace_id)?;
        if !ws.state.has_worktree() {
            return Err(invalid(format!(
                "{} has no folder right now. Restore it first.",
                ws.city
            )));
        }
        let project = self.projects.get(&ws.project_id)?;
        let settings = self.load_settings(&project)?;
        let port = ports::allocate(&self.projects.db, &ws.id, &SystemProbe).ok();
        Ok((
            self.context(&ws, &project, &settings, port),
            scripts::resolve(&settings.scripts)?,
        ))
    }

    /// The workspace is ready to use. If it has a setup script, that runs first, in the Setup
    /// tab, and the workspace becomes ready when it ends. A failing setup does not block it.
    async fn begin_work(
        &self,
        ws: &Workspace,
        project: &Project,
        settings: &Settings,
    ) -> AppResult<()> {
        let info = scripts::resolve(&settings.scripts)?;
        let Some(command) = info.setup else {
            self.set_state(&ws.id, WorkspaceState::Ready, None)?;
            return Ok(());
        };

        let port = ports::allocate(&self.projects.db, &ws.id, &SystemProbe).ok();
        let ctx = self.context(ws, project, settings, port);
        self.set_state(&ws.id, WorkspaceState::SetupRunning, None)?;
        self.add_event(
            &ws.id,
            "info",
            &format!("Running the setup script: {command}"),
        );
        let pty = self.terminals.run_setup(&self.env, &ctx, &command).await?;

        let this = self.clone();
        let id = ws.id.clone();
        tokio::spawn(async move {
            let exit = pty.wait_exit().await;
            this.finish_setup(&id, exit).await;
        });
        Ok(())
    }

    async fn finish_setup(&self, id: &str, exit: ExitInfo) {
        let _lock = self.locks.acquire(id).await;
        let Ok(ws) = self.get(id) else { return };
        if ws.state != WorkspaceState::SetupRunning {
            return;
        }
        if exit.code == Some(0) {
            self.add_event(id, "info", "The setup script finished");
        } else {
            let how = match exit.code {
                Some(code) => format!("exited with code {code}"),
                None => "was stopped".to_string(),
            };
            self.add_event(
                id,
                "warning",
                &format!("The setup script {how}. Open the Setup tab to see why. The workspace is ready to use anyway."),
            );
        }
        let _ = self.set_state(id, WorkspaceState::Ready, None);
    }

    /// Runs the archive script, if there is one. It never stops the archive: a script that fails
    /// or hangs is reported and the archive goes on.
    async fn run_archive_script(&self, ws: &Workspace, project: &Project) {
        let settings = match self.load_settings(project) {
            Ok(s) => s,
            Err(e) => {
                self.add_event(
                    &ws.id,
                    "warning",
                    &format!("The archive script was skipped: {e}"),
                );
                return;
            }
        };
        let command = match scripts::resolve(&settings.scripts) {
            Ok(info) => info.archive,
            Err(e) => {
                self.add_event(
                    &ws.id,
                    "warning",
                    &format!("The archive script was skipped: {e}"),
                );
                return;
            }
        };
        let Some(command) = command else { return };

        let port = ports::get(&self.projects.db, &ws.id).ok().flatten();
        let ctx = self.context(ws, project, &settings, port);
        self.add_event(
            &ws.id,
            "info",
            &format!("Running the archive script: {command}"),
        );
        match self
            .terminals
            .run_to_completion(&self.env, &ctx, &command, ARCHIVE_SCRIPT_LIMIT)
            .await
        {
            Ok((exit, _)) if exit.code == Some(0) => {
                self.add_event(&ws.id, "info", "The archive script finished");
            }
            Ok((exit, output)) => {
                let code = exit
                    .code
                    .map(|c| format!(" with code {c}"))
                    .unwrap_or_default();
                self.add_event(
                    &ws.id,
                    "warning",
                    &format!(
                        "The archive script failed{code}. Archiving went on.{}",
                        tail_of(&output)
                    ),
                );
            }
            Err(e) => self.add_event(
                &ws.id,
                "warning",
                &format!("The archive script did not finish ({e}). Archiving went on."),
            ),
        }
    }

    /// Copies files the user attached in the Create dialog into `.context/attachments/`.
    /// A file that cannot be attached is reported and skipped, it never fails the creation.
    fn attach_files(&self, ws: &Workspace, worktree: &Path, paths: &[String]) {
        if paths.is_empty() {
            return;
        }
        let dir = worktree.join(CONTEXT_DIR).join("attachments");
        let mut attached = 0;
        for raw in paths {
            match copy::copy_attachment(Path::new(raw), &dir) {
                Ok(_) => attached += 1,
                Err(reason) => {
                    let name = Path::new(raw)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| raw.clone());
                    self.add_event(
                        &ws.id,
                        "warning",
                        &format!("Could not attach {name}: {reason}"),
                    );
                }
            }
        }
        if attached > 0 {
            self.add_event(
                &ws.id,
                "info",
                &format!(
                    "Attached {attached} file{}",
                    if attached == 1 { "" } else { "s" }
                ),
            );
        }
    }

    async fn rollback_creation(&self, ws: &Workspace, repo: &Path, progress: &Progress) {
        let git = &self.projects.git;
        let path = PathBuf::from(&ws.path);
        if progress.worktree_added {
            let _ = git.worktree_remove(repo, &path, true).await;
            self.remove_managed_dir(&path);
            let _ = git.worktree_prune(repo).await;
            if progress.created_branch {
                let _ = git.branch_delete(repo, &progress.branch, true).await;
            }
        }
    }

    // Archive and restore

    /// Removes the worktree and keeps the branch. Uncommitted work is never discarded: it must be
    /// committed (`commit_dirty`) or the archive is refused.
    pub async fn archive(&self, id: &str, commit_dirty: bool) -> AppResult<Workspace> {
        let _workspace_lock = self.locks.acquire(id).await;
        let ws = self.get(id)?;
        match ws.state {
            WorkspaceState::Archived => return Ok(ws),
            s if s.is_busy() => {
                return Err(invalid(format!(
                    "Stop the agent in {} before archiving it.",
                    ws.city
                )))
            }
            s if s.is_provisioning() => {
                return Err(invalid(format!("{} is still being set up.", ws.city)))
            }
            _ => {}
        }

        let project = self.projects.get(&ws.project_id)?;
        let repo = self.repo_path(&project);
        let path = PathBuf::from(&ws.path);
        let _project_lock = self.locks.acquire(&format!("project:{}", project.id)).await;
        let git = &self.projects.git;

        if path.join(".git").exists() {
            let status = git.status(&path).await?;
            if status.is_dirty {
                if !commit_dirty {
                    let n = status.files.len();
                    return Err(GitError {
                        kind: GitErrorKind::DirtyTree,
                        message: format!(
                            "{} has {n} uncommitted change{}. Commit them, or archive with auto-commit.",
                            ws.city,
                            if n == 1 { "" } else { "s" }
                        ),
                        stderr: String::new(),
                        path: Some(ws.path.clone()),
                    }
                    .into());
                }
                git.commit_all(&path, "WIP: archived from Circles").await?;
                self.add_event(id, "info", "Committed uncommitted changes before archiving");
            }

            // Servers and shells end first, so they let go of ports and files.
            self.terminals.stop_workspace(id).await;
            self.run_archive_script(&ws, &project).await;

            self.save_context(&ws)?;
            if let Err(e) = git.worktree_remove(&repo, &path, false).await {
                let _ = std::fs::remove_dir_all(self.context_backup_dir(id));
                return Err(e.into());
            }
        }
        self.terminals.stop_workspace(id).await;
        let _ = ports::release(&self.projects.db, id);
        let _ = git.worktree_prune(&repo).await;

        let archived = self.set_state(id, WorkspaceState::Archived, None)?;
        self.add_event(
            id,
            "info",
            &format!("Archived. The branch {} was kept.", ws.branch),
        );
        Ok(archived)
    }

    /// Copies `.context/` out of the worktree before it is removed.
    fn save_context(&self, ws: &Workspace) -> AppResult<()> {
        let src = Path::new(&ws.path).join(CONTEXT_DIR);
        if !src.is_dir() {
            return Ok(());
        }
        let dst = self.context_backup_dir(&ws.id);
        let _ = std::fs::remove_dir_all(&dst);
        copy::copy_path(&src, &dst)?;
        Ok(())
    }

    /// Puts a saved `.context/` back, or creates an empty one.
    fn restore_context(&self, ws: &Workspace) -> AppResult<()> {
        let backup = self.context_backup_dir(&ws.id);
        let dst = Path::new(&ws.path).join(CONTEXT_DIR);
        if backup.is_dir() {
            copy::copy_path(&backup, &dst)?;
            let _ = std::fs::remove_dir_all(&backup);
        } else {
            std::fs::create_dir_all(&dst)?;
        }
        Ok(())
    }

    /// Recreates the worktree for an archived (or broken) workspace from its branch.
    pub async fn restore(&self, id: &str) -> AppResult<Workspace> {
        let _workspace_lock = self.locks.acquire(id).await;
        let ws = self.get(id)?;
        if !matches!(ws.state, WorkspaceState::Archived | WorkspaceState::Errored) {
            return Err(invalid(format!("{} is not archived.", ws.city)));
        }
        let project = self.projects.get(&ws.project_id)?;
        self.set_state(id, WorkspaceState::Creating, None)?;

        match self.reprovision(&ws, &project).await {
            Ok(()) => {
                self.add_event(
                    id,
                    "info",
                    &format!("Restored {} on {}", ws.city.to_lowercase(), ws.branch),
                );
                let settings = self.load_settings(&project)?;
                self.begin_work(&ws, &project, &settings).await?;
                self.get(id)
            }
            Err(e) => {
                self.add_event(id, "error", &e.to_string());
                let _ = self.set_state(id, WorkspaceState::Errored, Some(&e.to_string()));
                Err(e)
            }
        }
    }

    async fn reprovision(&self, ws: &Workspace, project: &Project) -> AppResult<()> {
        let repo = self.repo_path(project);
        let path = PathBuf::from(&ws.path);
        let git = &self.projects.git;
        let settings = self.load_settings(project)?;
        let _project_lock = self.locks.acquire(&format!("project:{}", project.id)).await;

        if !git.branch_exists(&repo, &ws.branch).await? {
            return Err(GitError::new(
                GitErrorKind::BranchNotFound,
                format!(
                    "The branch {} no longer exists, so {} cannot be restored.",
                    ws.branch, ws.city
                ),
            )
            .into());
        }

        let registered = git
            .worktree_list(&repo)
            .await?
            .iter()
            .any(|w| paths_equal(Path::new(&w.path), &path));
        if !(registered && path.join(".git").exists()) {
            git.worktree_prune(&repo).await?;
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            git.worktree_add_existing(&repo, &path, &ws.branch).await?;
        }

        ports::allocate(&self.projects.db, &ws.id, &SystemProbe)?;
        self.prepare_context_dir(&path).await?;
        self.restore_context(ws)?;
        self.copy_local_files(project, &settings, &path).await?;
        Ok(())
    }

    // Edit

    pub async fn rename_branch(&self, id: &str, new_name: &str) -> AppResult<Workspace> {
        let _workspace_lock = self.locks.acquire(id).await;
        let ws = self.get(id)?;
        let new_name = new_name.trim();
        if ws.state.is_provisioning() {
            return Err(invalid(format!("{} is still being set up.", ws.city)));
        }
        if new_name.is_empty() || new_name == ws.branch {
            return Err(invalid("Enter a new branch name."));
        }
        let project = self.projects.get(&ws.project_id)?;
        let repo = self.repo_path(&project);
        if !self
            .projects
            .git
            .is_valid_branch_name(&repo, new_name)
            .await?
        {
            return Err(invalid(format!(
                "\"{new_name}\" is not a valid branch name."
            )));
        }

        let _project_lock = self.locks.acquire(&format!("project:{}", project.id)).await;
        self.projects
            .git
            .branch_rename(&repo, &ws.branch, new_name)
            .await?;
        self.projects.db.write(|c| {
            c.execute(
                "UPDATE workspaces SET branch = ?1, updated_at = ?2 WHERE id = ?3",
                params![new_name, now_ms(), id],
            )
        })?;
        self.add_event(
            id,
            "info",
            &format!("Renamed branch {} to {new_name}", ws.branch),
        );
        self.get(id)
    }

    pub fn set_pinned(&self, id: &str, pinned: bool) -> AppResult<Workspace> {
        self.get(id)?;
        self.projects.db.write(|c| {
            c.execute(
                "UPDATE workspaces SET pinned = ?1 WHERE id = ?2",
                params![pinned as i64, id],
            )
        })?;
        self.get(id)
    }

    pub fn set_title(&self, id: &str, title: Option<&str>) -> AppResult<Workspace> {
        self.get(id)?;
        let title = title.map(str::trim).filter(|t| !t.is_empty());
        self.projects.db.write(|c| {
            c.execute(
                "UPDATE workspaces SET title = ?1, updated_at = ?2 WHERE id = ?3",
                params![title, now_ms(), id],
            )
        })?;
        self.get(id)
    }

    /// Deletes an archived or errored workspace record. The branch is kept unless asked.
    pub async fn delete(&self, id: &str, delete_branch: bool) -> AppResult<()> {
        self.delete_inner(id, delete_branch, false).await
    }

    /// Deletes a workspace in any state, for removing its whole project: the worktree goes, with
    /// anything in it that was not committed. The branch is always kept.
    pub async fn purge(&self, id: &str) -> AppResult<()> {
        self.delete_inner(id, false, true).await
    }

    /// Lets a workspace go without touching its folder. Circles stops tracking it, and the folder
    /// stays on disk as the working copy it was, still a worktree of the repository.
    pub async fn release(&self, id: &str) -> AppResult<()> {
        let _workspace_lock = self.locks.acquire(id).await;
        let ws = self.get(id)?;
        self.terminals.stop_workspace(id).await;
        self.terminals.forget_workspace(id);
        let _ = ports::release(&self.projects.db, id);
        self.projects
            .db
            .write(|c| c.execute("DELETE FROM workspaces WHERE id = ?1", params![id]))?;
        tracing::info!(workspace = %ws.city, "workspace released, its folder was kept");
        Ok(())
    }

    async fn delete_inner(&self, id: &str, delete_branch: bool, force: bool) -> AppResult<()> {
        let _workspace_lock = self.locks.acquire(id).await;
        let ws = self.get(id)?;
        if !force && !matches!(ws.state, WorkspaceState::Archived | WorkspaceState::Errored) {
            return Err(invalid(format!("Archive {} before deleting it.", ws.city)));
        }
        let project = self.projects.get(&ws.project_id)?;
        let repo = self.repo_path(&project);
        let path = PathBuf::from(&ws.path);
        let _project_lock = self.locks.acquire(&format!("project:{}", project.id)).await;
        let git = &self.projects.git;

        if path.join(".git").exists() {
            let _ = git.worktree_remove(&repo, &path, true).await;
        }
        self.remove_managed_dir(&path);
        let _ = git.worktree_prune(&repo).await;

        if delete_branch && git.branch_exists(&repo, &ws.branch).await? {
            git.branch_delete(&repo, &ws.branch, true).await?;
        }
        let _ = std::fs::remove_dir_all(self.context_backup_dir(id));
        self.terminals.stop_workspace(id).await;
        self.terminals.forget_workspace(id);
        self.projects
            .db
            .write(|c| c.execute("DELETE FROM workspaces WHERE id = ?1", params![id]))?;
        // Tidy up the project's folder under ~/circles/workspaces once it holds nothing. This
        // only removes an empty directory, so it can never delete anything of value.
        if let Some(parent) = path.parent() {
            if self.is_managed(parent) {
                let _ = std::fs::remove_dir(parent);
            }
        }
        Ok(())
    }

    // Startup

    /// Brings the database back in line with what is on disk. Safe to run at any time.
    pub async fn reconcile(&self) -> AppResult<ReconcileReport> {
        let mut report = ReconcileReport::default();
        let live: Vec<Workspace> = self
            .list(None, true)?
            .into_iter()
            .filter(|w| w.state != WorkspaceState::Archived)
            .collect();

        let mut by_project: HashMap<String, Vec<Workspace>> = HashMap::new();
        for ws in live {
            by_project
                .entry(ws.project_id.clone())
                .or_default()
                .push(ws);
        }

        for (project_id, workspaces) in by_project {
            let project = match self.projects.get(&project_id) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let repo = self.repo_path(&project);
            let registered: Option<Vec<PathBuf>> = if repo.is_dir() {
                match self.projects.git.worktree_list(&repo).await {
                    Ok(list) => Some(list.into_iter().map(|w| PathBuf::from(w.path)).collect()),
                    Err(e) => {
                        tracing::warn!(project = %project.name, error = %e, "cannot list worktrees");
                        continue;
                    }
                }
            } else {
                None
            };

            for ws in workspaces {
                let path = PathBuf::from(&ws.path);
                let present = registered
                    .as_ref()
                    .is_some_and(|list| list.iter().any(|p| paths_equal(p, &path)))
                    && path.join(".git").exists();

                match ws.state {
                    WorkspaceState::SetupRunning if present => {
                        // The worktree is fine. Only the setup script was cut short.
                        self.add_event(
                            &ws.id,
                            "warning",
                            "Setup was interrupted when Circles quit. Run it again from the Setup tab.",
                        );
                        self.set_state(&ws.id, WorkspaceState::Ready, None)?;
                        report.recovered += 1;
                    }
                    WorkspaceState::Creating | WorkspaceState::SetupRunning => {
                        self.mark_errored(&ws, "Setup was interrupted when Circles quit. Restore the workspace to try again.");
                        report.errored += 1;
                    }
                    WorkspaceState::Working | WorkspaceState::Blocked => {
                        if present {
                            self.set_state(&ws.id, WorkspaceState::Idle, None)?;
                            report.reset += 1;
                        } else {
                            self.mark_errored(&ws, "The worktree folder is missing.");
                            report.errored += 1;
                        }
                    }
                    WorkspaceState::Ready | WorkspaceState::Idle if !present => {
                        self.mark_errored(&ws, "The worktree folder is missing.");
                        report.errored += 1;
                    }
                    WorkspaceState::Errored if present => {
                        self.set_state(&ws.id, WorkspaceState::Ready, None)?;
                        self.add_event(
                            &ws.id,
                            "info",
                            "The worktree folder is back, so the workspace was recovered.",
                        );
                        report.recovered += 1;
                    }
                    _ => {}
                }
            }

            if repo.is_dir() {
                let _ = self.projects.git.worktree_prune(&repo).await;
            }
        }
        Ok(report)
    }

    fn mark_errored(&self, ws: &Workspace, reason: &str) {
        self.add_event(&ws.id, "error", reason);
        if let Err(e) = self.set_state(&ws.id, WorkspaceState::Errored, Some(reason)) {
            tracing::warn!(workspace = %ws.city, error = %e, "cannot mark workspace as errored");
        }
    }

    // Sections

    pub fn list_sections(&self) -> AppResult<Vec<Section>> {
        self.projects.db.read(|c| {
            let mut stmt =
                c.prepare("SELECT id, name, position FROM sections ORDER BY position, rowid")?;
            let rows = stmt.query_map([], |r| {
                Ok(Section {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    position: r.get(2)?,
                })
            })?;
            rows.collect()
        })
    }

    fn section(&self, id: &str) -> AppResult<Section> {
        self.list_sections()?
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| AppError::NotFound(format!("section {id}")))
    }

    fn clean_section_name(name: &str) -> AppResult<&str> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 60 {
            Err(invalid("Section names need 1 to 60 characters."))
        } else {
            Ok(name)
        }
    }

    pub fn create_section(&self, name: &str) -> AppResult<Section> {
        let name = Self::clean_section_name(name)?;
        let id = new_id();
        self.projects.db.write(|c| {
            let next: i64 = c.query_row(
                "SELECT COALESCE(MAX(position) + 1, 0) FROM sections",
                [],
                |r| r.get(0),
            )?;
            c.execute(
                "INSERT INTO sections (id, name, position, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![id, name, next, now_ms()],
            )
        })?;
        self.section(&id)
    }

    pub fn rename_section(&self, id: &str, name: &str) -> AppResult<Section> {
        let name = Self::clean_section_name(name)?;
        self.section(id)?;
        self.projects.db.write(|c| {
            c.execute(
                "UPDATE sections SET name = ?1 WHERE id = ?2",
                params![name, id],
            )
        })?;
        self.section(id)
    }

    /// Deleting a section keeps its workspaces and returns them to their project.
    pub fn delete_section(&self, id: &str) -> AppResult<()> {
        self.section(id)?;
        self.projects
            .db
            .write(|c| c.execute("DELETE FROM sections WHERE id = ?1", params![id]))?;
        Ok(())
    }

    /// `ids` must list every section exactly once, in the new order.
    pub fn reorder_sections(&self, ids: &[String]) -> AppResult<Vec<Section>> {
        let existing: HashSet<String> = self.list_sections()?.into_iter().map(|s| s.id).collect();
        let given: HashSet<&String> = ids.iter().collect();
        if given.len() != ids.len()
            || ids.len() != existing.len()
            || !ids.iter().all(|i| existing.contains(i))
        {
            return Err(invalid("Send every section exactly once to reorder them."));
        }
        self.projects.db.write(|c| {
            let tx = c.unchecked_transaction()?;
            for (position, id) in ids.iter().enumerate() {
                tx.execute(
                    "UPDATE sections SET position = ?1 WHERE id = ?2",
                    params![position as i64, id],
                )?;
            }
            tx.commit()
        })?;
        self.list_sections()
    }

    pub fn set_section(
        &self,
        workspace_id: &str,
        section_id: Option<&str>,
    ) -> AppResult<Workspace> {
        self.get(workspace_id)?;
        if let Some(id) = section_id {
            self.section(id)?;
        }
        self.projects.db.write(|c| {
            c.execute(
                "UPDATE workspaces SET section_id = ?1 WHERE id = ?2",
                params![section_id, workspace_id],
            )
        })?;
        self.get(workspace_id)
    }
}

/// The last lines of script output, without terminal colors, for a short message.
fn tail_of(output: &str) -> String {
    let mut clean = String::new();
    let mut chars = output.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // Skip an escape sequence such as ESC [ 31 m.
            if chars.peek() == Some(&'[') {
                chars.next();
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else if c != '\r' {
            clean.push(c);
        }
    }
    let lines: Vec<&str> = clean.lines().filter(|l| !l.trim().is_empty()).collect();
    let tail = lines[lines.len().saturating_sub(3)..].join(" / ");
    if tail.is_empty() {
        String::new()
    } else if tail.chars().count() > 300 {
        format!(
            " Output: ...{}",
            tail.chars()
                .skip(tail.chars().count() - 300)
                .collect::<String>()
        )
    } else {
        format!(" Output: {tail}")
    }
}

fn no_such_branch(name: &str) -> AppError {
    GitError::new(
        GitErrorKind::BranchNotFound,
        format!("The branch \"{name}\" does not exist locally or on origin."),
    )
    .into()
}

fn branch_missing(base: &str) -> AppError {
    GitError::new(
        GitErrorKind::BranchNotFound,
        format!("The base branch \"{base}\" does not exist locally or on origin."),
    )
    .into()
}

/// Compares two paths, resolving symlinks such as `/var` and `/private/var` when they exist.
fn paths_equal(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// Appends `line` to `file` unless it is already there. Creates the file and its directory.
fn ensure_line(file: &Path, line: &str) -> std::io::Result<()> {
    let existing = std::fs::read_to_string(file).unwrap_or_default();
    if existing.lines().any(|l| l.trim() == line) {
        return Ok(());
    }
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut text = existing;
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(line);
    text.push('\n');
    std::fs::write(file, text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::git::testutil::{repo_with_commit, sh, test_git};
    use crate::github::Gh;
    use crate::paths::AppPaths;
    use crate::workspace::events::CollectSink;

    struct Fx {
        _tmp: tempfile::TempDir,
        root: PathBuf,
        repo: PathBuf,
        svc: WorkspaceService,
        sink: Arc<CollectSink>,
        project: Project,
    }

    fn service(root: &Path) -> (WorkspaceService, Arc<CollectSink>) {
        let paths = AppPaths::from_roots(&root.join("data"), &root.join("home"));
        paths.ensure().unwrap();
        let db = Arc::new(Db::open(&paths.db_file).unwrap());
        let terminals = Terminals::new(
            db.clone(),
            paths.clone(),
            Arc::new(crate::terminal::NullNotifier),
        );
        let projects = ProjectService {
            db,
            git: test_git(root),
            gh: Gh::new(Arc::new(crate::env::ResolvedEnv::for_test(&[(
                "PATH",
                "/nonexistent",
            )]))),
            paths,
        };
        let sink = CollectSink::new();
        let path = std::env::var("PATH").unwrap_or_default();
        let svc = WorkspaceService {
            projects,
            sink: sink.clone(),
            locks: Arc::new(Locks::default()),
            terminals,
            env: Arc::new(crate::env::ResolvedEnv::for_test(&[
                ("PATH", path.as_str()),
                ("SHELL", "/bin/sh"),
                ("HOME", "/tmp"),
            ])),
        };
        svc.kv_set(GH_LOGIN_KEY, "tester");
        (svc, sink)
    }

    #[tokio::test]
    async fn a_clone_of_circles_commits_as_the_account_that_owns_it() {
        let fx = fixture().await;
        let repo = fx.svc.projects.paths.repos_dir.join("owned");
        repo_with_commit(&fx.svc.projects.git, &repo).await;
        sh(
            &repo,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/decimozs/owned.git",
            ],
        );
        fx.svc.kv_set(
            &format!("{GH_IDENTITY_KEY}:{}", repo.display()),
            "decimozs\t42",
        );
        fx.svc.ensure_identity(&repo).await;
        let git = &fx.svc.projects.git;
        assert_eq!(
            git.config_get_local(&repo, "user.name")
                .await
                .unwrap()
                .as_deref(),
            Some("decimozs")
        );
        assert_eq!(
            git.config_get_local(&repo, "user.email")
                .await
                .unwrap()
                .as_deref(),
            Some("42+decimozs@users.noreply.github.com")
        );
    }

    #[tokio::test]
    async fn the_branch_states_count_commits_and_have_no_pull_request_off_github() {
        let fx = fixture().await;
        let ws = fx.svc.create(&fx.project.id, None).await.unwrap();
        let none = fx.svc.branch_states().await.unwrap();
        assert_eq!(none.len(), 1);
        assert_eq!(none[0].workspace_id, ws.id);
        assert_eq!(
            (none[0].ahead, none[0].pr_number, none[0].pr_state),
            (0, None, None)
        );

        let dir = Path::new(&ws.path);
        std::fs::write(dir.join("new.txt"), "x").unwrap();
        sh(dir, &["add", "-A"]);
        sh(
            dir,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-q",
                "-m",
                "work",
            ],
        );
        let one = fx.svc.branch_states().await.unwrap();
        assert_eq!(one[0].ahead, 1);
    }

    #[tokio::test]
    async fn a_repository_from_the_persons_own_folder_keeps_its_identity() {
        let fx = fixture().await;
        sh(
            &fx.repo,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/decimozs/roster.git",
            ],
        );
        fx.svc.kv_set(
            &format!("{GH_IDENTITY_KEY}:{}", fx.repo.display()),
            "decimozs\t42",
        );
        fx.svc.ensure_identity(&fx.repo).await;
        let local = fx
            .svc
            .projects
            .git
            .config_get_local(&fx.repo, "user.email")
            .await
            .unwrap();
        assert_eq!(local, None);
    }

    #[tokio::test]
    async fn an_identity_the_repository_already_sets_is_not_replaced() {
        let fx = fixture().await;
        let repo = fx.svc.projects.paths.repos_dir.join("owned");
        repo_with_commit(&fx.svc.projects.git, &repo).await;
        sh(
            &repo,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/decimozs/owned.git",
            ],
        );
        sh(&repo, &["config", "user.email", "me@example.com"]);
        fx.svc.kv_set(
            &format!("{GH_IDENTITY_KEY}:{}", repo.display()),
            "decimozs\t42",
        );
        fx.svc.ensure_identity(&repo).await;
        assert_eq!(
            fx.svc
                .projects
                .git
                .config_get_local(&repo, "user.email")
                .await
                .unwrap()
                .as_deref(),
            Some("me@example.com")
        );
    }

    /// A project with one commit on `main` and no remote.
    async fn fixture() -> Fx {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let (svc, sink) = service(&root);
        let repo = root.join("roster");
        repo_with_commit(&svc.projects.git, &repo).await;
        let project = svc
            .projects
            .add_local(repo.to_str().unwrap())
            .await
            .unwrap();
        Fx {
            _tmp: tmp,
            root,
            repo,
            svc,
            sink,
            project,
        }
    }

    /// Same, with a bare `origin` that already has `main` pushed.
    async fn fixture_with_origin() -> Fx {
        let fx = fixture().await;
        let bare = fx.root.join("origin.git");
        sh(
            &fx.root,
            &["init", "-q", "--bare", "-b", "main", "origin.git"],
        );
        sh(
            &fx.repo,
            &["remote", "add", "origin", bare.to_str().unwrap()],
        );
        sh(&fx.repo, &["push", "-q", "-u", "origin", "main"]);
        sh(&fx.repo, &["remote", "set-head", "origin", "main"]);
        fx
    }

    fn messages(fx: &Fx, id: &str) -> Vec<String> {
        fx.svc
            .events(id)
            .unwrap()
            .into_iter()
            .map(|e| e.message)
            .collect()
    }

    fn git_kind(e: &AppError) -> Option<GitErrorKind> {
        match e {
            AppError::Git(g) => Some(g.kind),
            _ => None,
        }
    }

    async fn branches(fx: &Fx) -> Vec<String> {
        let out = std::process::Command::new("git")
            .args(["branch", "--format=%(refname:short)"])
            .current_dir(&fx.repo)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::to_string)
            .collect()
    }

    #[tokio::test]
    async fn creates_a_ready_workspace_with_branch_worktree_and_events() {
        let fx = fixture().await;
        let ws = fx.svc.create(&fx.project.id, None).await.unwrap();

        assert_eq!(ws.state, WorkspaceState::Ready);
        assert!(
            ws.branch.starts_with("tester/") && ws.branch.len() == "tester/".len() + 5,
            "{}",
            ws.branch
        );
        assert_eq!(ws.base_branch, "main");
        let expected = fx
            .svc
            .projects
            .paths
            .workspaces_dir
            .join("roster")
            .join(names::city_slug(&ws.city));
        assert_eq!(PathBuf::from(&ws.path), expected);
        assert!(expected.join("README.md").exists());
        assert!(expected.join(".context").is_dir());

        let status = fx.svc.projects.git.status(&expected).await.unwrap();
        assert_eq!(status.branch.as_deref(), Some(ws.branch.as_str()));
        assert!(!status.is_dirty, ".context must not show up as a change");

        let exclude = std::fs::read_to_string(fx.repo.join(".git/info/exclude")).unwrap();
        assert_eq!(exclude.matches("/.context/").count(), 1);
        assert!(
            !fx.repo.join(".gitignore").exists(),
            "the repository's .gitignore is untouched"
        );

        let lines = messages(&fx, &ws.id);
        assert_eq!(
            lines[0],
            format!(
                "You're in a new copy of roster called {}",
                ws.city.to_lowercase()
            )
        );
        assert!(
            lines[1].contains("from main (this repository has no remote)"),
            "{lines:?}"
        );
        // The summary of what was copied and the hint about a setup script are not shown.
        assert_eq!(lines.len(), 2, "{lines:?}");

        let changes = fx.sink.changes();
        assert_eq!(changes.len(), 2);
        assert_eq!(
            (changes[0].from, changes[0].to),
            (None, WorkspaceState::Creating)
        );
        assert_eq!(
            (changes[1].from, changes[1].to),
            (Some(WorkspaceState::Creating), WorkspaceState::Ready)
        );
        assert_eq!(
            fx.svc.projects.get(&fx.project.id).unwrap().workspace_count,
            1
        );
    }

    #[tokio::test]
    async fn branches_from_the_fetched_origin() {
        let fx = fixture_with_origin().await;
        // Someone else pushes a commit that the local clone has not fetched yet.
        sh(&fx.root, &["clone", "-q", "origin.git", "other"]);
        let other = fx.root.join("other");
        std::fs::write(other.join("upstream.txt"), "new").unwrap();
        sh(&other, &["add", "-A"]);
        sh(&other, &["commit", "-q", "-m", "upstream work"]);
        sh(&other, &["push", "-q", "origin", "main"]);

        let ws = fx.svc.create(&fx.project.id, None).await.unwrap();
        assert!(
            Path::new(&ws.path).join("upstream.txt").exists(),
            "the workspace starts from the latest origin/main"
        );
        assert!(
            messages(&fx, &ws.id)[1].ends_with("from origin/main"),
            "{:?}",
            messages(&fx, &ws.id)
        );
        assert!(
            !fx.repo.join("upstream.txt").exists(),
            "the main checkout is not touched"
        );
        let status = fx
            .svc
            .projects
            .git
            .status(Path::new(&ws.path))
            .await
            .unwrap();
        assert_eq!(status.upstream, None);
    }

    #[tokio::test]
    async fn falls_back_to_the_local_base_when_origin_is_unreachable() {
        let fx = fixture_with_origin().await;
        std::fs::remove_dir_all(fx.root.join("origin.git")).unwrap();

        let ws = fx.svc.create(&fx.project.id, None).await.unwrap();
        assert_eq!(ws.state, WorkspaceState::Ready);
        let events = fx.svc.events(&ws.id).unwrap();
        let warning = events
            .iter()
            .find(|e| e.kind == "warning")
            .expect("a warning event");
        assert!(
            warning.message.contains("Could not update from origin"),
            "{}",
            warning.message
        );
        assert!(
            warning.message.contains("from local main"),
            "{}",
            warning.message
        );
    }

    #[tokio::test]
    async fn a_new_branch_can_be_given_a_name() {
        let fx = fixture().await;
        let mut options = opts(CreateSource::NewBranch { base_branch: None });
        options.branch_name = Some(" feat/add-dark-mode-toggle ".into());
        let ws = fx.svc.create_with(&fx.project.id, options).await.unwrap();
        assert_eq!(ws.branch, "feat/add-dark-mode-toggle");

        // The same name again is refused, and so are names git would not accept.
        let mut again = opts(CreateSource::NewBranch { base_branch: None });
        again.branch_name = Some("feat/add-dark-mode-toggle".into());
        let err = fx.svc.create_with(&fx.project.id, again).await.unwrap_err();
        assert!(err.to_string().contains("already exists"), "{err}");
        for bad in ["has space", "bad..name", "--output=/tmp/x"] {
            let mut o = opts(CreateSource::NewBranch { base_branch: None });
            o.branch_name = Some(bad.into());
            assert!(
                fx.svc.create_with(&fx.project.id, o).await.is_err(),
                "{bad}"
            );
        }
        // Without a name it is generated as before.
        let plain = fx.svc.create(&fx.project.id, None).await.unwrap();
        assert!(plain.branch.contains('/') && !plain.branch.starts_with("feat/"));
    }

    #[tokio::test]
    async fn base_branch_can_be_overridden_per_workspace() {
        let fx = fixture().await;
        sh(&fx.repo, &["branch", "develop"]);
        let ws = fx
            .svc
            .create(&fx.project.id, Some(" develop "))
            .await
            .unwrap();
        assert_eq!(ws.base_branch, "develop");
        assert!(messages(&fx, &ws.id)[1].contains("from develop"));
    }

    #[tokio::test]
    async fn two_workspaces_run_side_by_side() {
        let fx = fixture().await;
        let (a, b, c) = tokio::join!(
            fx.svc.create(&fx.project.id, None),
            fx.svc.create(&fx.project.id, None),
            fx.svc.create(&fx.project.id, None),
        );
        let all = [a.unwrap(), b.unwrap(), c.unwrap()];
        let cities: HashSet<_> = all.iter().map(|w| w.city.clone()).collect();
        let branches: HashSet<_> = all.iter().map(|w| w.branch.clone()).collect();
        let paths: HashSet<_> = all.iter().map(|w| w.path.clone()).collect();
        assert_eq!((cities.len(), branches.len(), paths.len()), (3, 3, 3));
        for ws in &all {
            assert_eq!(ws.state, WorkspaceState::Ready);
            let status = fx
                .svc
                .projects
                .git
                .status(Path::new(&ws.path))
                .await
                .unwrap();
            assert_eq!(status.branch.as_deref(), Some(ws.branch.as_str()));
        }
        assert_eq!(
            fx.svc
                .projects
                .git
                .worktree_list(&fx.repo)
                .await
                .unwrap()
                .len(),
            4
        );
    }

    #[tokio::test]
    async fn copies_ignored_files_from_settings_and_worktreeinclude() {
        let fx = fixture().await;
        std::fs::write(
            fx.repo.join(".gitignore"),
            ".env\n.env.local\nnode_modules/\nsecrets/\n",
        )
        .unwrap();
        sh(&fx.repo, &["add", "-A"]);
        sh(&fx.repo, &["commit", "-q", "-m", "ignore files"]);
        std::fs::write(fx.repo.join(".env"), "A=1").unwrap();
        std::fs::write(fx.repo.join(".env.local"), "B=2").unwrap();
        std::fs::create_dir_all(fx.repo.join("node_modules/pkg")).unwrap();
        std::fs::write(fx.repo.join("node_modules/pkg/index.js"), "x").unwrap();
        std::fs::create_dir_all(fx.repo.join("secrets")).unwrap();
        std::fs::write(fx.repo.join("secrets/key.txt"), "k").unwrap();

        // `.env` from the setting, `.env.local` and `secrets/` from `.worktreeinclude`.
        std::fs::write(
            &fx.svc.projects.paths.settings_file,
            "files_to_copy = [\".env\"]\n",
        )
        .unwrap();
        std::fs::write(
            fx.repo.join(".worktreeinclude"),
            "# local files\n.env.local\nsecrets/\n",
        )
        .unwrap();

        let ws = fx.svc.create(&fx.project.id, None).await.unwrap();
        let dir = PathBuf::from(&ws.path);
        assert_eq!(std::fs::read_to_string(dir.join(".env")).unwrap(), "A=1");
        assert_eq!(
            std::fs::read_to_string(dir.join(".env.local")).unwrap(),
            "B=2"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("secrets/key.txt")).unwrap(),
            "k"
        );
        assert!(
            !dir.join("node_modules").exists(),
            "files that match no pattern are not copied"
        );
        assert!(!fx.svc.projects.git.status(&dir).await.unwrap().is_dirty);

        assert!(
            !messages(&fx, &ws.id).iter().any(|m| m.contains("copied")),
            "{:?}",
            messages(&fx, &ws.id)
        );
    }

    #[tokio::test]
    async fn a_missing_base_branch_fails_cleanly_and_keeps_an_errored_record() {
        let fx = fixture().await;
        let err = fx
            .svc
            .create(&fx.project.id, Some("nope"))
            .await
            .unwrap_err();
        assert_eq!(git_kind(&err), Some(GitErrorKind::BranchNotFound));

        let all = fx.svc.list(None, true).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].state, WorkspaceState::Errored);
        assert!(all[0].error.as_deref().unwrap().contains("nope"));
        assert!(!Path::new(&all[0].path).exists());
        assert_eq!(branches(&fx).await, vec!["main"]);
        assert!(messages(&fx, &all[0].id).last().unwrap().contains("nope"));
        assert_eq!(
            fx.sink.changes().last().unwrap().to,
            WorkspaceState::Errored
        );
    }

    #[tokio::test]
    async fn a_failure_after_the_worktree_exists_removes_the_worktree_and_branch() {
        let fx = fixture().await;
        // A tracked file named `.context` makes the `.context/` step fail, which happens after
        // the worktree and branch were created.
        std::fs::write(fx.repo.join(".context"), "not a directory").unwrap();
        sh(&fx.repo, &["add", "-A"]);
        sh(
            &fx.repo,
            &["commit", "-q", "-m", "track a file called .context"],
        );
        fx.svc.create(&fx.project.id, None).await.unwrap_err();

        let ws = &fx.svc.list(None, true).unwrap()[0];
        assert_eq!(ws.state, WorkspaceState::Errored);
        assert!(
            !Path::new(&ws.path).exists(),
            "the worktree folder is removed"
        );
        assert_eq!(
            fx.svc
                .projects
                .git
                .worktree_list(&fx.repo)
                .await
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            branches(&fx).await,
            vec!["main"],
            "the new branch is removed"
        );
        assert_eq!(
            fx.sink.changes().last().unwrap().to,
            WorkspaceState::Errored
        );
    }

    #[tokio::test]
    async fn archive_keeps_the_branch_and_restore_brings_everything_back() {
        let fx = fixture().await;
        std::fs::write(fx.repo.join(".gitignore"), ".env\n").unwrap();
        sh(&fx.repo, &["add", "-A"]);
        sh(&fx.repo, &["commit", "-q", "-m", "ignore"]);
        std::fs::write(fx.repo.join(".env"), "SECRET=1").unwrap();
        std::fs::write(fx.repo.join(".worktreeinclude"), ".env\n").unwrap();

        let ws = fx.svc.create(&fx.project.id, None).await.unwrap();
        let dir = PathBuf::from(&ws.path);
        std::fs::write(dir.join(".context/notes.md"), "remember this").unwrap();

        let archived = fx.svc.archive(&ws.id, false).await.unwrap();
        assert_eq!(archived.state, WorkspaceState::Archived);
        assert!(archived.archived_at.is_some());
        assert!(!dir.exists(), "the worktree is removed");
        assert!(
            branches(&fx).await.contains(&ws.branch),
            "the branch is kept"
        );
        assert_eq!(
            fx.svc
                .projects
                .git
                .worktree_list(&fx.repo)
                .await
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            fx.svc.projects.get(&fx.project.id).unwrap().workspace_count,
            0
        );
        assert!(fx.svc.list(None, false).unwrap().is_empty());
        assert_eq!(fx.svc.list(None, true).unwrap().len(), 1);

        let restored = fx.svc.restore(&ws.id).await.unwrap();
        assert_eq!(restored.state, WorkspaceState::Ready);
        assert_eq!(restored.path, ws.path);
        assert_eq!(restored.branch, ws.branch);
        assert!(restored.archived_at.is_none());
        assert_eq!(
            std::fs::read_to_string(dir.join(".context/notes.md")).unwrap(),
            "remember this"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join(".env")).unwrap(),
            "SECRET=1"
        );
        assert!(
            !fx.svc
                .projects
                .paths
                .data_dir
                .join("context")
                .join(&ws.id)
                .exists(),
            "the backup is consumed"
        );
        let status = fx.svc.projects.git.status(&dir).await.unwrap();
        assert_eq!(status.branch.as_deref(), Some(ws.branch.as_str()));
        assert!(!status.is_dirty);
    }

    #[tokio::test]
    async fn archiving_dirty_work_asks_first_and_never_loses_it() {
        let fx = fixture().await;
        let ws = fx.svc.create(&fx.project.id, None).await.unwrap();
        let dir = PathBuf::from(&ws.path);
        std::fs::write(dir.join("wip.txt"), "unsaved work").unwrap();
        std::fs::write(dir.join("README.md"), "# edited\n").unwrap();

        let err = fx.svc.archive(&ws.id, false).await.unwrap_err();
        assert_eq!(git_kind(&err), Some(GitErrorKind::DirtyTree));
        assert!(err.to_string().contains("2 uncommitted changes"), "{err}");
        assert_eq!(fx.svc.get(&ws.id).unwrap().state, WorkspaceState::Ready);
        assert_eq!(
            std::fs::read_to_string(dir.join("wip.txt")).unwrap(),
            "unsaved work"
        );

        let archived = fx.svc.archive(&ws.id, true).await.unwrap();
        assert_eq!(archived.state, WorkspaceState::Archived);
        let log = fx
            .svc
            .projects
            .git
            .log(&fx.repo, &ws.branch, 1)
            .await
            .unwrap();
        assert_eq!(log[0].subject, "WIP: archived from Circles");

        fx.svc.restore(&ws.id).await.unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("wip.txt")).unwrap(),
            "unsaved work"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("README.md")).unwrap(),
            "# edited\n"
        );
    }

    #[tokio::test]
    async fn busy_and_provisioning_workspaces_cannot_be_archived() {
        let fx = fixture().await;
        let ws = fx.svc.create(&fx.project.id, None).await.unwrap();
        fx.svc
            .set_state(&ws.id, WorkspaceState::Working, None)
            .unwrap();
        let err = fx.svc.archive(&ws.id, true).await.unwrap_err();
        assert_eq!(err.code(), "invalid");
        assert!(err.to_string().contains("Stop the agent"));
        assert!(Path::new(&ws.path).exists());

        fx.svc
            .set_state(&ws.id, WorkspaceState::Idle, None)
            .unwrap();
        assert_eq!(
            fx.svc.archive(&ws.id, true).await.unwrap().state,
            WorkspaceState::Archived
        );
        // Archiving twice is harmless.
        assert_eq!(
            fx.svc.archive(&ws.id, true).await.unwrap().state,
            WorkspaceState::Archived
        );
    }

    #[tokio::test]
    async fn invalid_state_changes_are_rejected() {
        let fx = fixture().await;
        let ws = fx.svc.create(&fx.project.id, None).await.unwrap();
        let err = fx
            .svc
            .set_state(&ws.id, WorkspaceState::Creating, None)
            .unwrap_err();
        assert_eq!(err.code(), "invalid");
        assert_eq!(fx.svc.get(&ws.id).unwrap().state, WorkspaceState::Ready);
        // Same state is a quiet no-op and emits nothing.
        let before = fx.sink.changes().len();
        fx.svc
            .set_state(&ws.id, WorkspaceState::Ready, None)
            .unwrap();
        assert_eq!(fx.sink.changes().len(), before);
    }

    #[tokio::test]
    async fn restore_reports_a_deleted_branch() {
        let fx = fixture().await;
        let ws = fx.svc.create(&fx.project.id, None).await.unwrap();
        fx.svc.archive(&ws.id, true).await.unwrap();
        sh(&fx.repo, &["branch", "-D", &ws.branch]);

        let err = fx.svc.restore(&ws.id).await.unwrap_err();
        assert_eq!(git_kind(&err), Some(GitErrorKind::BranchNotFound));
        let after = fx.svc.get(&ws.id).unwrap();
        assert_eq!(after.state, WorkspaceState::Errored);
        assert!(after.error.unwrap().contains("no longer exists"));
    }

    #[tokio::test]
    async fn restore_reports_a_branch_checked_out_elsewhere() {
        let fx = fixture().await;
        let ws = fx.svc.create(&fx.project.id, None).await.unwrap();
        fx.svc.archive(&ws.id, true).await.unwrap();
        let other = fx.root.join("elsewhere");
        fx.svc
            .projects
            .git
            .worktree_add_existing(&fx.repo, &other, &ws.branch)
            .await
            .unwrap();

        let err = fx.svc.restore(&ws.id).await.unwrap_err();
        assert_eq!(git_kind(&err), Some(GitErrorKind::BranchCheckedOut));
        assert_eq!(fx.svc.get(&ws.id).unwrap().state, WorkspaceState::Errored);
    }

    #[tokio::test]
    async fn renames_the_branch() {
        let fx = fixture().await;
        let ws = fx.svc.create(&fx.project.id, None).await.unwrap();
        let renamed = fx
            .svc
            .rename_branch(&ws.id, "  tester/fix-login  ")
            .await
            .unwrap();
        assert_eq!(renamed.branch, "tester/fix-login");
        let status = fx
            .svc
            .projects
            .git
            .status(Path::new(&ws.path))
            .await
            .unwrap();
        assert_eq!(status.branch.as_deref(), Some("tester/fix-login"));
        assert!(!branches(&fx).await.contains(&ws.branch));

        for bad in ["", "tester/fix-login", "bad..name", "has space"] {
            assert_eq!(
                fx.svc.rename_branch(&ws.id, bad).await.unwrap_err().code(),
                "invalid",
                "{bad:?}"
            );
        }
        let taken = fx.svc.rename_branch(&ws.id, "main").await.unwrap_err();
        assert_eq!(git_kind(&taken), Some(GitErrorKind::BranchExists));
        assert_eq!(fx.svc.get(&ws.id).unwrap().branch, "tester/fix-login");
    }

    #[tokio::test]
    async fn pin_title_and_open_time() {
        let fx = fixture().await;
        let ws = fx.svc.create(&fx.project.id, None).await.unwrap();
        assert!(!ws.pinned && ws.title.is_none() && ws.last_opened_at.is_none());
        assert!(fx.svc.set_pinned(&ws.id, true).unwrap().pinned);
        assert!(!fx.svc.set_pinned(&ws.id, false).unwrap().pinned);
        assert_eq!(
            fx.svc
                .set_title(&ws.id, Some("  Fix login "))
                .unwrap()
                .title
                .as_deref(),
            Some("Fix login")
        );
        assert_eq!(fx.svc.set_title(&ws.id, Some("  ")).unwrap().title, None);
        fx.svc.touch_opened(&ws.id).unwrap();
        assert!(fx.svc.get(&ws.id).unwrap().last_opened_at.is_some());
        assert_eq!(
            fx.svc.touch_opened("missing").unwrap_err().code(),
            "not_found"
        );
    }

    #[tokio::test]
    async fn delete_only_applies_to_archived_workspaces_and_keeps_the_branch() {
        let fx = fixture().await;
        let ws = fx.svc.create(&fx.project.id, None).await.unwrap();
        let err = fx.svc.delete(&ws.id, false).await.unwrap_err();
        assert_eq!(err.code(), "invalid");
        assert!(Path::new(&ws.path).exists());

        fx.svc.archive(&ws.id, true).await.unwrap();
        fx.svc.delete(&ws.id, false).await.unwrap();
        assert_eq!(fx.svc.get(&ws.id).unwrap_err().code(), "not_found");
        assert!(
            branches(&fx).await.contains(&ws.branch),
            "the branch survives by default"
        );

        let second = fx.svc.create(&fx.project.id, None).await.unwrap();
        fx.svc.archive(&second.id, true).await.unwrap();
        fx.svc.delete(&second.id, true).await.unwrap();
        assert!(
            !branches(&fx).await.contains(&second.branch),
            "the branch is removed on request"
        );
    }

    #[tokio::test]
    async fn purge_deletes_a_live_workspace_with_uncommitted_work_and_keeps_its_branch() {
        let fx = fixture().await;
        let ws = fx.svc.create(&fx.project.id, None).await.unwrap();
        std::fs::write(Path::new(&ws.path).join("scratch.txt"), "not committed").unwrap();
        fx.svc.purge(&ws.id).await.unwrap();
        assert_eq!(fx.svc.get(&ws.id).unwrap_err().code(), "not_found");
        assert!(!Path::new(&ws.path).exists(), "the folder is gone");
        assert!(
            branches(&fx).await.contains(&ws.branch),
            "the branch is kept"
        );
    }

    #[tokio::test]
    async fn release_forgets_a_workspace_and_leaves_its_folder_working() {
        let fx = fixture().await;
        let ws = fx.svc.create(&fx.project.id, None).await.unwrap();
        std::fs::write(Path::new(&ws.path).join("keep.txt"), "mine").unwrap();
        fx.svc.release(&ws.id).await.unwrap();
        assert_eq!(fx.svc.get(&ws.id).unwrap_err().code(), "not_found");
        let folder = Path::new(&ws.path);
        assert_eq!(
            std::fs::read_to_string(folder.join("keep.txt")).unwrap(),
            "mine"
        );
        // Still a working copy of the repository.
        let status = fx.svc.projects.git.status(folder).await.unwrap();
        assert!(status.is_dirty);
    }

    #[tokio::test]
    async fn a_project_goes_with_all_its_workspaces_when_asked() {
        let fx = fixture().await;
        let live = fx.svc.create(&fx.project.id, None).await.unwrap();
        let old = fx.svc.create(&fx.project.id, None).await.unwrap();
        fx.svc.archive(&old.id, true).await.unwrap();
        // The plain removal is still refused while workspaces exist.
        assert!(fx.svc.projects.remove(&fx.project.id).is_err());
        for ws in fx.svc.list(Some(&fx.project.id), true).unwrap() {
            fx.svc.purge(&ws.id).await.unwrap();
        }
        fx.svc
            .projects
            .remove_with_workspaces(&fx.project.id)
            .unwrap();
        assert_eq!(
            fx.svc.projects.get(&fx.project.id).unwrap_err().code(),
            "not_found"
        );
        assert!(!Path::new(&live.path).exists());
        assert!(
            Path::new(&fx.project.path).exists(),
            "the repository is never touched"
        );
    }

    #[tokio::test]
    async fn deleting_the_last_workspace_removes_the_empty_project_folder() {
        let fx = fixture().await;
        let a = fx.svc.create(&fx.project.id, None).await.unwrap();
        let b = fx.svc.create(&fx.project.id, None).await.unwrap();
        let project_dir = Path::new(&a.path).parent().unwrap().to_path_buf();

        fx.svc.archive(&a.id, true).await.unwrap();
        fx.svc.delete(&a.id, false).await.unwrap();
        assert!(project_dir.exists(), "another workspace still lives there");
        assert!(Path::new(&b.path).exists());

        fx.svc.archive(&b.id, true).await.unwrap();
        fx.svc.delete(&b.id, false).await.unwrap();
        assert!(!project_dir.exists(), "the empty folder is removed");
        assert!(
            fx.svc.projects.paths.workspaces_dir.exists(),
            "the workspaces root stays"
        );
    }

    #[tokio::test]
    async fn an_errored_workspace_can_be_deleted() {
        let fx = fixture().await;
        let _ = fx
            .svc
            .create(&fx.project.id, Some("nope"))
            .await
            .unwrap_err();
        let ws = fx.svc.list(None, true).unwrap().remove(0);
        fx.svc.delete(&ws.id, false).await.unwrap();
        assert!(fx.svc.list(None, true).unwrap().is_empty());
    }

    #[tokio::test]
    async fn reconcile_marks_missing_worktrees_and_restore_repairs_them() {
        let fx = fixture().await;
        let ws = fx.svc.create(&fx.project.id, None).await.unwrap();
        std::fs::remove_dir_all(&ws.path).unwrap();

        let report = fx.svc.reconcile().await.unwrap();
        assert_eq!(report.errored, 1);
        let broken = fx.svc.get(&ws.id).unwrap();
        assert_eq!(broken.state, WorkspaceState::Errored);
        assert_eq!(
            broken.error.as_deref(),
            Some("The worktree folder is missing.")
        );
        assert_eq!(
            fx.svc
                .projects
                .git
                .worktree_list(&fx.repo)
                .await
                .unwrap()
                .len(),
            1,
            "dangling registration is pruned"
        );

        let fixed = fx.svc.restore(&ws.id).await.unwrap();
        assert_eq!(fixed.state, WorkspaceState::Ready);
        assert!(Path::new(&ws.path).join("README.md").exists());
        assert_eq!(
            fx.svc.reconcile().await.unwrap(),
            ReconcileReport::default()
        );
    }

    #[tokio::test]
    async fn reconcile_handles_interrupted_and_stale_states() {
        let fx = fixture().await;
        let a = fx.svc.create(&fx.project.id, None).await.unwrap();
        let b = fx.svc.create(&fx.project.id, None).await.unwrap();
        let c = fx.svc.create(&fx.project.id, None).await.unwrap();
        let set = |id: &str, state: &str| {
            fx.svc
                .projects
                .db
                .write(|conn| {
                    conn.execute(
                        "UPDATE workspaces SET state = ?1 WHERE id = ?2",
                        params![state, id],
                    )
                })
                .unwrap();
        };
        set(&a.id, "creating");
        set(&b.id, "working");
        set(&c.id, "errored");

        let report = fx.svc.reconcile().await.unwrap();
        assert_eq!(
            report,
            ReconcileReport {
                errored: 1,
                recovered: 1,
                reset: 1
            }
        );
        assert_eq!(fx.svc.get(&a.id).unwrap().state, WorkspaceState::Errored);
        assert!(fx
            .svc
            .get(&a.id)
            .unwrap()
            .error
            .unwrap()
            .contains("interrupted"));
        assert_eq!(fx.svc.get(&b.id).unwrap().state, WorkspaceState::Idle);
        assert_eq!(fx.svc.get(&c.id).unwrap().state, WorkspaceState::Ready);
    }

    #[tokio::test]
    async fn branch_prefix_uses_the_cached_login_then_git_then_a_default() {
        let fx = fixture().await;
        assert_eq!(fx.svc.branch_prefix(&fx.repo).await, "tester");

        fx.svc
            .projects
            .db
            .write(|c| c.execute("DELETE FROM kv", []))
            .unwrap();
        assert_eq!(fx.svc.branch_prefix(&fx.repo).await, "circles");

        sh(&fx.repo, &["config", "user.name", "Marlon Martin"]);
        assert_eq!(fx.svc.branch_prefix(&fx.repo).await, "marlon-martin");
    }

    #[tokio::test]
    async fn a_branch_prefix_in_the_settings_replaces_the_usual_one() {
        let fx = fixture().await;
        assert_eq!(
            fx.svc.branch_prefix_for(&fx.project, &fx.repo).await,
            "tester"
        );

        let file = crate::paths::project_settings_file(&fx.repo);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "[git]\nbranch_prefix = \"Team Alpha\"\n").unwrap();
        assert_eq!(
            fx.svc.branch_prefix_for(&fx.project, &fx.repo).await,
            "team-alpha"
        );

        std::fs::write(&file, "[git]\nbranch_prefix = \"  \"\n").unwrap();
        assert_eq!(
            fx.svc.branch_prefix_for(&fx.project, &fx.repo).await,
            "tester",
            "blank means not set"
        );

        std::fs::write(&file, "this is = not [valid").unwrap();
        assert_eq!(
            fx.svc.branch_prefix_for(&fx.project, &fx.repo).await,
            "tester",
            "a broken file never stops a workspace"
        );
    }

    #[tokio::test]
    async fn sections_group_and_reorder_workspaces() {
        let fx = fixture().await;
        let ws = fx.svc.create(&fx.project.id, None).await.unwrap();
        let (a, b, c) = (
            fx.svc.create_section("  Work ").unwrap(),
            fx.svc.create_section("Side projects").unwrap(),
            fx.svc.create_section("Later").unwrap(),
        );
        assert_eq!(a.name, "Work");
        assert_eq!([a.position, b.position, c.position], [0, 1, 2]);
        assert_eq!(fx.svc.create_section("   ").unwrap_err().code(), "invalid");
        assert_eq!(
            fx.svc.create_section(&"x".repeat(61)).unwrap_err().code(),
            "invalid"
        );

        assert_eq!(fx.svc.rename_section(&b.id, "Hobby").unwrap().name, "Hobby");
        let order = fx
            .svc
            .reorder_sections(&[c.id.clone(), a.id.clone(), b.id.clone()])
            .unwrap();
        assert_eq!(
            order.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            ["Later", "Work", "Hobby"]
        );
        assert_eq!(
            fx.svc
                .reorder_sections(std::slice::from_ref(&a.id))
                .unwrap_err()
                .code(),
            "invalid"
        );
        assert_eq!(
            fx.svc
                .reorder_sections(&[a.id.clone(), a.id.clone(), b.id.clone()])
                .unwrap_err()
                .code(),
            "invalid"
        );

        assert_eq!(
            fx.svc
                .set_section(&ws.id, Some(&a.id))
                .unwrap()
                .section_id
                .as_deref(),
            Some(a.id.as_str())
        );
        assert_eq!(
            fx.svc
                .set_section(&ws.id, Some("missing"))
                .unwrap_err()
                .code(),
            "not_found"
        );
        fx.svc.delete_section(&a.id).unwrap();
        assert_eq!(
            fx.svc.get(&ws.id).unwrap().section_id,
            None,
            "deleting a section keeps its workspaces"
        );
        assert_eq!(fx.svc.list_sections().unwrap().len(), 2);
    }

    fn opts(source: CreateSource) -> CreateOptions {
        CreateOptions {
            source,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn stores_the_first_prompt_model_and_effort() {
        let fx = fixture().await;
        let ws = fx
            .svc
            .create_with(
                &fx.project.id,
                CreateOptions {
                    prompt: Some("  Add a dark mode toggle  ".into()),
                    model: Some("opus".into()),
                    effort: Some("high".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(ws.pending_prompt.as_deref(), Some("Add a dark mode toggle"));
        assert_eq!(ws.pending_model.as_deref(), Some("opus"));
        assert_eq!(ws.pending_effort.as_deref(), Some("high"));
        assert_eq!(fx.svc.get(&ws.id).unwrap(), ws);

        let blank = fx
            .svc
            .create_with(
                &fx.project.id,
                CreateOptions {
                    prompt: Some("   ".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(blank.pending_prompt, None);
        assert_eq!(blank.pending_model, None);
    }

    #[tokio::test]
    async fn emits_progress_stages_in_order() {
        let fx = fixture().await;
        fx.svc.create(&fx.project.id, None).await.unwrap();
        assert_eq!(
            fx.sink.stages(),
            [
                "fetching",
                "creating_worktree",
                "copying_files",
                "finishing"
            ]
        );
    }

    #[tokio::test]
    async fn creates_from_an_existing_local_branch() {
        let fx = fixture().await;
        sh(&fx.repo, &["branch", "feature/login"]);
        let ws = fx
            .svc
            .create_with(
                &fx.project.id,
                opts(CreateSource::ExistingBranch {
                    name: " feature/login ".into(),
                }),
            )
            .await
            .unwrap();

        assert_eq!(ws.state, WorkspaceState::Ready);
        assert_eq!(ws.branch, "feature/login");
        assert_eq!(ws.base_branch, "main");
        let status = fx
            .svc
            .projects
            .git
            .status(Path::new(&ws.path))
            .await
            .unwrap();
        assert_eq!(status.branch.as_deref(), Some("feature/login"));
        assert_eq!(
            messages(&fx, &ws.id)[1],
            "Checked out the existing branch feature/login"
        );
        assert!(fx
            .svc
            .projects
            .git
            .branch_exists(&fx.repo, "feature/login")
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn a_failed_checkout_never_deletes_a_branch_that_already_existed() {
        let fx = fixture().await;
        sh(&fx.repo, &["branch", "precious"]);
        // A tracked file named `.context` makes the `.context/` step fail after the worktree exists.
        std::fs::write(fx.repo.join(".context"), "not a directory").unwrap();
        sh(&fx.repo, &["add", "-A"]);
        sh(
            &fx.repo,
            &["commit", "-q", "-m", "track a file called .context"],
        );
        sh(&fx.repo, &["branch", "-f", "precious", "main"]);

        fx.svc
            .create_with(
                &fx.project.id,
                opts(CreateSource::ExistingBranch {
                    name: "precious".into(),
                }),
            )
            .await
            .unwrap_err();

        assert_eq!(
            fx.svc.list(None, true).unwrap()[0].state,
            WorkspaceState::Errored
        );
        assert!(
            branches(&fx).await.contains(&"precious".to_string()),
            "the user's branch survives"
        );
        assert_eq!(
            fx.svc
                .projects
                .git
                .worktree_list(&fx.repo)
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn creates_from_a_branch_that_only_exists_on_origin() {
        let fx = fixture_with_origin().await;
        sh(&fx.root, &["clone", "-q", "origin.git", "other"]);
        let other = fx.root.join("other");
        sh(&other, &["checkout", "-q", "-b", "remote-feature"]);
        std::fs::write(other.join("remote.txt"), "from a teammate").unwrap();
        sh(&other, &["add", "-A"]);
        sh(&other, &["commit", "-q", "-m", "teammate work"]);
        sh(&other, &["push", "-q", "origin", "remote-feature"]);
        assert!(!branches(&fx).await.contains(&"remote-feature".to_string()));

        let ws = fx
            .svc
            .create_with(
                &fx.project.id,
                opts(CreateSource::ExistingBranch {
                    name: "remote-feature".into(),
                }),
            )
            .await
            .unwrap();

        assert_eq!(ws.state, WorkspaceState::Ready);
        assert_eq!(
            std::fs::read_to_string(Path::new(&ws.path).join("remote.txt")).unwrap(),
            "from a teammate"
        );
        let status = fx
            .svc
            .projects
            .git
            .status(Path::new(&ws.path))
            .await
            .unwrap();
        assert_eq!(status.branch.as_deref(), Some("remote-feature"));
        assert_eq!(
            status.upstream.as_deref(),
            Some("origin/remote-feature"),
            "it tracks the remote branch"
        );
        assert_eq!(
            messages(&fx, &ws.id)[1],
            "Checked out remote-feature from origin/remote-feature"
        );
    }

    #[tokio::test]
    async fn a_branch_in_use_elsewhere_creates_no_workspace() {
        let fx = fixture().await;
        // `main` is checked out in the repository itself.
        let err = fx
            .svc
            .create_with(
                &fx.project.id,
                opts(CreateSource::ExistingBranch {
                    name: "main".into(),
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(git_kind(&err), Some(GitErrorKind::BranchCheckedOut));
        assert!(err.to_string().contains(fx.repo.to_str().unwrap()), "{err}");
        assert!(
            fx.svc.list(None, true).unwrap().is_empty(),
            "no record is left behind"
        );

        // The branch of another workspace is in use too.
        let other = fx.svc.create(&fx.project.id, None).await.unwrap();
        let err = fx
            .svc
            .create_with(
                &fx.project.id,
                opts(CreateSource::ExistingBranch {
                    name: other.branch.clone(),
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(git_kind(&err), Some(GitErrorKind::BranchCheckedOut));
        assert!(err.to_string().contains(&other.path));
        assert_eq!(fx.svc.list(None, true).unwrap().len(), 1);
    }

    #[tokio::test]
    async fn an_unknown_existing_branch_is_reported_before_anything_is_created() {
        let fx = fixture().await;
        for name in ["does-not-exist", "  "] {
            let err = fx
                .svc
                .create_with(
                    &fx.project.id,
                    opts(CreateSource::ExistingBranch { name: name.into() }),
                )
                .await
                .unwrap_err();
            assert!(
                git_kind(&err) == Some(GitErrorKind::BranchNotFound) || err.code() == "invalid",
                "{err}"
            );
        }
        assert!(fx.svc.list(None, true).unwrap().is_empty());
    }

    #[tokio::test]
    async fn attaches_files_and_reports_the_ones_it_cannot() {
        let fx = fixture().await;
        let notes = fx.root.join("notes.md");
        std::fs::write(&notes, "# spec").unwrap();
        let ws = fx
            .svc
            .create_with(
                &fx.project.id,
                CreateOptions {
                    attachments: vec![
                        notes.to_string_lossy().into_owned(),
                        fx.root.join("missing.png").to_string_lossy().into_owned(),
                    ],
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        assert_eq!(
            ws.state,
            WorkspaceState::Ready,
            "a bad attachment never fails the creation"
        );
        let attached = Path::new(&ws.path).join(".context/attachments/notes.md");
        assert_eq!(std::fs::read_to_string(attached).unwrap(), "# spec");
        let lines = messages(&fx, &ws.id);
        assert!(lines.iter().any(|m| m == "Attached 1 file"), "{lines:?}");
        assert!(
            lines
                .iter()
                .any(|m| m.starts_with("Could not attach missing.png")),
            "{lines:?}"
        );
        assert!(
            !fx.svc
                .projects
                .git
                .status(Path::new(&ws.path))
                .await
                .unwrap()
                .is_dirty
        );
    }

    #[tokio::test]
    async fn lists_branches_with_where_they_are_checked_out() {
        let fx = fixture_with_origin().await;
        sh(&fx.root, &["clone", "-q", "origin.git", "other"]);
        let other = fx.root.join("other");
        sh(&other, &["checkout", "-q", "-b", "remote-only"]);
        std::fs::write(other.join("x"), "x").unwrap();
        sh(&other, &["add", "-A"]);
        sh(&other, &["commit", "-q", "-m", "x"]);
        sh(&other, &["push", "-q", "origin", "remote-only"]);
        sh(&fx.repo, &["fetch", "-q", "origin"]);
        sh(&fx.repo, &["branch", "local-only"]);
        let ws = fx.svc.create(&fx.project.id, None).await.unwrap();

        let list = fx.svc.list_branches(&fx.project.id).await.unwrap();
        let find = |name: &str| {
            list.iter()
                .find(|b| b.name == name)
                .unwrap_or_else(|| panic!("{name} missing"))
        };

        let main = find("main");
        assert!(main.local && main.remote);
        assert_eq!(
            main.checked_out_at.as_deref(),
            Some(fx.repo.to_str().unwrap())
        );
        let remote_only = find("remote-only");
        assert!(!remote_only.local && remote_only.remote && remote_only.checked_out_at.is_none());
        let local_only = find("local-only");
        assert!(local_only.local && !local_only.remote);
        assert_eq!(
            find(&ws.branch).checked_out_at.as_deref(),
            Some(ws.path.as_str())
        );
        assert!(list.iter().all(|b| b.name != "HEAD"));
    }

    /// Creates workspaces from a pull request with the real gh, against a public repository.
    /// Run with `cargo test --lib -- --ignored --nocapture real_pull_request`.
    #[tokio::test]
    #[ignore = "needs a signed in gh and network access"]
    async fn real_pull_request_checkout() {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let paths = AppPaths::from_roots(&root.join("data"), &root.join("home"));
        paths.ensure().unwrap();
        let env = Arc::new(crate::env::resolve().await);
        let projects = ProjectService {
            db: Arc::new(Db::open(&paths.db_file).unwrap()),
            git: crate::git::Git::new(env.clone()),
            gh: Gh::new(env),
            paths,
        };
        let terminals = Terminals::new(
            projects.db.clone(),
            projects.paths.clone(),
            Arc::new(crate::terminal::NullNotifier),
        );
        let svc = WorkspaceService {
            projects,
            sink: CollectSink::new(),
            locks: Arc::new(Locks::default()),
            terminals,
            env: Arc::new(crate::env::resolve().await),
        };
        let project = svc
            .projects
            .add_github("octocat/Hello-World")
            .await
            .unwrap();

        let prs = svc.list_pull_requests(&project.id).await.unwrap();
        println!(
            "{} open pull requests, first: {:?}",
            prs.len(),
            prs.first()
                .map(|p| (p.number, p.title.clone(), p.is_cross_repository))
        );
        let branches = svc.list_branches(&project.id).await.unwrap();
        println!("{} branches", branches.len());

        let number = prs.first().map(|p| p.number).unwrap_or(1);
        let ws = svc
            .create_with(&project.id, opts(CreateSource::PullRequest { number }))
            .await
            .unwrap();
        println!(
            "workspace {} on {} title={:?} state={:?}",
            ws.city, ws.branch, ws.title, ws.state
        );
        for e in svc.events(&ws.id).unwrap() {
            println!("   [{}] {}", e.kind, e.message);
        }
        assert_eq!(ws.state, WorkspaceState::Ready);
        assert!(
            Path::new(&ws.path).join("README").exists()
                || Path::new(&ws.path).read_dir().unwrap().count() > 1
        );
    }

    /// Runs the whole lifecycle against the real git and gh with a real GitHub clone, in scratch
    /// folders. Run with `cargo test --lib -- --ignored --nocapture real_workspace_walkthrough`.
    #[tokio::test]
    #[ignore = "needs a signed in gh and network access"]
    async fn real_workspace_walkthrough() {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let paths = AppPaths::from_roots(&root.join("data"), &root.join("home"));
        paths.ensure().unwrap();
        let env = Arc::new(crate::env::resolve().await);
        let projects = ProjectService {
            db: Arc::new(Db::open(&paths.db_file).unwrap()),
            git: crate::git::Git::new(env.clone()),
            gh: Gh::new(env),
            paths,
        };
        let sink = CollectSink::new();
        let terminals = Terminals::new(
            projects.db.clone(),
            projects.paths.clone(),
            Arc::new(crate::terminal::NullNotifier),
        );
        let svc = WorkspaceService {
            projects,
            sink: sink.clone(),
            locks: Arc::new(Locks::default()),
            terminals,
            env: Arc::new(crate::env::resolve().await),
        };

        let project = svc
            .projects
            .add_github("decimozs/neovim-config")
            .await
            .unwrap();
        println!(
            "project {} at {} base={}",
            project.name, project.path, project.default_branch
        );

        println!("-- create two workspaces (real fetch, real gh login for the branch prefix)");
        let a = svc.create(&project.id, None).await.unwrap();
        let b = svc.create(&project.id, None).await.unwrap();
        for w in [&a, &b] {
            println!("   {} branch={} state={:?}", w.city, w.branch, w.state);
            for e in svc.events(&w.id).unwrap() {
                println!("      [{}] {}", e.kind, e.message);
            }
        }
        assert!(a.branch.starts_with("decimozs/"));

        println!("-- dirty archive asks first");
        std::fs::write(Path::new(&a.path).join("notes.txt"), "work in progress").unwrap();
        let err = svc.archive(&a.id, false).await.unwrap_err();
        println!("   refused: {err}");
        let archived = svc.archive(&a.id, true).await.unwrap();
        println!(
            "   archived: state={:?} folder exists={}",
            archived.state,
            Path::new(&a.path).exists()
        );
        let restored = svc.restore(&a.id).await.unwrap();
        println!(
            "   restored: state={:?} notes.txt={:?}",
            restored.state,
            std::fs::read_to_string(Path::new(&a.path).join("notes.txt")).unwrap()
        );

        println!("-- rename, pin, reconcile after the folder is deleted by hand");
        let renamed = svc
            .rename_branch(&b.id, "decimozs/my-feature")
            .await
            .unwrap();
        println!("   renamed to {}", renamed.branch);
        svc.set_pinned(&b.id, true).unwrap();
        std::fs::remove_dir_all(&b.path).unwrap();
        let report = svc.reconcile().await.unwrap();
        println!(
            "   reconcile: {report:?} -> state={:?} error={:?}",
            svc.get(&b.id).unwrap().state,
            svc.get(&b.id).unwrap().error
        );
        let fixed = svc.restore(&b.id).await.unwrap();
        println!(
            "   restored from errored: state={:?} branch={}",
            fixed.state, fixed.branch
        );

        println!("-- delete");
        svc.archive(&b.id, true).await.unwrap();
        svc.delete(&b.id, true).await.unwrap();
        println!(
            "   remaining workspaces: {:?}",
            svc.list(None, true)
                .unwrap()
                .iter()
                .map(|w| w.city.clone())
                .collect::<Vec<_>>()
        );
        println!("   state events: {}", sink.changes().len());
        println!(
            "   worktrees on disk: {:?}",
            svc.projects
                .git
                .worktree_list(Path::new(&project.path))
                .await
                .unwrap()
                .iter()
                .map(|w| w.branch.clone())
                .collect::<Vec<_>>()
        );
    }

    fn write_settings(fx: &Fx, toml: &str) {
        std::fs::write(&fx.svc.projects.paths.settings_file, toml).unwrap();
    }

    async fn until_state(fx: &Fx, id: &str, want: WorkspaceState) {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            let state = fx.svc.get(id).unwrap().state;
            if state == want {
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "timed out waiting for {want:?}, still {state:?}"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    #[tokio::test]
    async fn every_workspace_gets_its_own_port_block_that_is_released_on_archive() {
        let fx = fixture().await;
        let a = fx.svc.create(&fx.project.id, None).await.unwrap();
        let b = fx.svc.create(&fx.project.id, None).await.unwrap();
        let pa = ports::get(&fx.svc.projects.db, &a.id)
            .unwrap()
            .expect("a has ports");
        let pb = ports::get(&fx.svc.projects.db, &b.id)
            .unwrap()
            .expect("b has ports");
        assert!(pa >= ports::FIRST_PORT && pb >= ports::FIRST_PORT);
        assert!(
            pa.abs_diff(pb) >= ports::BLOCK_SIZE,
            "blocks do not overlap"
        );

        let (ctx, _) = fx.svc.context_for(&a.id).unwrap();
        let vars: std::collections::BTreeMap<_, _> = ctx.env.into_iter().collect();
        assert_eq!(vars["CIRCLES_PORT"], pa.to_string());
        assert_eq!(vars["CIRCLES_WORKSPACE_PATH"], a.path);
        assert_eq!(vars["CIRCLES_ROOT_PATH"], fx.project.path);

        fx.svc.archive(&a.id, true).await.unwrap();
        assert_eq!(
            ports::get(&fx.svc.projects.db, &a.id).unwrap(),
            None,
            "released on archive"
        );
        assert_eq!(ports::get(&fx.svc.projects.db, &b.id).unwrap(), Some(pb));

        fx.svc.restore(&a.id).await.unwrap();
        assert!(
            ports::get(&fx.svc.projects.db, &a.id).unwrap().is_some(),
            "allocated again on restore"
        );
    }

    #[tokio::test]
    async fn a_workspace_without_ports_gets_them_when_a_terminal_needs_them() {
        let fx = fixture().await;
        let ws = fx.svc.create(&fx.project.id, None).await.unwrap();
        ports::release(&fx.svc.projects.db, &ws.id).unwrap();
        let (ctx, _) = fx.svc.context_for(&ws.id).unwrap();
        assert!(ctx.env.iter().any(|(k, _)| k == "CIRCLES_PORT"));
        assert!(ports::get(&fx.svc.projects.db, &ws.id).unwrap().is_some());
    }

    #[tokio::test]
    async fn only_live_workspaces_have_a_terminal_context() {
        let fx = fixture().await;
        let ws = fx.svc.create(&fx.project.id, None).await.unwrap();
        fx.svc.archive(&ws.id, true).await.unwrap();
        let err = fx.svc.context_for(&ws.id).unwrap_err();
        assert_eq!(err.code(), "invalid");
        assert!(err.to_string().contains("Restore it first"));
    }

    #[tokio::test]
    async fn the_setup_script_runs_in_the_workspace_and_then_it_is_ready() {
        let fx = fixture().await;
        write_settings(
            &fx,
            "[scripts]\nsetup = \"echo $CIRCLES_PORT $MY_VAR > setup-env.txt; echo installing\"\n[environment_variables]\nMY_VAR = \"custom\"\n",
        );
        let ws = fx.svc.create(&fx.project.id, None).await.unwrap();
        assert_eq!(
            ws.state,
            WorkspaceState::SetupRunning,
            "create returns while setup is running"
        );
        until_state(&fx, &ws.id, WorkspaceState::Ready).await;

        let port = ports::get(&fx.svc.projects.db, &ws.id).unwrap().unwrap();
        let written = std::fs::read_to_string(Path::new(&ws.path).join("setup-env.txt")).unwrap();
        assert_eq!(
            written.trim(),
            format!("{port} custom"),
            "ran in the worktree with the workspace environment"
        );

        let lines = messages(&fx, &ws.id);
        assert!(
            lines
                .iter()
                .any(|m| m.starts_with("Running the setup script: echo")),
            "{lines:?}"
        );
        assert!(
            lines.contains(&"The setup script finished".to_string()),
            "{lines:?}"
        );

        let states: Vec<_> = fx.sink.changes().iter().map(|c| c.to).collect();
        assert_eq!(
            states,
            [
                WorkspaceState::Creating,
                WorkspaceState::SetupRunning,
                WorkspaceState::Ready
            ]
        );

        let setup_tab = fx
            .svc
            .terminals
            .list(&ws.id)
            .unwrap()
            .into_iter()
            .find(|t| t.kind == crate::terminal::TerminalKind::Setup)
            .unwrap();
        assert!(setup_tab.has_output, "the Setup tab kept the output");
        assert_eq!(setup_tab.exit.unwrap().code, Some(0));
    }

    #[tokio::test]
    async fn a_failing_setup_script_warns_but_the_workspace_is_still_usable() {
        let fx = fixture().await;
        write_settings(&fx, "[scripts]\nsetup = \"echo broken; exit 3\"\n");
        let ws = fx.svc.create(&fx.project.id, None).await.unwrap();
        until_state(&fx, &ws.id, WorkspaceState::Ready).await;
        let warning = fx
            .svc
            .events(&ws.id)
            .unwrap()
            .into_iter()
            .find(|e| e.kind == "warning")
            .expect("a warning");
        assert!(
            warning.message.contains("exited with code 3"),
            "{}",
            warning.message
        );
        assert!(warning.message.contains("Setup tab"));
        assert!(fx.svc.get(&ws.id).unwrap().error.is_none());
    }

    #[tokio::test]
    async fn a_workspace_cannot_be_archived_while_its_setup_runs() {
        let fx = fixture().await;
        write_settings(&fx, "[scripts]\nsetup = \"sleep 1\"\n");
        let ws = fx.svc.create(&fx.project.id, None).await.unwrap();
        let err = fx.svc.archive(&ws.id, true).await.unwrap_err();
        assert_eq!(err.code(), "invalid");
        assert!(err.to_string().contains("still being set up"));
        until_state(&fx, &ws.id, WorkspaceState::Ready).await;
        fx.svc.archive(&ws.id, true).await.unwrap();
    }

    #[tokio::test]
    async fn restoring_runs_the_setup_script_again() {
        let fx = fixture().await;
        write_settings(&fx, "[scripts]\nsetup = \"echo run >> setup-count.txt\"\n");
        let ws = fx.svc.create(&fx.project.id, None).await.unwrap();
        until_state(&fx, &ws.id, WorkspaceState::Ready).await;
        std::fs::remove_file(Path::new(&ws.path).join("setup-count.txt")).unwrap();
        fx.svc.archive(&ws.id, true).await.unwrap();

        let restored = fx.svc.restore(&ws.id).await.unwrap();
        assert_eq!(restored.state, WorkspaceState::SetupRunning);
        until_state(&fx, &ws.id, WorkspaceState::Ready).await;
        assert_eq!(
            std::fs::read_to_string(Path::new(&ws.path).join("setup-count.txt"))
                .unwrap()
                .trim(),
            "run"
        );
    }

    #[tokio::test]
    async fn a_setup_script_cut_short_by_a_restart_leaves_a_ready_workspace_with_a_warning() {
        let fx = fixture().await;
        let ws = fx.svc.create(&fx.project.id, None).await.unwrap();
        fx.svc
            .projects
            .db
            .write(|c| {
                c.execute(
                    "UPDATE workspaces SET state = 'setup_running' WHERE id = ?1",
                    params![ws.id],
                )
            })
            .unwrap();

        let report = fx.svc.reconcile().await.unwrap();
        assert_eq!(report.recovered, 1);
        assert_eq!(fx.svc.get(&ws.id).unwrap().state, WorkspaceState::Ready);
        assert!(messages(&fx, &ws.id)
            .iter()
            .any(|m| m.contains("Setup was interrupted")));
    }

    #[tokio::test]
    async fn the_archive_script_runs_before_the_worktree_goes_away() {
        let fx = fixture().await;
        let marker = fx.root.join("archive-marker.txt");
        write_settings(
            &fx,
            &format!(
                "[scripts]\narchive = \"echo $CIRCLES_PORT $CIRCLES_WORKSPACE_NAME > {} && test -d \\\"$CIRCLES_WORKSPACE_PATH\\\"\"\n",
                marker.display()
            ),
        );
        let ws = fx.svc.create(&fx.project.id, None).await.unwrap();
        let port = ports::get(&fx.svc.projects.db, &ws.id).unwrap().unwrap();

        fx.svc.archive(&ws.id, true).await.unwrap();
        let written = std::fs::read_to_string(&marker).unwrap();
        assert_eq!(
            written.trim(),
            format!("{port} {}", names::city_slug(&ws.city))
        );
        let lines = messages(&fx, &ws.id);
        assert!(
            lines
                .iter()
                .any(|m| m.starts_with("Running the archive script")),
            "{lines:?}"
        );
        assert!(
            lines.contains(&"The archive script finished".to_string()),
            "{lines:?}"
        );
        assert!(!Path::new(&ws.path).exists());
    }

    #[tokio::test]
    async fn a_failing_archive_script_never_blocks_the_archive() {
        let fx = fixture().await;
        write_settings(
            &fx,
            "[scripts]\narchive = \"echo \\\"could not reach the database\\\"; exit 1\"\n",
        );
        let ws = fx.svc.create(&fx.project.id, None).await.unwrap();
        let archived = fx.svc.archive(&ws.id, true).await.unwrap();
        assert_eq!(archived.state, WorkspaceState::Archived);
        let warning = fx
            .svc
            .events(&ws.id)
            .unwrap()
            .into_iter()
            .find(|e| e.kind == "warning")
            .expect("a warning");
        assert!(
            warning.message.contains("failed with code 1"),
            "{}",
            warning.message
        );
        assert!(warning.message.contains("Archiving went on"));
        assert!(
            warning.message.contains("could not reach the database"),
            "{}",
            warning.message
        );
    }

    #[tokio::test]
    async fn archiving_ends_running_scripts_and_shells() {
        let fx = fixture().await;
        let ws = fx.svc.create(&fx.project.id, None).await.unwrap();
        let (ctx, _) = fx.svc.context_for(&ws.id).unwrap();
        let script = crate::scripts::RunScript {
            name: "web".into(),
            command: "sleep 60 & sleep 60 & wait".into(),
            default: true,
            icon: None,
        };
        fx.svc
            .terminals
            .run_script(
                &fx.svc.env,
                &ctx,
                &script,
                crate::scripts::RunMode::Concurrent,
            )
            .await
            .unwrap();
        assert_eq!(fx.svc.terminals.running_count(), 1);

        fx.svc.archive(&ws.id, true).await.unwrap();
        assert_eq!(
            fx.svc.terminals.running_count(),
            0,
            "the dev server was stopped"
        );
        assert!(!Path::new(&ws.path).exists());
    }

    #[tokio::test]
    async fn deleting_a_workspace_removes_its_tabs_and_saved_output() {
        let fx = fixture().await;
        write_settings(&fx, "[scripts]\nsetup = \"echo hello-setup\"\n");
        let ws = fx.svc.create(&fx.project.id, None).await.unwrap();
        until_state(&fx, &ws.id, WorkspaceState::Ready).await;
        let tabs = fx.svc.terminals.list(&ws.id).unwrap();
        let setup = tabs
            .iter()
            .find(|t| t.kind == crate::terminal::TerminalKind::Setup)
            .unwrap();
        assert!(fx.svc.terminals.saved_len(&setup.id) > 0);

        fx.svc.archive(&ws.id, true).await.unwrap();
        fx.svc.delete(&ws.id, false).await.unwrap();
        assert!(!fx.svc.terminals.has_row(&setup.id));
        assert_eq!(fx.svc.terminals.saved_len(&setup.id), 0);
        assert_eq!(ports::get(&fx.svc.projects.db, &ws.id).unwrap(), None);
    }

    #[tokio::test]
    async fn the_workspace_follows_what_its_agents_do() {
        let fx = fixture().await;
        let ws = fx.svc.create(&fx.project.id, None).await.unwrap();
        let state = |fx: &Fx| fx.svc.get(&ws.id).unwrap().state;

        fx.svc
            .apply_agent_state(&ws.id, AgentState::Working)
            .unwrap();
        assert_eq!(state(&fx), WorkspaceState::Working);
        fx.svc
            .apply_agent_state(&ws.id, AgentState::Blocked)
            .unwrap();
        assert_eq!(state(&fx), WorkspaceState::Blocked);
        fx.svc
            .apply_agent_state(&ws.id, AgentState::Working)
            .unwrap();
        assert_eq!(state(&fx), WorkspaceState::Working);
        fx.svc.apply_agent_state(&ws.id, AgentState::Idle).unwrap();
        assert_eq!(state(&fx), WorkspaceState::Idle);
        fx.svc
            .apply_agent_state(&ws.id, AgentState::Working)
            .unwrap();
        assert_eq!(
            state(&fx),
            WorkspaceState::Working,
            "idle can start working again"
        );

        let changes: Vec<_> = fx.sink.changes().iter().map(|c| c.to).collect();
        assert_eq!(
            &changes[2..],
            [
                WorkspaceState::Working,
                WorkspaceState::Blocked,
                WorkspaceState::Working,
                WorkspaceState::Idle,
                WorkspaceState::Working
            ]
        );
    }

    #[tokio::test]
    async fn a_workspace_that_never_worked_stays_ready_when_its_agents_are_idle() {
        let fx = fixture().await;
        let ws = fx.svc.create(&fx.project.id, None).await.unwrap();
        fx.svc.apply_agent_state(&ws.id, AgentState::Idle).unwrap();
        assert_eq!(fx.svc.get(&ws.id).unwrap().state, WorkspaceState::Ready);
    }

    #[tokio::test]
    async fn agents_do_not_change_workspaces_that_are_not_theirs_to_change() {
        let fx = fixture().await;
        write_settings(&fx, "[scripts]\nsetup = \"sleep 1\"\n");
        let setting_up = fx.svc.create(&fx.project.id, None).await.unwrap();
        assert_eq!(setting_up.state, WorkspaceState::SetupRunning);
        fx.svc
            .apply_agent_state(&setting_up.id, AgentState::Working)
            .unwrap();
        assert_eq!(
            fx.svc.get(&setting_up.id).unwrap().state,
            WorkspaceState::SetupRunning
        );
        until_state(&fx, &setting_up.id, WorkspaceState::Ready).await;

        fx.svc.archive(&setting_up.id, true).await.unwrap();
        fx.svc
            .apply_agent_state(&setting_up.id, AgentState::Working)
            .unwrap();
        assert_eq!(
            fx.svc.get(&setting_up.id).unwrap().state,
            WorkspaceState::Archived
        );
        assert_eq!(
            fx.svc
                .apply_agent_state("missing", AgentState::Idle)
                .unwrap_err()
                .code(),
            "not_found"
        );
    }

    #[tokio::test]
    async fn the_first_prompt_is_handed_over_once() {
        let fx = fixture().await;
        let ws = fx
            .svc
            .create_with(
                &fx.project.id,
                CreateOptions {
                    prompt: Some("Add dark mode".into()),
                    model: Some("opus".into()),
                    effort: Some("high".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        let taken = fx
            .svc
            .take_pending_prompt(&ws.id)
            .unwrap()
            .expect("a prompt");
        assert_eq!(
            taken,
            PendingPrompt {
                prompt: "Add dark mode".into(),
                model: Some("opus".into()),
                effort: Some("high".into())
            }
        );
        assert_eq!(
            fx.svc.take_pending_prompt(&ws.id).unwrap(),
            None,
            "taken only once"
        );
        let after = fx.svc.get(&ws.id).unwrap();
        assert!(
            after.pending_prompt.is_none()
                && after.pending_model.is_none()
                && after.pending_effort.is_none()
        );

        let blank = fx.svc.create(&fx.project.id, None).await.unwrap();
        assert_eq!(fx.svc.take_pending_prompt(&blank.id).unwrap(), None);
        assert_eq!(
            fx.svc.take_pending_prompt("missing").unwrap_err().code(),
            "not_found"
        );
    }

    #[test]
    fn script_output_is_summarised_without_colors() {
        assert_eq!(tail_of(""), "");
        assert_eq!(
            tail_of("\u{1b}[31mred error\u{1b}[0m\r\n"),
            " Output: red error"
        );
        let long = (0..10)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(tail_of(&long), " Output: line 7 / line 8 / line 9");
        assert!(tail_of(&"x".repeat(1000)).chars().count() < 320);
    }

    #[test]
    fn ensure_line_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("nested/info/exclude");
        ensure_line(&file, "/.context/").unwrap();
        ensure_line(&file, "/.context/").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "/.context/\n");

        std::fs::write(&file, "# comment").unwrap();
        ensure_line(&file, "/.context/").unwrap();
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "# comment\n/.context/\n"
        );
    }
}
