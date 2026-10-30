//! Sessions, raw events and chat messages in the database.
//!
//! Every line Claude Code prints, and every line we send it, is stored in `events`. The chat is
//! built from `messages`, a projection of those events. Raw events are the source of truth.

use std::sync::Arc;

use rusqlite::{params, OptionalExtension, Row};
use serde::Serialize;
use serde_json::Value;
use ts_rs::TS;

use super::parser::{MessageKind, NewMessage, Role};
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::ipc::{new_id, now_ms};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Idle,
    Working,
    /// Waiting for the user to answer a permission request.
    Blocked,
    /// The process ended while a turn was running.
    Interrupted,
    /// The chat tab was closed.
    Closed,
}

impl SessionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            SessionStatus::Idle => "idle",
            SessionStatus::Working => "working",
            SessionStatus::Blocked => "blocked",
            SessionStatus::Interrupted => "interrupted",
            SessionStatus::Closed => "closed",
        }
    }

    fn parse(s: &str) -> SessionStatus {
        match s {
            "working" => SessionStatus::Working,
            "blocked" => SessionStatus::Blocked,
            "interrupted" => SessionStatus::Interrupted,
            "closed" => SessionStatus::Closed,
            _ => SessionStatus::Idle,
        }
    }

    pub fn is_active(self) -> bool {
        matches!(self, SessionStatus::Working | SessionStatus::Blocked)
    }
}

/// How Claude Code asks for approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    /// Ask before anything that is not already allowed.
    Manual,
    /// File edits go through, other tools ask.
    AcceptEdits,
    /// Claude only plans and does not change anything until the plan is approved.
    Plan,
    /// Nothing asks. Claude can run any command.
    Bypass,
}

