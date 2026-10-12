//! Composers, their runs, children, approvals and triggers, in the database.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::ipc::{new_id, now_ms};
use crate::tools::policy::{Autonomy, Limits, Mode, Policy};

/// The hidden project that holds the scratch workspaces of Composers.
pub const HOME_PROJECT_ID: &str = "composers";

fn invalid(message: impl Into<String>) -> AppError {
    AppError::Invalid(message.into())
}

#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[ts(export)]
pub struct Composer {
    pub id: String,
    pub name: String,
    pub instructions: String,
    /// The hidden workspace whose chats are this Composer's runs.
    pub workspace_id: String,
    /// `manual`, `accept_edits`, `plan` or `bypass`: what it does with a tool that has no setting
    /// of its own below.
    pub mode: String,
    /// What each tool does. A tool that is not here follows the mode.
    pub autonomy: BTreeMap<String, Autonomy>,
    /// Projects it may touch, or `None` for all.
    pub projects: Option<Vec<String>>,
    /// Outside services it may use, such as `jira`.
    pub integrations: Vec<String>,
    pub limits: Limits,
    pub allow_override: bool,
    pub notes: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub pinned: bool,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub updated_at: i64,
}

impl Composer {
    pub fn policy(&self) -> Policy {
        Policy {
            mode: Mode::parse(&self.mode).unwrap_or_default(),
            autonomy: self.autonomy.clone(),
            projects: self.projects.clone(),
            limits: self.limits.clone(),
            allow_override: self.allow_override,
        }
    }
}

/// What the dialog sends to create or change a Composer.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ComposerInput {
    pub name: String,
    #[serde(default)]
    pub instructions: String,
    /// `manual`, `accept_edits`, `plan` or `bypass`. When it is not given, a new Composer takes the
    /// default permission mode from the settings, and an edited one keeps its own.
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub autonomy: BTreeMap<String, Autonomy>,
    pub projects: Option<Vec<String>>,
    /// Outside services it may use, such as `jira`. Only known ones are kept.
    #[serde(default)]
    pub integrations: Vec<String>,
    #[serde(default)]
    pub limits: Limits,
    #[serde(default)]
    pub allow_override: bool,
    #[serde(default)]
    pub notes: String,
    pub model: Option<String>,
    pub effort: Option<String>,
}

/// The most notes a Composer may keep, so they stay readable in full before every run.
pub const NOTES_LIMIT: usize = 4000;

