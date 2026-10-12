use rusqlite::Row;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::state::WorkspaceState;

#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[ts(export)]
pub struct Workspace {
    pub id: String,
    pub project_id: String,
    /// Display name, for example `Algiers`.
    pub city: String,
    pub title: Option<String>,
    pub branch: String,
    pub base_branch: String,
    pub path: String,
    pub state: WorkspaceState,
    /// Why the workspace is `errored`.
    pub error: Option<String>,
    pub pinned: bool,
    pub section_id: Option<String>,
    #[ts(type = "number | null")]
    pub pr_number: Option<i64>,
    pub pr_url: Option<String>,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub updated_at: i64,
    #[ts(type = "number | null")]
    pub last_opened_at: Option<i64>,
    #[ts(type = "number | null")]
    pub archived_at: Option<i64>,
    /// The first message from the Create dialog, waiting to be sent by the chat.
    pub pending_prompt: Option<String>,
    pub pending_model: Option<String>,
    pub pending_effort: Option<String>,
}

pub const WORKSPACE_COLUMNS: &str =
    "id, project_id, city, title, branch, base_branch, path, state, error, pinned, \
     section_id, pr_number, pr_url, created_at, updated_at, last_opened_at, archived_at, \
     pending_prompt, pending_model, pending_effort";

pub fn workspace_from_row(row: &Row<'_>) -> rusqlite::Result<Workspace> {
    let state: String = row.get(7)?;
    let state = state.parse::<WorkspaceState>().map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(7, rusqlite::types::Type::Text, e.into())
    })?;
    Ok(Workspace {
        id: row.get(0)?,
        project_id: row.get(1)?,
        city: row.get(2)?,
        title: row.get(3)?,
        branch: row.get(4)?,
        base_branch: row.get(5)?,
        path: row.get(6)?,
        state,
        error: row.get(8)?,
        pinned: row.get::<_, i64>(9)? != 0,
        section_id: row.get(10)?,
        pr_number: row.get(11)?,
        pr_url: row.get(12)?,
        created_at: row.get(13)?,
        updated_at: row.get(14)?,
        last_opened_at: row.get(15)?,
        archived_at: row.get(16)?,
        pending_prompt: row.get(17)?,
        pending_model: row.get(18)?,
        pending_effort: row.get(19)?,
    })
}

#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[ts(export)]
pub struct Section {
    pub id: String,
    pub name: String,
    #[ts(type = "number")]
    pub position: i64,
}

#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[ts(export)]
pub struct WorkspaceEvent {
    pub id: String,
    pub workspace_id: String,
    /// `created`, `info`, `warning`, `hint` or `error`.
    pub kind: String,
    pub message: String,
    #[ts(type = "number")]
    pub created_at: i64,
}

/// Emitted as `workspace:state-changed` whenever a workspace changes state.
#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[ts(export)]
pub struct StateChange {
    pub workspace_id: String,
    pub project_id: String,
    /// `None` when the workspace was just created.
    pub from: Option<WorkspaceState>,
    pub to: WorkspaceState,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, TS, PartialEq, Eq)]
#[ts(export)]
pub struct ReconcileReport {
    /// Workspaces marked errored because their folder is missing or setup was interrupted.
    pub errored: u32,
    /// Errored workspaces whose folder came back.
    pub recovered: u32,
    /// Workspaces that were working or blocked and are now idle, since no agent survives a restart.
    pub reset: u32,
}

/// Where a new workspace's branch comes from.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CreateSource {
    /// A new branch off a base. `None` uses the project's base branch.
    NewBranch { base_branch: Option<String> },
    /// Check out a branch that already exists, locally or on origin.
    ExistingBranch { name: String },
    /// Check out a pull request's branch.
    PullRequest { number: u32 },
}

impl Default for CreateSource {
    fn default() -> Self {
        CreateSource::NewBranch { base_branch: None }
    }
}

/// Everything the Create dialog sends.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CreateOptions {
    #[serde(default)]
    pub source: CreateSource,
    pub prompt: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    /// A name for a new branch. Without one it gets `<login>/<random>`. Ignored when an existing
    /// branch or a pull request is checked out.
    #[serde(default)]
    #[ts(optional)]
    pub branch_name: Option<String>,
    /// Absolute paths of files to copy into `.context/attachments/`.
    #[serde(default)]
    pub attachments: Vec<String>,
}

/// Emitted as `workspace:progress` while a workspace is being created.
#[derive(Debug, Clone, Serialize, TS, PartialEq, Eq)]
#[ts(export)]
pub struct ProgressEvent {
    pub workspace_id: String,
    pub project_id: String,
    /// `fetching`, `creating_worktree`, `copying_files` or `finishing`.
    pub stage: String,
}

/// Where a workspace's branch stands, for the sidebar: its pull request and its commits.
#[derive(Debug, Clone, Serialize, TS, PartialEq, Eq)]
#[ts(export)]
pub struct BranchState {
    pub workspace_id: String,
    pub pr_number: Option<u32>,
    pub pr_state: Option<crate::github::pr::PrState>,
    pub is_draft: bool,
    /// Commits the branch has that its base lacks.
    pub ahead: u32,
}

#[derive(Debug, Clone, Serialize, TS, PartialEq, Eq)]
#[ts(export)]
pub struct BranchInfo {
    pub name: String,
    pub local: bool,
    pub remote: bool,
    /// Folder of the worktree that has this branch checked out, if any.
    pub checked_out_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS, PartialEq, Eq)]
#[ts(export)]
pub struct PullRequest {
    #[ts(type = "number")]
    pub number: u32,
    pub title: String,
    pub head_ref: String,
    pub author: String,
    pub is_cross_repository: bool,
    pub updated_at: String,
}

/// The first message of a workspace, taken from the Create dialog.
#[derive(Debug, Clone, Serialize, TS, PartialEq, Eq)]
#[ts(export)]
pub struct PendingPrompt {
    pub prompt: String,
    pub model: Option<String>,
    pub effort: Option<String>,
}