impl PermissionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            PermissionMode::Manual => "manual",
            PermissionMode::AcceptEdits => "accept_edits",
            PermissionMode::Plan => "plan",
            PermissionMode::Bypass => "bypass",
        }
    }

    /// The value of `claude --permission-mode`.
    pub fn cli_value(self) -> &'static str {
        match self {
            PermissionMode::Manual => "manual",
            PermissionMode::AcceptEdits => "acceptEdits",
            PermissionMode::Plan => "plan",
            PermissionMode::Bypass => "bypassPermissions",
        }
    }

    pub fn parse(s: &str) -> Option<PermissionMode> {
        match s {
            "manual" => Some(PermissionMode::Manual),
            "accept_edits" => Some(PermissionMode::AcceptEdits),
            "plan" => Some(PermissionMode::Plan),
            "bypass" => Some(PermissionMode::Bypass),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[ts(export)]
pub struct Session {
    pub id: String,
    pub workspace_id: String,
    /// Claude Code's own id, used to resume. Unset until the first start.
    pub claude_session_id: Option<String>,
    pub title: Option<String>,
    pub status: SessionStatus,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub permission_mode: PermissionMode,
    #[ts(type = "number")]
    pub position: i64,
    pub total_cost_usd: f64,
    #[ts(type = "number")]
    pub turns: i64,
    /// Why the session last failed, when it did.
    pub last_error: Option<String>,
    /// A Claude Code process is running for this session right now.
    pub running: bool,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[ts(export)]
pub struct Message {
    pub id: String,
    pub session_id: String,
    #[ts(type = "number")]
    pub seq: i64,
    pub role: Role,
    pub kind: MessageKind,
    #[ts(type = "unknown")]
    pub content: Value,
    #[ts(type = "number")]
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[ts(export)]
pub struct EventRow {
    pub id: String,
    #[ts(type = "number")]
    pub seq: i64,
    /// `assistant`, `stream_event:content_block_delta`, `sent:user`, ...
    pub kind: String,
    /// The line exactly as it was received or sent.
    pub raw: String,
    #[ts(type = "number")]
    pub created_at: i64,
}

#[derive(Debug, Clone, Default)]
pub struct NewSession {
    pub title: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub permission_mode: Option<PermissionMode>,
    /// Claude session id to fork from on first start.
    pub fork_of: Option<String>,
}

const SESSION_COLUMNS: &str =
    "id, workspace_id, claude_session_id, title, status, model, effort, permission_mode, \
     position, total_cost_usd, turns, last_error, created_at, updated_at";

fn session_from_row(row: &Row<'_>) -> rusqlite::Result<Session> {
    Ok(Session {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        claude_session_id: row.get(2)?,
        title: row.get(3)?,
        status: SessionStatus::parse(&row.get::<_, String>(4)?),
        model: row.get(5)?,
        effort: row.get(6)?,
        permission_mode: row
            .get::<_, Option<String>>(7)?
            .and_then(|m| PermissionMode::parse(&m))
            .unwrap_or(PermissionMode::Manual),
        position: row.get(8)?,
        total_cost_usd: row.get(9)?,
        turns: row.get(10)?,
        last_error: row.get(11)?,
        running: false,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

fn message_from_row(row: &Row<'_>) -> rusqlite::Result<Message> {
    let role: String = row.get(3)?;
    let kind: String = row.get(4)?;
    let content: String = row.get(5)?;
    Ok(Message {
        id: row.get(0)?,
        session_id: row.get(1)?,
        seq: row.get(2)?,
        role: match role.as_str() {
            "user" => Role::User,
            "assistant" => Role::Assistant,
            _ => Role::System,
        },
        kind: match kind.as_str() {
            "thinking" => MessageKind::Thinking,
            "tool_call" => MessageKind::ToolCall,
            "tool_result" => MessageKind::ToolResult,
            "system" => MessageKind::System,
            _ => MessageKind::Text,
        },
        content: serde_json::from_str(&content).unwrap_or(Value::Null),
        created_at: row.get(6)?,
    })
}

#[derive(Clone)]
pub struct SessionStore {
    db: Arc<Db>,
}

impl SessionStore {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    pub fn create(&self, workspace_id: &str, new: NewSession) -> AppResult<Session> {
        let id = new_id();
        let now = now_ms();
        self.db.write(|c| {
            let position: i64 = c.query_row(
                "SELECT COALESCE(MAX(position) + 1, 0) FROM sessions WHERE workspace_id = ?1",
                params![workspace_id],
                |r| r.get(0),
            )?;
            c.execute(
                "INSERT INTO sessions (id, workspace_id, title, status, model, effort, permission_mode, position, fork_of, created_at, updated_at)
                 VALUES (?1, ?2, ?3, 'idle', ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
                params![
                    id,
                    workspace_id,
                    new.title,
                    new.model,
                    new.effort,
                    new.permission_mode.unwrap_or(PermissionMode::Manual).as_str(),
                    position,
                    new.fork_of,
                    now
                ],
            )
        })?;
        self.get(&id)
    }

    pub fn get(&self, id: &str) -> AppResult<Session> {
        self.db
            .read(|c| {
                c.query_row(
                    &format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?1"),
                    params![id],
                    session_from_row,
                )
            })
            .map_err(|e| match e {
                AppError::Db(rusqlite::Error::QueryReturnedNoRows) => {
                    AppError::NotFound(format!("session {id}"))
                }
                other => other,
            })
    }

    /// Sessions of a workspace in tab order. Closed ones are left out unless asked for.
    pub fn list(&self, workspace_id: &str, include_closed: bool) -> AppResult<Vec<Session>> {
        self.db.read(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {SESSION_COLUMNS} FROM sessions
                 WHERE workspace_id = ?1 AND (?2 OR status != 'closed')
                 ORDER BY position, rowid"
            ))?;
            let rows = stmt.query_map(params![workspace_id, include_closed], session_from_row)?;
            rows.collect::<Result<Vec<_>, _>>()
        })
    }

    pub fn fork_of(&self, id: &str) -> AppResult<Option<String>> {
        self.db.read(|c| {
            c.query_row(
                "SELECT fork_of FROM sessions WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
        })
    }

    pub fn set_status(
        &self,
        id: &str,
        status: SessionStatus,
        error: Option<&str>,
    ) -> AppResult<()> {
        self.db.write(|c| {
            c.execute(
                "UPDATE sessions SET status = ?1, last_error = ?2, updated_at = ?3 WHERE id = ?4",
                params![status.as_str(), error, now_ms(), id],
            )
        })?;
        Ok(())
    }

    /// Remembers Claude's own session id, and the model it reported. A fork stops being a fork
    /// once it has an id of its own.
    pub fn set_claude_session(
        &self,
        id: &str,
        claude_session_id: &str,
        model: Option<&str>,
    ) -> AppResult<()> {
        self.db.write(|c| {
            c.execute(
                "UPDATE sessions SET claude_session_id = ?1, fork_of = NULL,
                   model = COALESCE(model, ?2), updated_at = ?3 WHERE id = ?4",
                params![claude_session_id, model, now_ms(), id],
            )
        })?;
        Ok(())
    }

    /// Forgets Claude's own session id, so the next message starts a new session.
    pub fn forget_claude_session(&self, id: &str) -> AppResult<()> {
        self.db.write(|c| {
            c.execute(
                "UPDATE sessions SET claude_session_id = NULL, fork_of = NULL, updated_at = ?1
                 WHERE id = ?2",
                params![now_ms(), id],
            )
        })?;
        Ok(())
    }

    /// What the chat's agent is told before its next message, after its session was started over.
    /// Read once: it is cleared as it is taken.
    pub fn take_replay_context(&self, id: &str) -> AppResult<Option<String>> {
        self.db.write(|c| {
            let text: Option<String> = c.query_row(
                "SELECT replay_context FROM sessions WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )?;
            if text.is_some() {
                c.execute(
                    "UPDATE sessions SET replay_context = NULL WHERE id = ?1",
                    params![id],
                )?;
            }
            Ok(text.filter(|t| !t.trim().is_empty()))
        })
    }

    /// The database behind the store, for tests that need to build other services on it.
    #[cfg(test)]
    pub fn db_for_tests(&self) -> Arc<Db> {
        self.db.clone()
    }

    pub fn set_replay_context(&self, id: &str, text: Option<&str>) -> AppResult<()> {
        self.db.write(|c| {
            c.execute(
                "UPDATE sessions SET replay_context = ?1 WHERE id = ?2",
                params![text, id],
            )
        })?;
        Ok(())
    }

    pub fn record_turn(&self, id: &str, cost_usd: f64) -> AppResult<()> {
        self.db.write(|c| {
            c.execute(
                "UPDATE sessions SET turns = turns + 1, total_cost_usd = ?1, updated_at = ?2 WHERE id = ?3",
                params![cost_usd, now_ms(), id],
            )
        })?;
        Ok(())
    }

    pub fn update_options(
        &self,
        id: &str,
        model: Option<&str>,
        effort: Option<&str>,
        mode: Option<PermissionMode>,
    ) -> AppResult<Session> {
        self.get(id)?;
        self.db.write(|c| {
            c.execute(
                "UPDATE sessions SET model = COALESCE(?1, model), effort = COALESCE(?2, effort),
                   permission_mode = COALESCE(?3, permission_mode), updated_at = ?4 WHERE id = ?5",
                params![
                    model,
                    effort,
                    mode.map(PermissionMode::as_str),
                    now_ms(),
                    id
                ],
            )
        })?;
        self.get(id)
    }

    pub fn rename(&self, id: &str, title: &str) -> AppResult<Session> {
        let title = title.trim();
        if title.is_empty() || title.chars().count() > 60 {
            return Err(AppError::Invalid(
                "Chat names need 1 to 60 characters.".into(),
            ));
        }
        self.get(id)?;
        // A person's name. It is never replaced by a made-up one.
        self.db.write(|c| {
            c.execute(
                "UPDATE sessions SET title = ?1, title_source = 'user' WHERE id = ?2",
                params![title, id],
            )
        })?;
        self.get(id)
    }

    /// Gives a chat that has no name yet one made from its first message. Does nothing when the
    /// chat is named already. Returns whether it was set.
    pub fn set_first_title(&self, id: &str, title: &str) -> AppResult<bool> {
        let title = title.trim();
        if title.is_empty() || title.chars().count() > 60 {
            return Ok(false);
        }
        let rows = self.db.write(|c| {
            c.execute(
                "UPDATE sessions SET title = ?1, title_source = 'auto'
                 WHERE id = ?2 AND (title IS NULL OR title = '')",
                params![title, id],
            )
        })?;
        Ok(rows > 0)
    }

    /// Replaces a made-up name with a better made-up one, never a name a person gave. Returns
    /// whether it was replaced.
    pub fn replace_auto_title(&self, id: &str, title: &str) -> AppResult<bool> {
        let title = title.trim();
        if title.is_empty() || title.chars().count() > 60 {
            return Ok(false);
        }
        let rows = self.db.write(|c| {
            c.execute(
                "UPDATE sessions SET title = ?1 WHERE id = ?2 AND title_source = 'auto'",
                params![title, id],
            )
        })?;
        Ok(rows > 0)
    }

    /// Puts the chats in the given order. Ids that are not in the workspace are ignored.
    pub fn reorder(&self, workspace_id: &str, ids: &[String]) -> AppResult<()> {
        self.db.write(|c| {
            for (position, id) in ids.iter().enumerate() {
                c.execute(
                    "UPDATE sessions SET position = ?1 WHERE id = ?2 AND workspace_id = ?3",
                    params![position as i64, id, workspace_id],
                )?;
            }
            Ok(())
        })?;
        Ok(())
    }

    /// Marks sessions that were mid turn when the app stopped. Returns their workspaces.
    pub fn interrupt_stale(&self) -> AppResult<Vec<(String, String)>> {
        self.db.write(|c| {
            let stale: Vec<(String, String)> = {
                let mut stmt = c.prepare("SELECT id, workspace_id FROM sessions WHERE status IN ('working', 'blocked')")?;
                let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
                rows.collect::<Result<_, _>>()?
            };
            c.execute(
                "UPDATE sessions SET status = 'interrupted', last_error = 'Circles stopped while this chat was working.', updated_at = ?1
                 WHERE status IN ('working', 'blocked')",
                params![now_ms()],
            )?;
            Ok(stale)
        })
    }

    // Events and messages

    /// Stores one raw line. Returns the event id and its sequence number in the session.
    pub fn append_event(
        &self,
        session_id: &str,
        kind: &str,
        raw: &str,
    ) -> AppResult<(String, i64)> {
        let id = new_id();
        let seq = self.db.write(|c| {
            let seq: i64 = c.query_row(
                "SELECT COALESCE(MAX(seq), 0) + 1 FROM events WHERE session_id = ?1",
                params![session_id],
                |r| r.get(0),
            )?;
            c.execute(
                "INSERT INTO events (id, session_id, seq, kind, raw, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id, session_id, seq, kind, raw, now_ms()],
            )?;
            Ok(seq)
        })?;
        Ok((id, seq))
    }

    pub fn append_messages(
        &self,
        session_id: &str,
        event_id: Option<&str>,
        messages: &[NewMessage],
    ) -> AppResult<Vec<Message>> {
        if messages.is_empty() {
            return Ok(Vec::new());
        }
        let now = now_ms();
        self.db.write(|c| {
            let tx = c.unchecked_transaction()?;
            let mut seq: i64 = tx.query_row(
                "SELECT COALESCE(MAX(seq), 0) FROM messages WHERE session_id = ?1",
                params![session_id],
                |r| r.get(0),
            )?;
            let mut stored = Vec::with_capacity(messages.len());
            for message in messages {
                seq += 1;
                let id = new_id();
                tx.execute(
                    "INSERT INTO messages (id, session_id, event_id, seq, role, kind, content, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        id,
                        session_id,
                        event_id,
                        seq,
                        message.role.as_str(),
                        message.kind.as_str(),
                        message.content.to_string(),
                        now
                    ],
                )?;
                stored.push(Message {
                    id,
                    session_id: session_id.to_string(),
                    seq,
                    role: message.role,
                    kind: message.kind,
                    content: message.content.clone(),
                    created_at: now,
                });
            }
            tx.commit()?;
            Ok(stored)
        })
    }

    /// Messages after `after_seq`, oldest first.
    pub fn messages(&self, session_id: &str, after_seq: Option<i64>) -> AppResult<Vec<Message>> {
        self.db.read(|c| {
            let mut stmt = c.prepare(
                "SELECT id, session_id, seq, role, kind, content, created_at FROM messages
                 WHERE session_id = ?1 AND seq > ?2 ORDER BY seq",
            )?;
            let rows = stmt.query_map(
                params![session_id, after_seq.unwrap_or(0)],
                message_from_row,
            )?;
            rows.collect::<Result<Vec<_>, _>>()
        })
    }

    /// Raw events after `after_seq`, oldest first.
    pub fn events(
        &self,
        session_id: &str,
        after_seq: Option<i64>,
        limit: u32,
    ) -> AppResult<Vec<EventRow>> {
        self.db.read(|c| {
            let mut stmt = c.prepare(
                "SELECT id, seq, kind, raw, created_at FROM events
                 WHERE session_id = ?1 AND seq > ?2 ORDER BY seq LIMIT ?3",
            )?;
            let rows = stmt.query_map(params![session_id, after_seq.unwrap_or(0), limit], |r| {
                Ok(EventRow {
                    id: r.get(0)?,
                    seq: r.get(1)?,
                    kind: r.get(2)?,
                    raw: r.get(3)?,
                    created_at: r.get(4)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()
        })
    }

    pub fn event_count(&self, session_id: &str) -> AppResult<i64> {
        self.db.read(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM events WHERE session_id = ?1",
                params![session_id],
                |r| r.get(0),
            )
        })
    }

    /// The workspace's sessions that are working or blocked, to work out the workspace's state.
    pub fn active_statuses(&self, workspace_id: &str) -> AppResult<Vec<SessionStatus>> {
        Ok(self
            .list(workspace_id, false)?
            .into_iter()
            .map(|s| s.status)
            .collect())
    }

    pub fn workspace_of(&self, session_id: &str) -> AppResult<String> {
        self.db
            .read(|c| {
                c.query_row(
                    "SELECT workspace_id FROM sessions WHERE id = ?1",
                    params![session_id],
                    |r| r.get(0),
                )
                .optional()
            })?
            .ok_or_else(|| AppError::NotFound(format!("session {session_id}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::parser::{normalize, parse_line, Block, Event};

    fn store() -> (tempfile::TempDir, SessionStore) {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::open(&tmp.path().join("t.db")).unwrap());
        db.write(|c| {
            c.execute_batch(
                "INSERT INTO projects VALUES ('p','n','/p','main',NULL,1,1);
                 INSERT INTO workspaces (id,project_id,city,branch,base_branch,path,state,created_at,updated_at)
                   VALUES ('w1','p','Oslo','b','main','/w1','ready',1,1),
                          ('w2','p','Lima','b2','main','/w2','ready',1,1);",
            )
        })
        .unwrap();
        (tmp, SessionStore::new(db))
    }

    #[test]
    fn creates_sessions_in_tab_order_with_defaults() {
        let (_tmp, store) = store();
        let a = store.create("w1", NewSession::default()).unwrap();
        let b = store
            .create(
                "w1",
                NewSession {
                    title: Some("Refactor".into()),
                    model: Some("opus".into()),
                    effort: Some("high".into()),
                    permission_mode: Some(PermissionMode::AcceptEdits),
                    fork_of: None,
                },
            )
            .unwrap();
        assert_eq!(
            (a.status, a.permission_mode, a.position),
            (SessionStatus::Idle, PermissionMode::Manual, 0)
        );
        assert_eq!(
            (b.position, b.permission_mode),
            (1, PermissionMode::AcceptEdits)
        );
        assert_eq!(
            (b.title.as_deref(), b.model.as_deref(), b.effort.as_deref()),
            (Some("Refactor"), Some("opus"), Some("high"))
        );
        assert!(a.claude_session_id.is_none() && !a.running);
        assert_eq!(store.list("w1", false).unwrap().len(), 2);
        assert!(store.list("w2", false).unwrap().is_empty());
        assert_eq!(store.get("nope").unwrap_err().code(), "not_found");
    }

    #[test]
    fn closed_sessions_are_hidden_unless_asked_for() {
        let (_tmp, store) = store();
        let a = store.create("w1", NewSession::default()).unwrap();
        store.create("w1", NewSession::default()).unwrap();
        store
            .set_status(&a.id, SessionStatus::Closed, None)
            .unwrap();
        assert_eq!(store.list("w1", false).unwrap().len(), 1);
        assert_eq!(store.list("w1", true).unwrap().len(), 2);
    }

    #[test]
    fn status_and_errors_are_stored() {
        let (_tmp, store) = store();
        let s = store.create("w1", NewSession::default()).unwrap();
        store
            .set_status(&s.id, SessionStatus::Working, None)
            .unwrap();
        assert_eq!(store.get(&s.id).unwrap().status, SessionStatus::Working);
        store
            .set_status(&s.id, SessionStatus::Interrupted, Some("the process ended"))
            .unwrap();
        let after = store.get(&s.id).unwrap();
        assert_eq!(
            (after.status, after.last_error.as_deref()),
            (SessionStatus::Interrupted, Some("the process ended"))
        );
        store.set_status(&s.id, SessionStatus::Idle, None).unwrap();
        assert_eq!(store.get(&s.id).unwrap().last_error, None);
    }

    #[test]
    fn claude_session_id_is_kept_and_ends_a_fork() {
        let (_tmp, store) = store();
        let s = store
            .create(
                "w1",
                NewSession {
                    fork_of: Some("parent-id".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(store.fork_of(&s.id).unwrap().as_deref(), Some("parent-id"));
        store
            .set_claude_session(&s.id, "own-id", Some("claude-sonnet-5-5"))
            .unwrap();
        let after = store.get(&s.id).unwrap();
        assert_eq!(after.claude_session_id.as_deref(), Some("own-id"));
        assert_eq!(after.model.as_deref(), Some("claude-sonnet-5-5"));
        assert_eq!(store.fork_of(&s.id).unwrap(), None);
        // A model the user chose is not replaced by the one Claude reports.
        let chosen = store
            .create(
                "w1",
                NewSession {
                    model: Some("opus".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        store
            .set_claude_session(&chosen.id, "x", Some("claude-opus-5-5"))
            .unwrap();
        assert_eq!(
            store.get(&chosen.id).unwrap().model.as_deref(),
            Some("opus")
        );
    }

    #[test]
    fn turns_and_cost_accumulate_from_the_reported_totals() {
        let (_tmp, store) = store();
        let s = store.create("w1", NewSession::default()).unwrap();
        store.record_turn(&s.id, 0.10).unwrap();
        store.record_turn(&s.id, 0.25).unwrap();
        let after = store.get(&s.id).unwrap();
        assert_eq!(after.turns, 2);
        assert!(
            (after.total_cost_usd - 0.25).abs() < 1e-9,
            "Claude reports a running total"
        );
    }

    #[test]
    fn options_change_independently_and_names_are_checked() {
        let (_tmp, store) = store();
        let s = store
            .create(
                "w1",
                NewSession {
                    model: Some("opus".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        let after = store
            .update_options(&s.id, None, Some("max"), Some(PermissionMode::Plan))
            .unwrap();
        assert_eq!(
            (
                after.model.as_deref(),
                after.effort.as_deref(),
                after.permission_mode
            ),
            (Some("opus"), Some("max"), PermissionMode::Plan)
        );
        assert_eq!(
            store
                .rename(&s.id, "  Fix login ")
                .unwrap()
                .title
                .as_deref(),
            Some("Fix login")
        );
        assert_eq!(store.rename(&s.id, "  ").unwrap_err().code(), "invalid");
        assert_eq!(
            store.rename(&s.id, &"x".repeat(61)).unwrap_err().code(),
            "invalid"
        );
    }

    #[test]
    fn permission_modes_map_to_the_cli() {
        for (mode, id, cli) in [
            (PermissionMode::Manual, "manual", "manual"),
            (PermissionMode::AcceptEdits, "accept_edits", "acceptEdits"),
            (PermissionMode::Plan, "plan", "plan"),
            (PermissionMode::Bypass, "bypass", "bypassPermissions"),
        ] {
            assert_eq!(mode.as_str(), id);
            assert_eq!(mode.cli_value(), cli);
            assert_eq!(PermissionMode::parse(id), Some(mode));
        }
        assert_eq!(PermissionMode::parse("nope"), None);
    }

    #[test]
    fn events_are_numbered_per_session_and_read_back_in_order() {
        let (_tmp, store) = store();
        let a = store.create("w1", NewSession::default()).unwrap();
        let b = store.create("w1", NewSession::default()).unwrap();
        let (_, s1) = store
            .append_event(&a.id, "system:init", r#"{"type":"system"}"#)
            .unwrap();
        let (_, s2) = store
            .append_event(&a.id, "assistant", r#"{"type":"assistant"}"#)
            .unwrap();
        let (_, other) = store.append_event(&b.id, "assistant", "{}").unwrap();
        assert_eq!((s1, s2, other), (1, 2, 1));

        let events = store.events(&a.id, None, 100).unwrap();
        assert_eq!(
            events
                .iter()
                .map(|e| (e.seq, e.kind.as_str()))
                .collect::<Vec<_>>(),
            [(1, "system:init"), (2, "assistant")]
        );
        assert_eq!(
            events[1].raw, r#"{"type":"assistant"}"#,
            "the raw line is kept exactly"
        );
        assert_eq!(store.events(&a.id, Some(1), 100).unwrap().len(), 1);
        assert_eq!(store.events(&a.id, None, 1).unwrap().len(), 1);
        assert_eq!(store.event_count(&a.id).unwrap(), 2);
    }

    #[test]
    fn messages_are_numbered_and_filtered_by_position() {
        let (_tmp, store) = store();
        let s = store.create("w1", NewSession::default()).unwrap();
        let (event_id, _) = store.append_event(&s.id, "assistant", "{}").unwrap();
        let first = store
            .append_messages(
                &s.id,
                Some(&event_id),
                &[
                    NewMessage {
                        role: Role::Assistant,
                        kind: MessageKind::Text,
                        content: serde_json::json!({"text": "hi"}),
                    },
                    NewMessage {
                        role: Role::Assistant,
                        kind: MessageKind::ToolCall,
                        content: serde_json::json!({"id": "t1", "name": "Read", "input": {}}),
                    },
                ],
            )
            .unwrap();
        assert_eq!(first.iter().map(|m| m.seq).collect::<Vec<_>>(), [1, 2]);
        let second = store
            .append_messages(
                &s.id,
                None,
                &[NewMessage {
                    role: Role::User,
                    kind: MessageKind::ToolResult,
                    content: serde_json::json!({"tool_use_id": "t1"}),
                }],
            )
            .unwrap();
        assert_eq!(second[0].seq, 3);
        assert!(store.append_messages(&s.id, None, &[]).unwrap().is_empty());

        let all = store.messages(&s.id, None).unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(
            (all[1].kind, all[1].content["name"].as_str()),
            (MessageKind::ToolCall, Some("Read"))
        );
        assert_eq!(store.messages(&s.id, Some(2)).unwrap().len(), 1);
        assert_eq!(all[0], first[0], "what was stored reads back the same");
    }

    #[test]
    fn a_real_turn_can_be_stored_and_read_back() {
        let (_tmp, store) = store();
        let s = store.create("w1", NewSession::default()).unwrap();
        for line in include_str!("fixtures/tool_turn.jsonl").lines() {
            let parsed = parse_line(line).unwrap();
            let (event_id, _) = store.append_event(&s.id, &parsed.kind, line).unwrap();
            store
                .append_messages(&s.id, Some(&event_id), &normalize(&parsed.event))
                .unwrap();
        }
        assert_eq!(store.event_count(&s.id).unwrap(), 7);
        let messages = store.messages(&s.id, None).unwrap();
        assert_eq!(
            messages.iter().map(|m| m.kind).collect::<Vec<_>>(),
            [
                MessageKind::ToolCall,
                MessageKind::ToolResult,
                MessageKind::Text
            ]
        );

        // Messages can always be rebuilt from the raw events.
        let rebuilt: Vec<_> = store
            .events(&s.id, None, 100)
            .unwrap()
            .iter()
            .flat_map(|e| normalize(&parse_line(&e.raw).unwrap().event))
            .collect();
        assert_eq!(rebuilt.len(), messages.len());
        assert!(
            matches!(parse_line(&store.events(&s.id, None, 100).unwrap()[1].raw).unwrap().event, Event::Assistant(ref a) if matches!(a.blocks[0], Block::ToolUse { .. }))
        );
    }

    #[test]
    fn sessions_left_working_by_a_crash_are_marked_interrupted() {
        let (_tmp, store) = store();
        let working = store.create("w1", NewSession::default()).unwrap();
        let blocked = store.create("w2", NewSession::default()).unwrap();
        let idle = store.create("w1", NewSession::default()).unwrap();
        store
            .set_status(&working.id, SessionStatus::Working, None)
            .unwrap();
        store
            .set_status(&blocked.id, SessionStatus::Blocked, None)
            .unwrap();

        let mut stale = store.interrupt_stale().unwrap();
        stale.sort();
        // Ids made in the same millisecond have no fixed order, so sort what is expected too.
        let mut expected = vec![
            (working.id.clone(), "w1".to_string()),
            (blocked.id.clone(), "w2".to_string()),
        ];
        expected.sort();
        assert_eq!(stale, expected);
        assert_eq!(
            store.get(&working.id).unwrap().status,
            SessionStatus::Interrupted
        );
        assert!(store.get(&working.id).unwrap().last_error.is_some());
        assert_eq!(store.get(&idle.id).unwrap().status, SessionStatus::Idle);
        assert!(store.interrupt_stale().unwrap().is_empty());
    }

    #[test]
    fn deleting_a_workspace_removes_its_sessions_events_and_messages() {
        let (_tmp, store) = store();
        let s = store.create("w1", NewSession::default()).unwrap();
        let (event_id, _) = store.append_event(&s.id, "assistant", "{}").unwrap();
        store
            .append_messages(
                &s.id,
                Some(&event_id),
                &[NewMessage {
                    role: Role::Assistant,
                    kind: MessageKind::Text,
                    content: serde_json::json!({"text": "x"}),
                }],
            )
            .unwrap();
        store
            .db
            .write(|c| c.execute("DELETE FROM workspaces WHERE id = 'w1'", []))
            .unwrap();
        assert_eq!(store.get(&s.id).unwrap_err().code(), "not_found");
        assert_eq!(store.event_count(&s.id).unwrap(), 0);
        assert!(store.messages(&s.id, None).unwrap().is_empty());
    }

    #[test]
    fn the_workspace_of_a_session_is_known() {
        let (_tmp, store) = store();
        let s = store.create("w2", NewSession::default()).unwrap();
        assert_eq!(store.workspace_of(&s.id).unwrap(), "w2");
        assert_eq!(store.workspace_of("nope").unwrap_err().code(), "not_found");
        assert_eq!(store.active_statuses("w2").unwrap(), [SessionStatus::Idle]);
    }

    #[test]
    fn a_made_up_title_is_replaced_but_a_persons_name_never_is() {
        let (_tmp, store) = store();
        let chat = store.create("w1", NewSession::default()).unwrap();
        assert!(chat.title.is_none());

        // The first words, then a model's title in their place.
        assert!(store
            .set_first_title(&chat.id, "fix the failing test in")
            .unwrap());
        assert!(store
            .replace_auto_title(&chat.id, "Fix failing checkout test")
            .unwrap());
        assert_eq!(
            store.get(&chat.id).unwrap().title.as_deref(),
            Some("Fix failing checkout test")
        );

        // It is named already, so no first title is set again.
        assert!(!store.set_first_title(&chat.id, "something else").unwrap());

        // A person renames it. Nothing made up can replace that.
        store.rename(&chat.id, "My name").unwrap();
        assert!(!store
            .replace_auto_title(&chat.id, "A model's idea")
            .unwrap());
        assert_eq!(
            store.get(&chat.id).unwrap().title.as_deref(),
            Some("My name")
        );
    }

    #[test]
    fn a_chat_created_with_a_title_is_treated_as_named_by_a_person() {
        let (_tmp, store) = store();
        let chat = store
            .create(
                "w1",
                NewSession {
                    title: Some("Casablanca".into()),
                    ..NewSession::default()
                },
            )
            .unwrap();
        assert!(!store.replace_auto_title(&chat.id, "Replaced").unwrap());
        assert_eq!(
            store.get(&chat.id).unwrap().title.as_deref(),
            Some("Casablanca")
        );
    }

    #[test]
    fn empty_and_overlong_titles_are_not_set() {
        let (_tmp, store) = store();
        let chat = store.create("w1", NewSession::default()).unwrap();
        assert!(!store.set_first_title(&chat.id, "   ").unwrap());
        assert!(!store.set_first_title(&chat.id, &"x".repeat(61)).unwrap());
        assert!(store.get(&chat.id).unwrap().title.is_none());
    }
}