impl ComposerInput {
    pub fn check(&self) -> AppResult<()> {
        if self.name.trim().is_empty() {
            return Err(invalid("Give the Composer a name."));
        }
        if self.name.chars().count() > 80 {
            return Err(invalid("The name can be at most 80 characters."));
        }
        if self.instructions.chars().count() > 20_000 {
            return Err(invalid("The instructions can be at most 20000 characters."));
        }
        if self.notes.chars().count() > NOTES_LIMIT {
            return Err(invalid(format!(
                "The notes can be at most {NOTES_LIMIT} characters."
            )));
        }
        if let Some(mode) = &self.mode {
            if Mode::parse(mode).is_none() {
                return Err(invalid(format!("\"{mode}\" is not a mode.")));
            }
        }
        for tool in self.autonomy.keys() {
            if crate::tools::catalog::get(tool).is_none() {
                return Err(invalid(format!("\"{tool}\" is not a tool.")));
            }
        }
        let l = &self.limits;
        if l.max_steps == 0
            || l.max_children_at_once == 0
            || l.max_children_total == 0
            || l.max_minutes == 0
        {
            return Err(invalid("Limits must be at least 1."));
        }
        if l.max_budget_usd.is_some_and(|b| b.is_nan() || b <= 0.0) {
            return Err(invalid("The spending limit must be more than 0."));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[ts(export)]
pub struct Approval {
    pub id: String,
    pub composer_id: String,
    pub session_id: String,
    pub tool: String,
    pub tier: String,
    pub input: String,
    /// `pending`, `allowed`, `denied` or `expired`.
    pub status: String,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number | null")]
    pub answered_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[ts(export)]
pub struct RunInfo {
    pub session_id: String,
    pub composer_id: String,
    /// `person`, `schedule` or `trigger`.
    pub started_by: String,
    pub trigger_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[ts(export)]
pub struct ChildRow {
    pub id: String,
    pub composer_id: String,
    /// The run that started it.
    pub session_id: String,
    pub workspace_id: Option<String>,
    pub chat_id: Option<String>,
    #[ts(type = "number")]
    pub created_at: i64,
}

/// A workspace a Composer's run started, for the badge on it.
#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[ts(export)]
pub struct WorkspaceLink {
    pub workspace_id: String,
    pub composer_id: String,
    pub composer_name: String,
    /// The run that started it.
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct Trigger {
    pub id: String,
    pub composer_id: String,
    /// `schedule`, `pull_request`, `check_failed` or `jira_issue`.
    pub kind: String,
    #[ts(type = "unknown")]
    pub config: serde_json::Value,
    pub project_id: Option<String>,
    pub prompt: String,
    pub enabled: bool,
    pub failures: u32,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number | null")]
    pub last_fired_at: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct TriggerInput {
    pub kind: String,
    #[ts(type = "unknown")]
    #[serde(default)]
    pub config: serde_json::Value,
    pub project_id: Option<String>,
    #[serde(default)]
    pub prompt: String,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

/// The services a Composer can be given. Anything else is dropped.
pub const INTEGRATIONS: &[&str] = &["jira"];

fn integrations_json(list: &[String]) -> String {
    let mut kept: Vec<&str> = INTEGRATIONS
        .iter()
        .copied()
        .filter(|name| list.iter().any(|l| l == name))
        .collect();
    kept.dedup();
    serde_json::to_string(&kept).unwrap_or_else(|_| "[]".into())
}

const COMPOSER_COLUMNS: &str = "id, name, instructions, workspace_id, autonomy, projects, limits, \
     allow_override, notes, model, effort, pinned, created_at, updated_at, mode, integrations";

fn composer_from_row(row: &Row<'_>) -> rusqlite::Result<Composer> {
    let autonomy: String = row.get(4)?;
    let projects: Option<String> = row.get(5)?;
    let limits: String = row.get(6)?;
    Ok(Composer {
        id: row.get(0)?,
        name: row.get(1)?,
        instructions: row.get(2)?,
        workspace_id: row.get(3)?,
        autonomy: serde_json::from_str(&autonomy).unwrap_or_default(),
        projects: projects.and_then(|p| serde_json::from_str(&p).ok()),
        limits: serde_json::from_str(&limits).unwrap_or_default(),
        allow_override: row.get::<_, i64>(7)? != 0,
        notes: row.get(8)?,
        model: row.get(9)?,
        effort: row.get(10)?,
        pinned: row.get::<_, i64>(11)? != 0,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
        mode: row.get(14)?,
        integrations: serde_json::from_str(&row.get::<_, String>(15)?).unwrap_or_default(),
    })
}

const TRIGGER_COLUMNS: &str =
    "id, composer_id, kind, config, project_id, prompt, enabled, failures, created_at, last_fired_at";

fn trigger_from_row(row: &Row<'_>) -> rusqlite::Result<Trigger> {
    let config: String = row.get(3)?;
    Ok(Trigger {
        id: row.get(0)?,
        composer_id: row.get(1)?,
        kind: row.get(2)?,
        config: serde_json::from_str(&config).unwrap_or(serde_json::Value::Null),
        project_id: row.get(4)?,
        prompt: row.get(5)?,
        enabled: row.get::<_, i64>(6)? != 0,
        failures: row.get(7)?,
        created_at: row.get(8)?,
        last_fired_at: row.get(9)?,
    })
}

fn approval_from_row(row: &Row<'_>) -> rusqlite::Result<Approval> {
    Ok(Approval {
        id: row.get(0)?,
        composer_id: row.get(1)?,
        session_id: row.get(2)?,
        tool: row.get(3)?,
        tier: row.get(4)?,
        input: row.get(5)?,
        status: row.get(6)?,
        created_at: row.get(7)?,
        answered_at: row.get(8)?,
    })
}

const APPROVAL_COLUMNS: &str =
    "id, composer_id, session_id, tool, tier, input, status, created_at, answered_at";

#[derive(Clone)]
pub struct ComposerStore {
    db: Arc<Db>,
    /// Where scratch workspaces are made.
    root: std::path::PathBuf,
}

impl ComposerStore {
    pub fn new(db: Arc<Db>, data_dir: &Path) -> Self {
        Self {
            db,
            root: data_dir.join("composers"),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn db(&self) -> &Arc<Db> {
        &self.db
    }

    /// The hidden project, made when the first Composer is.
    fn ensure_home(&self) -> AppResult<()> {
        std::fs::create_dir_all(&self.root)?;
        let path = self.root.to_string_lossy().into_owned();
        let now = now_ms();
        self.db.write(|c| {
            c.execute(
                "INSERT OR IGNORE INTO projects (id, name, path, default_branch, created_at, updated_at)
                 VALUES (?1, 'Composers', ?2, '-', ?3, ?3)",
                params![HOME_PROJECT_ID, path, now],
            )
        })?;
        Ok(())
    }

    // Composers

    pub fn list(&self) -> AppResult<Vec<Composer>> {
        self.db.read(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {COMPOSER_COLUMNS} FROM composers ORDER BY created_at, rowid"
            ))?;
            let rows = stmt.query_map([], composer_from_row)?;
            rows.collect()
        })
    }

    pub fn get(&self, id: &str) -> AppResult<Composer> {
        self.db
            .read(|c| {
                c.query_row(
                    &format!("SELECT {COMPOSER_COLUMNS} FROM composers WHERE id = ?1"),
                    params![id],
                    composer_from_row,
                )
            })
            .map_err(|e| match e {
                AppError::Db(rusqlite::Error::QueryReturnedNoRows) => {
                    AppError::NotFound(format!("composer {id}"))
                }
                other => other,
            })
    }

    pub fn by_workspace(&self, workspace_id: &str) -> AppResult<Option<Composer>> {
        self.db.read(|c| {
            c.query_row(
                &format!("SELECT {COMPOSER_COLUMNS} FROM composers WHERE workspace_id = ?1"),
                params![workspace_id],
                composer_from_row,
            )
            .optional()
        })
    }

    /// The Composer whose run this chat is.
    /// Takes a service away from every Composer, when its connection is removed.
    pub fn clear_integration(&self, name: &str) -> AppResult<()> {
        for composer in self.list()? {
            if composer.integrations.iter().any(|i| i == name) {
                let rest: Vec<String> = composer
                    .integrations
                    .iter()
                    .filter(|i| i.as_str() != name)
                    .cloned()
                    .collect();
                self.db.write(|c| {
                    c.execute(
                        "UPDATE composers SET integrations = ?2 WHERE id = ?1",
                        params![composer.id, integrations_json(&rest)],
                    )
                })?;
            }
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn db_for_tests(&self) -> &Db {
        &self.db
    }

    pub fn by_session(&self, session_id: &str) -> AppResult<Option<Composer>> {
        self.db.read(|c| {
            c.query_row(
                &format!(
                    "SELECT {} FROM composers co JOIN sessions s ON s.workspace_id = co.workspace_id
                     WHERE s.id = ?1",
                    COMPOSER_COLUMNS
                        .split(", ")
                        .map(|col| format!("co.{col}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                params![session_id],
                composer_from_row,
            )
            .optional()
        })
    }

    pub fn create(&self, input: &ComposerInput) -> AppResult<Composer> {
        input.check()?;
        self.ensure_home()?;
        let id = new_id();
        let workspace_id = new_id();
        let folder = self.root.join(&id);
        std::fs::create_dir_all(&folder)?;
        let now = now_ms();
        let folder_text = folder.to_string_lossy().into_owned();
        self.db.write(|c| {
            c.execute(
                "INSERT INTO workspaces (id, project_id, city, title, branch, base_branch, path, state,
                                         created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, '-', '-', ?5, 'idle', ?6, ?6)",
                params![
                    workspace_id,
                    HOME_PROJECT_ID,
                    id,
                    input.name.trim(),
                    folder_text,
                    now
                ],
            )?;
            c.execute(
                "INSERT INTO composers (id, name, instructions, workspace_id, autonomy, projects, limits,
                                        allow_override, notes, model, effort, created_at, updated_at, mode, integrations)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12, ?13, ?14)",
                params![
                    id,
                    input.name.trim(),
                    input.instructions,
                    workspace_id,
                    serde_json::to_string(&input.autonomy).unwrap_or_else(|_| "{}".into()),
                    input
                        .projects
                        .as_ref()
                        .and_then(|p| serde_json::to_string(p).ok()),
                    serde_json::to_string(&input.limits).unwrap_or_else(|_| "{}".into()),
                    input.allow_override,
                    input.notes,
                    input.model,
                    input.effort,
                    now,
                    input.mode.as_deref().unwrap_or("manual"),
                    integrations_json(&input.integrations)
                ],
            )
        })?;
        self.get(&id)
    }

    pub fn update(&self, id: &str, input: &ComposerInput) -> AppResult<Composer> {
        input.check()?;
        let existing = self.get(id)?;
        let now = now_ms();
        self.db.write(|c| {
            c.execute(
                "UPDATE composers SET name = ?2, instructions = ?3, autonomy = ?4, projects = ?5,
                        limits = ?6, allow_override = ?7, notes = ?8, model = ?9, effort = ?10,
                        updated_at = ?11, mode = ?12, integrations = ?13
                 WHERE id = ?1",
                params![
                    id,
                    input.name.trim(),
                    input.instructions,
                    serde_json::to_string(&input.autonomy).unwrap_or_else(|_| "{}".into()),
                    input
                        .projects
                        .as_ref()
                        .and_then(|p| serde_json::to_string(p).ok()),
                    serde_json::to_string(&input.limits).unwrap_or_else(|_| "{}".into()),
                    input.allow_override,
                    input.notes,
                    input.model,
                    input.effort,
                    now,
                    input.mode.as_deref().unwrap_or(existing.mode.as_str()),
                    integrations_json(&input.integrations)
                ],
            )?;
            c.execute(
                "UPDATE workspaces SET title = ?2, updated_at = ?3 WHERE id = ?1",
                params![existing.workspace_id, input.name.trim(), now],
            )
        })?;
        self.get(id)
    }

    pub fn set_pinned(&self, id: &str, pinned: bool) -> AppResult<Composer> {
        self.get(id)?;
        self.db.write(|c| {
            c.execute(
                "UPDATE composers SET pinned = ?2 WHERE id = ?1",
                params![id, pinned],
            )
        })?;
        self.get(id)
    }

    /// Changes how much the Composer does on its own, for every run, from the next call on.
    pub fn set_mode(&self, id: &str, mode: &str) -> AppResult<Composer> {
        if Mode::parse(mode).is_none() {
            return Err(invalid(format!("\"{mode}\" is not a mode.")));
        }
        self.get(id)?;
        self.db.write(|c| {
            c.execute(
                "UPDATE composers SET mode = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, mode, now_ms()],
            )
        })?;
        self.get(id)
    }

    pub fn set_notes(&self, id: &str, notes: &str) -> AppResult<Composer> {
        if notes.chars().count() > NOTES_LIMIT {
            return Err(invalid(format!(
                "The notes can be at most {NOTES_LIMIT} characters."
            )));
        }
        self.get(id)?;
        self.db.write(|c| {
            c.execute(
                "UPDATE composers SET notes = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, notes, now_ms()],
            )
        })?;
        self.get(id)
    }

    /// Removes the Composer, its scratch workspace and (by cascade) its chats, runs and history.
    pub fn delete(&self, id: &str) -> AppResult<()> {
        let composer = self.get(id)?;
        self.db.write(|c| {
            c.execute("DELETE FROM approvals WHERE composer_id = ?1", params![id])?;
            c.execute("DELETE FROM composers WHERE id = ?1", params![id])?;
            c.execute(
                "DELETE FROM workspaces WHERE id = ?1",
                params![composer.workspace_id],
            )
        })?;
        let _ = std::fs::remove_dir_all(self.root.join(id));
        Ok(())
    }

    // Runs

    pub fn record_run(
        &self,
        session_id: &str,
        composer_id: &str,
        started_by: &str,
        trigger_id: Option<&str>,
    ) -> AppResult<()> {
        self.db.write(|c| {
            c.execute(
                "INSERT OR REPLACE INTO composer_runs (session_id, composer_id, started_by, trigger_id, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![session_id, composer_id, started_by, trigger_id, now_ms()],
            )
        })?;
        Ok(())
    }

    pub fn run_info(&self, session_id: &str) -> AppResult<Option<RunInfo>> {
        self.db.read(|c| {
            c.query_row(
                "SELECT session_id, composer_id, started_by, trigger_id FROM composer_runs WHERE session_id = ?1",
                params![session_id],
                |r| {
                    Ok(RunInfo {
                        session_id: r.get(0)?,
                        composer_id: r.get(1)?,
                        started_by: r.get(2)?,
                        trigger_id: r.get(3)?,
                    })
                },
            )
            .optional()
        })
    }

    /// When the chat was made, for the run's wall time limit.
    pub fn session_started_at(&self, session_id: &str) -> AppResult<Option<i64>> {
        self.db.read(|c| {
            c.query_row(
                "SELECT created_at FROM sessions WHERE id = ?1",
                params![session_id],
                |r| r.get(0),
            )
            .optional()
        })
    }

    /// Triggered runs started today (UTC day), for the daily cap.
    pub fn triggered_runs_since(&self, since_ms: i64) -> AppResult<u32> {
        self.db.read(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM composer_runs WHERE started_by != 'person' AND created_at >= ?1",
                params![since_ms],
                |r| r.get(0),
            )
        })
    }

    // Children

    pub fn add_child(
        &self,
        composer_id: &str,
        session_id: &str,
        workspace_id: Option<&str>,
        chat_id: Option<&str>,
    ) -> AppResult<()> {
        self.db.write(|c| {
            c.execute(
                "INSERT INTO composer_children (id, composer_id, session_id, workspace_id, chat_id, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![new_id(), composer_id, session_id, workspace_id, chat_id, now_ms()],
            )
        })?;
        Ok(())
    }

    pub fn children_of_run(&self, session_id: &str) -> AppResult<Vec<ChildRow>> {
        self.children_where("session_id = ?1", session_id)
    }

    /// When the person's latest message in this run was sent, which is when its current turn began.
    /// A Composer has one conversation that goes on for days, so what a run may start is counted
    /// from here, not from the first message ever. `0` when there is no message yet.
    pub fn turn_started_at(&self, session_id: &str) -> AppResult<i64> {
        self.db.read(|c| {
            c.query_row(
                "SELECT COALESCE(MAX(created_at), 0) FROM messages
                 WHERE session_id = ?1 AND role = 'user' AND kind = 'text'",
                params![session_id],
                |r| r.get(0),
            )
        })
    }

    pub fn children_of_composer(&self, composer_id: &str) -> AppResult<Vec<ChildRow>> {
        self.children_where("composer_id = ?1", composer_id)
    }

    fn children_where(&self, clause: &str, value: &str) -> AppResult<Vec<ChildRow>> {
        self.db.read(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT id, composer_id, session_id, workspace_id, chat_id, created_at
                 FROM composer_children WHERE {clause} ORDER BY created_at, rowid"
            ))?;
            let rows = stmt.query_map(params![value], |r| {
                Ok(ChildRow {
                    id: r.get(0)?,
                    composer_id: r.get(1)?,
                    session_id: r.get(2)?,
                    workspace_id: r.get(3)?,
                    chat_id: r.get(4)?,
                    created_at: r.get(5)?,
                })
            })?;
            rows.collect()
        })
    }

    /// The run that started a workspace, if a Composer did.
    pub fn run_of_workspace(&self, workspace_id: &str) -> AppResult<Option<ChildRow>> {
        self.db.read(|c| {
            c.query_row(
                "SELECT id, composer_id, session_id, workspace_id, chat_id, created_at
                 FROM composer_children WHERE workspace_id = ?1 ORDER BY created_at LIMIT 1",
                params![workspace_id],
                |r| {
                    Ok(ChildRow {
                        id: r.get(0)?,
                        composer_id: r.get(1)?,
                        session_id: r.get(2)?,
                        workspace_id: r.get(3)?,
                        chat_id: r.get(4)?,
                        created_at: r.get(5)?,
                    })
                },
            )
            .optional()
        })
    }

    /// Every workspace a Composer started, with who started it.
    pub fn workspace_links(&self) -> AppResult<Vec<WorkspaceLink>> {
        self.db.read(|c| {
            let mut stmt = c.prepare(
                "SELECT ch.workspace_id, co.id, co.name, ch.session_id
                 FROM composer_children ch JOIN composers co ON co.id = ch.composer_id
                 WHERE ch.workspace_id IS NOT NULL
                 GROUP BY ch.workspace_id
                 ORDER BY MIN(ch.created_at)",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(WorkspaceLink {
                    workspace_id: r.get(0)?,
                    composer_id: r.get(1)?,
                    composer_name: r.get(2)?,
                    session_id: r.get(3)?,
                })
            })?;
            rows.collect()
        })
    }

    // Approvals

    pub fn insert_approval(
        &self,
        composer_id: &str,
        session_id: &str,
        tool: &str,
        tier: &str,
        input: &str,
    ) -> AppResult<Approval> {
        let id = new_id();
        self.db.write(|c| {
            c.execute(
                "INSERT INTO approvals (id, composer_id, session_id, tool, tier, input, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![id, composer_id, session_id, tool, tier, input, now_ms()],
            )
        })?;
        self.approval(&id)
    }

    pub fn approval(&self, id: &str) -> AppResult<Approval> {
        self.db
            .read(|c| {
                c.query_row(
                    &format!("SELECT {APPROVAL_COLUMNS} FROM approvals WHERE id = ?1"),
                    params![id],
                    approval_from_row,
                )
            })
            .map_err(|e| match e {
                AppError::Db(rusqlite::Error::QueryReturnedNoRows) => {
                    AppError::NotFound(format!("approval {id}"))
                }
                other => other,
            })
    }

    /// Sets the answer, only while the approval is still pending. Returns whether it was.
    pub fn answer_approval(&self, id: &str, status: &str) -> AppResult<bool> {
        let changed = self.db.write(|c| {
            c.execute(
                "UPDATE approvals SET status = ?2, answered_at = ?3 WHERE id = ?1 AND status = 'pending'",
                params![id, status, now_ms()],
            )
        })?;
        Ok(changed > 0)
    }

    /// Pending first, then the latest answered ones. Every filter is optional.
    pub fn approvals(
        &self,
        composer_id: Option<&str>,
        session_id: Option<&str>,
        only_pending: bool,
        limit: u32,
    ) -> AppResult<Vec<Approval>> {
        self.db.read(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {APPROVAL_COLUMNS} FROM approvals
                 WHERE (?1 IS NULL OR composer_id = ?1) AND (?2 IS NULL OR session_id = ?2)
                   AND (?3 = 0 OR status = 'pending')
                 ORDER BY (status = 'pending') DESC, created_at DESC, rowid DESC LIMIT ?4"
            ))?;
            let rows = stmt.query_map(
                params![composer_id, session_id, only_pending, limit],
                approval_from_row,
            )?;
            rows.collect()
        })
    }

    /// After a restart nothing is waiting any more, because the calls that waited are gone.
    pub fn expire_pending(&self) -> AppResult<usize> {
        self.db.write(|c| {
            c.execute(
                "UPDATE approvals SET status = 'expired', answered_at = ?1 WHERE status = 'pending'",
                params![now_ms()],
            )
        })
    }

    // Triggers

    pub fn triggers(&self, composer_id: Option<&str>) -> AppResult<Vec<Trigger>> {
        self.db.read(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {TRIGGER_COLUMNS} FROM composer_triggers
                 WHERE (?1 IS NULL OR composer_id = ?1) ORDER BY created_at, rowid"
            ))?;
            let rows = stmt.query_map(params![composer_id], trigger_from_row)?;
            rows.collect()
        })
    }

    pub fn trigger(&self, id: &str) -> AppResult<Trigger> {
        self.db
            .read(|c| {
                c.query_row(
                    &format!("SELECT {TRIGGER_COLUMNS} FROM composer_triggers WHERE id = ?1"),
                    params![id],
                    trigger_from_row,
                )
            })
            .map_err(|e| match e {
                AppError::Db(rusqlite::Error::QueryReturnedNoRows) => {
                    AppError::NotFound(format!("trigger {id}"))
                }
                other => other,
            })
    }

    pub fn create_trigger(&self, composer_id: &str, input: &TriggerInput) -> AppResult<Trigger> {
        self.get(composer_id)?;
        check_trigger(input)?;
        let id = new_id();
        self.db.write(|c| {
            c.execute(
                "INSERT INTO composer_triggers (id, composer_id, kind, config, project_id, prompt, enabled, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    id,
                    composer_id,
                    input.kind,
                    input.config.to_string(),
                    input.project_id,
                    input.prompt,
                    input.enabled,
                    now_ms()
                ],
            )
        })?;
        self.trigger(&id)
    }

    pub fn update_trigger(&self, id: &str, input: &TriggerInput) -> AppResult<Trigger> {
        self.trigger(id)?;
        check_trigger(input)?;
        self.db.write(|c| {
            c.execute(
                "UPDATE composer_triggers SET kind = ?2, config = ?3, project_id = ?4, prompt = ?5,
                        enabled = ?6, failures = CASE WHEN ?6 = 1 THEN 0 ELSE failures END
                 WHERE id = ?1",
                params![
                    id,
                    input.kind,
                    input.config.to_string(),
                    input.project_id,
                    input.prompt,
                    input.enabled
                ],
            )
        })?;
        self.trigger(id)
    }

    pub fn delete_trigger(&self, id: &str) -> AppResult<()> {
        self.db
            .write(|c| c.execute("DELETE FROM composer_triggers WHERE id = ?1", params![id]))?;
        Ok(())
    }

    pub fn mark_fired(&self, id: &str) -> AppResult<()> {
        self.db.write(|c| {
            c.execute(
                "UPDATE composer_triggers SET last_fired_at = ?2, failures = 0 WHERE id = ?1",
                params![id, now_ms()],
            )
        })?;
        Ok(())
    }

    /// A good look at what a trigger watches: the failures in a row start again.
    pub fn clear_failures(&self, id: &str) -> AppResult<()> {
        self.db.write(|c| {
            c.execute(
                "UPDATE composer_triggers SET failures = 0 WHERE id = ?1 AND failures > 0",
                params![id],
            )
        })?;
        Ok(())
    }

    /// Runs started by a schedule or trigger that are working or waiting right now.
    pub fn unattended_active(&self) -> AppResult<u32> {
        self.db.read(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM composer_runs r JOIN sessions s ON s.id = r.session_id
                 WHERE r.started_by != 'person' AND s.status IN ('working', 'blocked')",
                [],
                |row| row.get(0),
            )
        })
    }

    /// Counts a failure and switches the trigger off once there are too many in a row.
    pub fn record_trigger_failure(&self, id: &str, max: u32) -> AppResult<bool> {
        self.db.write(|c| {
            c.execute(
                "UPDATE composer_triggers SET failures = failures + 1 WHERE id = ?1",
                params![id],
            )?;
            c.execute(
                "UPDATE composer_triggers SET enabled = 0 WHERE id = ?1 AND failures >= ?2",
                params![id, max],
            )
        })?;
        Ok(!self.trigger(id)?.enabled)
    }

    /// Records that an event was handled. Returns false when it already was.
    pub fn mark_seen(&self, trigger_id: &str, key: &str) -> AppResult<bool> {
        let changed = self.db.write(|c| {
            c.execute(
                "INSERT OR IGNORE INTO trigger_seen (trigger_id, key, seen_at) VALUES (?1, ?2, ?3)",
                params![trigger_id, key, now_ms()],
            )
        })?;
        Ok(changed > 0)
    }

    pub fn has_seen_any(&self, trigger_id: &str) -> AppResult<bool> {
        self.db.read(|c| {
            c.query_row(
                "SELECT EXISTS(SELECT 1 FROM trigger_seen WHERE trigger_id = ?1)",
                params![trigger_id],
                |r| r.get(0),
            )
        })
    }
}

fn check_trigger(input: &TriggerInput) -> AppResult<()> {
    match input.kind.as_str() {
        "schedule" => {
            let every = input.config["every_minutes"].as_u64();
            let daily = input.config["daily_at"].as_str();
            match (every, daily) {
                (Some(n), None) if (5..=10_080).contains(&n) => {}
                (None, Some(t)) if parse_time(t).is_some() => {}
                (Some(_), None) => {
                    return Err(invalid(
                        "A schedule can repeat every 5 minutes to every 7 days.",
                    ))
                }
                _ => {
                    return Err(invalid(
                        "A schedule needs either every_minutes or daily_at (HH:MM).",
                    ))
                }
            }
        }
        "pull_request" | "check_failed" => {
            if input.project_id.is_none() {
                return Err(invalid("This trigger needs a project to watch."));
            }
        }
        "jira_issue" => {
            let jql = input.config["jql"].as_str().unwrap_or("").trim();
            if jql.is_empty() || jql.chars().count() > 2000 {
                return Err(invalid(
                    "A Jira trigger needs a JQL query of at most 2000 characters.",
                ));
            }
        }
        other => return Err(invalid(format!("\"{other}\" is not a kind of trigger."))),
    }
    if input.prompt.chars().count() > 20_000 {
        return Err(invalid("The prompt can be at most 20000 characters."));
    }
    Ok(())
}

/// `HH:MM` as minutes after midnight.
pub fn parse_time(text: &str) -> Option<u32> {
    let (h, m) = text.split_once(':')?;
    let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn store() -> (ComposerStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::open(&dir.path().join("t.db")).unwrap());
        (ComposerStore::new(db, dir.path()), dir)
    }

    fn input(name: &str) -> ComposerInput {
        ComposerInput {
            name: name.into(),
            ..Default::default()
        }
    }

    #[test]
    fn a_composer_gets_a_hidden_workspace_and_stays_out_of_the_lists() {
        let (s, dir) = store();
        let c = s.create(&input("Release captain")).unwrap();
        assert!(dir.path().join("composers").join(&c.id).is_dir());
        assert_eq!(s.list().unwrap().len(), 1);
        assert_eq!(s.by_workspace(&c.workspace_id).unwrap().unwrap().id, c.id);

        // The hidden project and workspace are in the tables but not in what the app lists.
        let visible_projects: i64 =
            s.db.read(|conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM projects WHERE id != ?1",
                    params![HOME_PROJECT_ID],
                    |r| r.get(0),
                )
            })
            .unwrap();
        assert_eq!(visible_projects, 0);
        let home: i64 =
            s.db.read(|conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM workspaces WHERE project_id = ?1",
                    params![HOME_PROJECT_ID],
                    |r| r.get(0),
                )
            })
            .unwrap();
        assert_eq!(home, 1);
    }

    #[test]
    fn update_keeps_the_workspace_and_renames_it() {
        let (s, _d) = store();
        let c = s.create(&input("A")).unwrap();
        let mut change = input("B");
        change.autonomy.insert("merge_pr".into(), Autonomy::Allow);
        change.projects = Some(vec!["p1".into()]);
        change.limits.max_steps = 7;
        let u = s.update(&c.id, &change).unwrap();
        assert_eq!(u.workspace_id, c.workspace_id);
        assert_eq!(u.name, "B");
        assert_eq!(u.autonomy["merge_pr"], Autonomy::Allow);
        assert_eq!(u.projects, Some(vec!["p1".to_string()]));
        assert_eq!(u.limits.max_steps, 7);
        let title: String =
            s.db.read(|conn| {
                conn.query_row(
                    "SELECT title FROM workspaces WHERE id = ?1",
                    params![c.workspace_id],
                    |r| r.get(0),
                )
            })
            .unwrap();
        assert_eq!(title, "B");
    }

    #[test]
    fn bad_input_is_refused_with_a_reason() {
        let (s, _d) = store();
        assert!(s
            .create(&input("  "))
            .unwrap_err()
            .to_string()
            .contains("name"));
        let mut bad = input("x");
        bad.autonomy.insert("rm_rf".into(), Autonomy::Allow);
        assert!(s
            .create(&bad)
            .unwrap_err()
            .to_string()
            .contains("not a tool"));
        let mut zero = input("x");
        zero.limits.max_steps = 0;
        assert!(s
            .create(&zero)
            .unwrap_err()
            .to_string()
            .contains("at least 1"));
        let mut notes = input("x");
        notes.notes = "n".repeat(NOTES_LIMIT + 1);
        assert!(s.create(&notes).unwrap_err().to_string().contains("notes"));
        let mut budget = input("x");
        budget.limits.max_budget_usd = Some(0.0);
        assert!(s.create(&budget).is_err());
    }

    #[test]
    fn the_mode_is_kept_checked_and_only_changed_when_given() {
        let (s, _d) = store();
        assert_eq!(s.create(&input("A")).unwrap().mode, "manual");
        let mut bypass = input("B");
        bypass.mode = Some("bypass".into());
        let b = s.create(&bypass).unwrap();
        assert_eq!(b.mode, "bypass");
        assert_eq!(b.policy().mode, Mode::Bypass);
        // An update that does not say keeps it. One that does changes it.
        assert_eq!(s.update(&b.id, &input("B2")).unwrap().mode, "bypass");
        let mut plan = input("B3");
        plan.mode = Some("plan".into());
        assert_eq!(s.update(&b.id, &plan).unwrap().mode, "plan");
        let mut bad = input("C");
        bad.mode = Some("yolo".into());
        assert!(s
            .create(&bad)
            .unwrap_err()
            .to_string()
            .contains("not a mode"));
    }

    #[test]
    fn the_mode_can_be_changed_on_its_own() {
        let (s, _d) = store();
        let c = s.create(&input("A")).unwrap();
        assert_eq!(s.set_mode(&c.id, "bypass").unwrap().mode, "bypass");
        assert_eq!(s.get(&c.id).unwrap().mode, "bypass");
        assert!(s.set_mode(&c.id, "yolo").is_err());
        assert!(s.set_mode("nope", "plan").is_err());
        assert_eq!(
            s.get(&c.id).unwrap().mode,
            "bypass",
            "a refused change leaves it as it was"
        );
    }

    #[test]
    fn deleting_removes_the_workspace_and_its_folder() {
        let (s, dir) = store();
        let c = s.create(&input("A")).unwrap();
        s.delete(&c.id).unwrap();
        assert!(s.list().unwrap().is_empty());
        assert!(!dir.path().join("composers").join(&c.id).exists());
        assert!(s.get(&c.id).is_err());
    }

    #[test]
    fn approvals_are_answered_once_and_expire_after_a_restart() {
        let (s, _d) = store();
        let c = s.create(&input("A")).unwrap();
        let a = s
            .insert_approval(&c.id, "run1", "delete_workspace", "destructive", "{}")
            .unwrap();
        assert_eq!(a.status, "pending");
        assert_eq!(s.approvals(None, None, true, 10).unwrap().len(), 1);
        assert!(s.answer_approval(&a.id, "allowed").unwrap());
        assert!(
            !s.answer_approval(&a.id, "denied").unwrap(),
            "only the first answer counts"
        );
        assert_eq!(s.approval(&a.id).unwrap().status, "allowed");

        s.insert_approval(&c.id, "run1", "merge_pr", "external", "{}")
            .unwrap();
        assert_eq!(s.expire_pending().unwrap(), 1);
        assert!(s.approvals(None, None, true, 10).unwrap().is_empty());
    }

    #[test]
    fn children_are_found_by_run_composer_and_workspace() {
        let (s, _d) = store();
        let c = s.create(&input("A")).unwrap();
        s.add_child(&c.id, "run1", Some("w1"), Some("chat1"))
            .unwrap();
        s.add_child(&c.id, "run2", Some("w2"), None).unwrap();
        assert_eq!(s.children_of_run("run1").unwrap().len(), 1);
        assert_eq!(s.children_of_composer(&c.id).unwrap().len(), 2);
        assert_eq!(
            s.run_of_workspace("w2").unwrap().unwrap().session_id,
            "run2"
        );
        assert!(s.run_of_workspace("nope").unwrap().is_none());
    }

    #[test]
    fn a_workspace_a_run_started_is_linked_to_the_composer_that_did() {
        let (s, _d) = store();
        let a = s.create(&input("Alpha")).unwrap();
        let b = s.create(&input("Beta")).unwrap();
        s.add_child(&a.id, "run1", Some("w1"), None).unwrap();
        s.add_child(&a.id, "run1", Some("w1"), Some("chat1"))
            .unwrap();
        s.add_child(&b.id, "run2", Some("w2"), None).unwrap();
        s.add_child(&b.id, "run2", None, Some("chat2")).unwrap();
        let links = s.workspace_links().unwrap();
        assert_eq!(
            links.len(),
            2,
            "one per workspace, and chats alone are not workspaces"
        );
        assert_eq!(
            (
                links[0].workspace_id.as_str(),
                links[0].composer_name.as_str()
            ),
            ("w1", "Alpha")
        );
        assert_eq!(links[1].session_id, "run2");
    }

    #[test]
    fn triggers_are_checked_and_fire_once_per_event() {
        let (s, _d) = store();
        let c = s.create(&input("A")).unwrap();
        let bad = |kind: &str, config: serde_json::Value, project: Option<&str>| TriggerInput {
            kind: kind.into(),
            config,
            project_id: project.map(String::from),
            prompt: String::new(),
            enabled: true,
        };
        assert!(s
            .create_trigger(&c.id, &bad("schedule", json!({}), None))
            .is_err());
        assert!(s
            .create_trigger(&c.id, &bad("schedule", json!({"every_minutes": 1}), None))
            .is_err());
        assert!(s
            .create_trigger(&c.id, &bad("schedule", json!({"daily_at": "25:00"}), None))
            .is_err());
        assert!(s
            .create_trigger(&c.id, &bad("pull_request", json!({}), None))
            .is_err());
        assert!(s
            .create_trigger(&c.id, &bad("cron", json!({}), None))
            .is_err());
        let t = s
            .create_trigger(&c.id, &bad("schedule", json!({"daily_at": "09:30"}), None))
            .unwrap();
        assert!(t.enabled);
        assert!(s.mark_seen(&t.id, "pr-1").unwrap());
        assert!(!s.mark_seen(&t.id, "pr-1").unwrap());
        assert!(s.has_seen_any(&t.id).unwrap());

        // Too many failures in a row switch it off, and turning it on again clears them.
        assert!(!s.record_trigger_failure(&t.id, 3).unwrap());
        assert!(!s.record_trigger_failure(&t.id, 3).unwrap());
        assert!(s.record_trigger_failure(&t.id, 3).unwrap());
        assert!(!s.trigger(&t.id).unwrap().enabled);
        let again = s
            .update_trigger(&t.id, &bad("schedule", json!({"daily_at": "09:30"}), None))
            .unwrap();
        assert!(again.enabled);
        assert_eq!(again.failures, 0);
    }

    #[test]
    fn times_are_parsed() {
        assert_eq!(parse_time("09:30"), Some(570));
        assert_eq!(parse_time("0:00"), Some(0));
        assert_eq!(parse_time("24:00"), None);
        assert_eq!(parse_time("9"), None);
    }

    #[test]
    fn integrations_are_kept_cleaned_and_cleared() {
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::open(&dir.path().join("t.db")).unwrap());
        let store = ComposerStore::new(db, dir.path());
        let made = store
            .create(&ComposerInput {
                name: "J".into(),
                integrations: vec!["jira".into(), "jira".into(), "evil".into()],
                ..Default::default()
            })
            .unwrap();
        assert_eq!(made.integrations, ["jira"]);
        store.clear_integration("jira").unwrap();
        assert!(store.get(&made.id).unwrap().integrations.is_empty());
    }

    #[test]
    fn a_jira_trigger_needs_a_query_and_no_project() {
        let make = |config: serde_json::Value| TriggerInput {
            kind: "jira_issue".into(),
            config,
            project_id: None,
            prompt: String::new(),
            enabled: true,
        };
        assert!(check_trigger(&make(serde_json::json!({}))).is_err());
        assert!(check_trigger(&make(serde_json::json!({"jql": "  "}))).is_err());
        assert!(check_trigger(&make(serde_json::json!({"jql": "x".repeat(2001)}))).is_err());
        assert!(check_trigger(&make(serde_json::json!({"jql": "project = P"}))).is_ok());
    }
}
