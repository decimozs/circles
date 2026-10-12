//! Checkpoints: a snapshot before every turn, and a safe way back.
//!
//! Before each turn the working tree is written to a commit that only a ref under
//! `refs/circles/checkpoints/` points at, so the branch, its history and the index are untouched.
//! Reverting puts the files back and removes that turn and the later ones from the conversation.
//! The state just before a revert is kept too, so a revert can be undone for a while.

pub mod snapshot;

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use serde_json::Value;
use ts_rs::TS;

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::git::Git;
use crate::harness::sessions::TurnHook;
use crate::ipc::{new_id, now_ms};
use snapshot::FileChange;

/// How long a revert can be undone.
pub const UNDO_WINDOW_MS: i64 = 60 * 60 * 1000;
/// Checkpoints older than this many days are removed.
pub const KEEP_DAYS: i64 = 30;

/// How many days to keep checkpoints, from the user's settings file. A missing, broken or
/// out-of-range value means the usual 30, so a problem with the file never stops the cleanup.
pub fn keep_days_from_settings(file: &std::path::Path) -> i64 {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
        .and_then(|table| table.get("advanced")?.get("checkpoint_days")?.as_integer())
        .filter(|days| (1..=365).contains(days))
        .unwrap_or(KEEP_DAYS)
}
/// How much of the earlier conversation is handed to a chat that starts over.
pub const TRANSCRIPT_LIMIT: usize = 60_000;

fn ref_for(workspace_id: &str, session_id: &str, turn: i64) -> String {
    format!("refs/circles/checkpoints/{workspace_id}/{session_id}/{turn}")
}

fn pre_revert_ref(workspace_id: &str, revert_id: &str) -> String {
    format!("refs/circles/checkpoints/{workspace_id}/pre-revert/{revert_id}")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct Checkpoint {
    pub id: String,
    pub workspace_id: String,
    pub session_id: Option<String>,
    #[ts(type = "number")]
    pub turn: i64,
    pub claude_session_id: Option<String>,
    /// The user message the turn started with. Unset until that message is stored.
    #[ts(type = "number | null")]
    pub message_seq: Option<i64>,
    #[ts(type = "number")]
    pub created_at: i64,
    /// Large untracked files that are not part of the snapshot.
    pub skipped: Vec<String>,
}

/// What a revert would do, for the confirmation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct RevertPreview {
    pub checkpoint: Checkpoint,
    pub files: Vec<FileChange>,
    /// Turns that go along with this one, not counting it.
    pub later_turns: u32,
    /// Other chats of the workspace with a turn running: they share its files.
    pub other_chats_active: u32,
    /// A turn is running in this chat, so it cannot be reverted.
    pub running: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct RevertOutcome {
    pub revert_id: String,
    pub files: Vec<FileChange>,
}

/// A revert that can still be undone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct UndoableRevert {
    pub id: String,
    #[ts(type = "number")]
    pub turn: i64,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub expires_at: i64,
}

#[derive(Clone)]
pub struct Checkpoints {
    pub git: Git,
    pub db: Arc<Db>,
}

const COLUMNS: &str =
    "id, workspace_id, session_id, turn, claude_session_id, message_seq, created_at, skipped";

fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Checkpoint> {
    let skipped: Option<String> = row.get(7)?;
    Ok(Checkpoint {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        session_id: row.get(2)?,
        turn: row.get(3)?,
        claude_session_id: row.get(4)?,
        message_seq: row.get(5)?,
        created_at: row.get(6)?,
        skipped: skipped
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default(),
    })
}

/// The earlier conversation as text, to tell a chat that starts over what was said. Only what was
/// said and which tools were used, not their output. When it is too long the oldest part is left
/// out.
pub fn transcript(messages: &[(String, String, Value)], limit: usize) -> String {
    let mut lines: Vec<String> = Vec::new();
    for (role, kind, content) in messages {
        let text = content
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        match (role.as_str(), kind.as_str()) {
            ("user", "text") if !text.is_empty() => lines.push(format!("User: {text}")),
            ("assistant", "text") if !text.is_empty() => lines.push(format!("Assistant: {text}")),
            ("assistant", "tool_call") => {
                let name = content
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("a tool");
                let input = content.get("input").cloned().unwrap_or(Value::Null);
                let detail = ["file_path", "command", "pattern", "path"]
                    .iter()
                    .find_map(|k| input.get(k).and_then(Value::as_str))
                    .unwrap_or("");
                let detail: String = detail.chars().take(120).collect();
                lines.push(if detail.is_empty() {
                    format!("[Assistant used {name}]")
                } else {
                    format!("[Assistant used {name}: {detail}]")
                });
            }
            _ => {}
        }
    }
    let mut kept: Vec<&str> = Vec::new();
    let mut used = 0usize;
    let mut cut = false;
    for line in lines.iter().rev() {
        let size = line.chars().count() + 1;
        if used + size > limit {
            cut = true;
            break;
        }
        used += size;
        kept.push(line);
    }
    kept.reverse();
    if kept.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "The conversation was rolled back to an earlier point. Below is what was said and done up to there. Treat it as your memory of the conversation. The files in the workspace are as they were at that point.\n\n--- Earlier conversation ---\n",
    );
    if cut {
        out.push_str("(Earlier messages are left out because they are long.)\n");
    }
    out.push_str(&kept.join("\n"));
    out.push_str("\n--- End of earlier conversation ---\n\nThe user's new message:");
    out
}

impl Checkpoints {
    pub fn new(git: Git, db: Arc<Db>) -> Self {
        Self { git, db }
    }

    pub fn get(&self, id: &str) -> AppResult<Checkpoint> {
        self.db
            .read(|c| {
                c.query_row(
                    &format!("SELECT {COLUMNS} FROM checkpoints WHERE id = ?1"),
                    params![id],
                    from_row,
                )
            })
            .map_err(|e| match e {
                AppError::Db(rusqlite::Error::QueryReturnedNoRows) => {
                    AppError::NotFound("That checkpoint no longer exists.".into())
                }
                other => other,
            })
    }

    /// The turns of a chat that can be reverted to, oldest first.
    pub fn list(&self, session_id: &str) -> AppResult<Vec<Checkpoint>> {
        self.db.read(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {COLUMNS} FROM checkpoints
                 WHERE session_id = ?1 AND kind = 'turn' AND turn > 0 ORDER BY turn"
            ))?;
            let rows = stmt.query_map(params![session_id], from_row)?;
            rows.collect()
        })
    }

    fn session_status(&self, session_id: &str) -> AppResult<String> {
        self.db.read(|c| {
            c.query_row(
                "SELECT status FROM sessions WHERE id = ?1",
                params![session_id],
                |r| r.get(0),
            )
        })
    }

    fn is_active(status: &str) -> bool {
        status == "working" || status == "blocked"
    }

    /// Takes the snapshot for the turn that is about to start and records it. The row learns the
    /// message of the turn from `attach_message`.
    pub async fn record_turn(
        &self,
        workspace_id: &str,
        session_id: &str,
        claude_session_id: Option<&str>,
        cwd: &Path,
    ) -> AppResult<String> {
        let turn: i64 = self.db.read(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM messages
                 WHERE session_id = ?1 AND role = 'user' AND kind = 'text'
                   AND COALESCE(json_extract(content, '$.steered'), 0) = 0",
                params![session_id],
                |r| r.get::<_, i64>(0),
            )
        })? + 1;
        let event_seq: i64 = self.db.read(|c| {
            c.query_row(
                "SELECT COALESCE(MAX(seq), 0) FROM events WHERE session_id = ?1",
                params![session_id],
                |r| r.get(0),
            )
        })?;

        let snap = snapshot::snapshot(&self.git, cwd, &format!("Before turn {turn}")).await?;
        let name = ref_for(workspace_id, session_id, turn);
        snapshot::set_ref(&self.git, cwd, &name, &snap.commit).await?;

        let id = new_id();
        let skipped = serde_json::to_string(&snap.skipped).unwrap_or_else(|_| "[]".into());
        self.db.write(|c| {
            // A new turn ends the chance to undo an earlier revert, and clears what it left.
            c.execute(
                "DELETE FROM checkpoints WHERE session_id = ?1 AND kind = 'reverted'",
                params![session_id],
            )?;
            c.execute(
                "INSERT OR REPLACE INTO checkpoints
                   (id, workspace_id, session_id, turn, ref_name, claude_session_id, event_seq, skipped, kind, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'turn', ?9)",
                params![id, workspace_id, session_id, turn, name, claude_session_id, event_seq, skipped, now_ms()],
            )
        })?;
        Ok(id)
    }

    pub fn attach_message(&self, id: &str, seq: i64) -> AppResult<()> {
        self.db.write(|c| {
            c.execute(
                "UPDATE checkpoints SET message_seq = ?1 WHERE id = ?2",
                params![seq, id],
            )
        })?;
        Ok(())
    }

    fn ref_of(&self, id: &str) -> AppResult<String> {
        self.db
            .read(|c| {
                c.query_row(
                    "SELECT ref_name FROM checkpoints WHERE id = ?1",
                    params![id],
                    |r| r.get(0),
                )
            })
            .map_err(|_| AppError::NotFound("That checkpoint no longer exists.".into()))
    }

    /// What a revert to `id` would change, for the confirmation dialog.
    pub async fn preview(&self, id: &str, cwd: &Path) -> AppResult<RevertPreview> {
        let checkpoint = self.get(id)?;
        let session_id = checkpoint
            .session_id
            .clone()
            .ok_or_else(|| AppError::Invalid("That checkpoint belongs to no chat.".into()))?;
        let files = snapshot::preview(&self.git, cwd, &self.ref_of(id)?).await?;
        let later_turns: u32 = self.db.read(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM checkpoints WHERE session_id = ?1 AND kind = 'turn' AND turn > ?2",
                params![session_id, checkpoint.turn],
                |r| r.get(0),
            )
        })?;
        let other_chats_active: u32 = self.db.read(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM sessions WHERE workspace_id = ?1 AND id != ?2
                   AND status IN ('working', 'blocked')",
                params![checkpoint.workspace_id, session_id],
                |r| r.get(0),
            )
        })?;
        let running = Self::is_active(&self.session_status(&session_id)?);
        Ok(RevertPreview {
            checkpoint,
            files,
            later_turns,
            other_chats_active,
            running,
        })
    }

    fn retained_transcript(&self, session_id: &str, before_seq: i64) -> AppResult<String> {
        let rows: Vec<(String, String, Value)> = self.db.read(|c| {
            let mut stmt = c.prepare(
                "SELECT role, kind, content FROM messages WHERE session_id = ?1 AND seq < ?2 ORDER BY seq",
            )?;
            let rows = stmt.query_map(params![session_id, before_seq], |r| {
                let content: String = r.get(2)?;
                Ok((r.get(0)?, r.get(1)?, serde_json::from_str(&content).unwrap_or(Value::Null)))
            })?;
            rows.collect()
        })?;
        Ok(transcript(&rows, TRANSCRIPT_LIMIT))
    }

    /// The earlier conversation of a chat as text, up to but not including `before_seq`, for a
    /// chat that branches off from there.
    pub fn context_before(&self, session_id: &str, before_seq: i64) -> AppResult<Option<String>> {
        let text = self.retained_transcript(session_id, before_seq)?;
        Ok((!text.is_empty()).then_some(text))
    }

    /// Puts the files and the conversation back to before turn `id`, removing that turn and the
    /// later ones. The state before the revert is kept so it can be undone.
    pub async fn revert(&self, id: &str, cwd: &Path) -> AppResult<RevertOutcome> {
        let checkpoint = self.get(id)?;
        let session_id = checkpoint
            .session_id
            .clone()
            .ok_or_else(|| AppError::Invalid("That checkpoint belongs to no chat.".into()))?;
        let message_seq = checkpoint.message_seq.ok_or_else(|| {
            AppError::Invalid("That turn never started, so there is nothing to go back to.".into())
        })?;
        if Self::is_active(&self.session_status(&session_id)?) {
            return Err(AppError::Invalid(
                "A turn is running in this chat. Stop it first, then revert.".into(),
            ));
        }
        let (old_claude, old_replay): (Option<String>, Option<String>) = self.db.read(|c| {
            c.query_row(
                "SELECT claude_session_id, replay_context FROM sessions WHERE id = ?1",
                params![session_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
        })?;
        let event_seq: i64 = self.db.read(|c| {
            c.query_row(
                "SELECT COALESCE(event_seq, 0) FROM checkpoints WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
        })?;

        // Keep the present first, so everything after this can be taken back.
        let revert_id = new_id();
        let pre = snapshot::snapshot(
            &self.git,
            cwd,
            &format!("Before reverting to turn {}", checkpoint.turn),
        )
        .await?;
        let pre_name = pre_revert_ref(&checkpoint.workspace_id, &revert_id);
        snapshot::set_ref(&self.git, cwd, &pre_name, &pre.commit).await?;

        let target = self.ref_of(id)?;
        let staging = cwd
            .join(".context")
            .join("checkpoint-staging")
            .join(&revert_id);
        let files = match snapshot::restore(&self.git, cwd, &target, &staging).await {
            Ok(files) => files,
            Err(e) => {
                // Anything half done goes back to how it was.
                let _ = snapshot::restore(&self.git, cwd, &pre.commit, &staging).await;
                let _ = snapshot::delete_ref(&self.git, cwd, &pre_name).await;
                let _ = std::fs::remove_dir_all(&staging);
                return Err(e);
            }
        };

        let replay = self.retained_transcript(&session_id, message_seq)?;
        let now = now_ms();
        self.db.write(|c| {
            let tx = c.unchecked_transaction()?;
            tx.execute(
                "INSERT INTO reverts (id, workspace_id, session_id, turn, message_seq, pre_revert_ref,
                                      claude_session_id, replay_context, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![revert_id, checkpoint.workspace_id, session_id, checkpoint.turn, message_seq, pre_name, old_claude, old_replay, now],
            )?;
            tx.execute(
                "INSERT INTO reverted_events SELECT ?1, id, session_id, seq, kind, raw, created_at
                 FROM events WHERE session_id = ?2 AND seq > ?3",
                params![revert_id, session_id, event_seq],
            )?;
            tx.execute(
                "INSERT INTO reverted_messages SELECT ?1, id, session_id, event_id, seq, role, kind, content, created_at
                 FROM messages WHERE session_id = ?2 AND seq >= ?3",
                params![revert_id, session_id, message_seq],
            )?;
            tx.execute("DELETE FROM messages WHERE session_id = ?1 AND seq >= ?2", params![session_id, message_seq])?;
            tx.execute("DELETE FROM events WHERE session_id = ?1 AND seq > ?2", params![session_id, event_seq])?;
            tx.execute(
                "UPDATE checkpoints SET kind = 'reverted', turn = -turn
                 WHERE session_id = ?1 AND kind = 'turn' AND turn >= ?2",
                params![session_id, checkpoint.turn],
            )?;
            // The agent starts a new session, told what was said before this point and nothing of
            // what came after.
            tx.execute(
                "UPDATE sessions SET claude_session_id = NULL, fork_of = NULL, replay_context = ?1, updated_at = ?2
                 WHERE id = ?3",
                params![(!replay.is_empty()).then_some(replay), now, session_id],
            )?;
            tx.commit()
        })?;
        Ok(RevertOutcome { revert_id, files })
    }

    /// The revert of a chat that can still be undone, if there is one: nothing was said since, and
    /// it was not long ago.
    pub fn last_revert(&self, session_id: &str) -> AppResult<Option<UndoableRevert>> {
        let found: Option<(String, i64, i64, i64)> = self.db.read(|c| {
            c.query_row(
                "SELECT id, turn, message_seq, created_at FROM reverts
                     WHERE session_id = ?1 AND undone = 0 ORDER BY created_at DESC LIMIT 1",
                params![session_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()
        })?;
        let Some((id, turn, message_seq, created_at)) = found else {
            return Ok(None);
        };
        let newest: i64 = self.db.read(|c| {
            c.query_row(
                "SELECT COALESCE(MAX(seq), 0) FROM messages WHERE session_id = ?1",
                params![session_id],
                |r| r.get(0),
            )
        })?;
        let expires_at = created_at + UNDO_WINDOW_MS;
        if newest >= message_seq || now_ms() > expires_at {
            return Ok(None);
        }
        Ok(Some(UndoableRevert {
            id,
            turn,
            created_at,
            expires_at,
        }))
    }

    /// Takes a revert back: the files, the conversation and the agent's session.
    pub async fn undo(&self, revert_id: &str, cwd: &Path) -> AppResult<Vec<FileChange>> {
        let (session_id, pre_name, claude, replay, undone): (String, String, Option<String>, Option<String>, i64) = self
            .db
            .read(|c| {
                c.query_row(
                    "SELECT session_id, pre_revert_ref, claude_session_id, replay_context, undone FROM reverts WHERE id = ?1",
                    params![revert_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )
            })
            .map_err(|_| AppError::NotFound("That revert no longer exists.".into()))?;
        if undone != 0 {
            return Err(AppError::Invalid("That revert was already undone.".into()));
        }
        if self
            .last_revert(&session_id)?
            .is_none_or(|r| r.id != revert_id)
        {
            return Err(AppError::Invalid(
                "It is too late to undo that revert: the chat moved on, or too much time has passed.".into(),
            ));
        }
        if Self::is_active(&self.session_status(&session_id)?) {
            return Err(AppError::Invalid("A turn is running in this chat.".into()));
        }
        let commit = self.git.rev_parse(cwd, &pre_name).await?;
        let staging = cwd
            .join(".context")
            .join("checkpoint-staging")
            .join(format!("undo-{revert_id}"));
        let files = snapshot::restore(&self.git, cwd, &commit, &staging).await?;
        self.db.write(|c| {
            let tx = c.unchecked_transaction()?;
            tx.execute(
                "INSERT INTO events (id, session_id, seq, kind, raw, created_at)
                 SELECT id, session_id, seq, kind, raw, created_at FROM reverted_events WHERE revert_id = ?1",
                params![revert_id],
            )?;
            tx.execute(
                "INSERT INTO messages (id, session_id, event_id, seq, role, kind, content, created_at)
                 SELECT id, session_id, event_id, seq, role, kind, content, created_at FROM reverted_messages WHERE revert_id = ?1",
                params![revert_id],
            )?;
            tx.execute("DELETE FROM reverted_events WHERE revert_id = ?1", params![revert_id])?;
            tx.execute("DELETE FROM reverted_messages WHERE revert_id = ?1", params![revert_id])?;
            tx.execute(
                "UPDATE checkpoints SET kind = 'turn', turn = -turn WHERE session_id = ?1 AND kind = 'reverted'",
                params![session_id],
            )?;
            tx.execute(
                "UPDATE sessions SET claude_session_id = ?1, replay_context = ?2, updated_at = ?3 WHERE id = ?4",
                params![claude, replay, now_ms(), session_id],
            )?;
            tx.execute("UPDATE reverts SET undone = 1 WHERE id = ?1", params![revert_id])?;
            tx.commit()
        })?;
        Ok(files)
    }

    async fn drop_refs(&self, repo: &Path, refs: Vec<String>) {
        for name in refs {
            let _ = snapshot::delete_ref(&self.git, repo, &name).await;
        }
    }

    /// Removes every checkpoint of a workspace, for when it is archived. `repo` is the project's
    /// repository, which holds the refs.
    pub async fn prune_workspace(&self, workspace_id: &str, repo: &Path) -> AppResult<()> {
        let refs: Vec<String> = self.db.read(|c| {
            let mut stmt = c.prepare(
                "SELECT ref_name FROM checkpoints WHERE workspace_id = ?1
                 UNION SELECT pre_revert_ref FROM reverts WHERE workspace_id = ?1",
            )?;
            let rows = stmt.query_map(params![workspace_id], |r| r.get(0))?;
            rows.collect()
        })?;
        self.drop_refs(repo, refs).await;
        self.db.write(|c| {
            c.execute(
                "DELETE FROM reverts WHERE workspace_id = ?1",
                params![workspace_id],
            )?;
            c.execute(
                "DELETE FROM checkpoints WHERE workspace_id = ?1",
                params![workspace_id],
            )
        })?;
        Ok(())
    }

    /// Removes checkpoints and reverts older than `days`. Returns how many checkpoints went.
    pub async fn prune_older_than(&self, days: i64) -> AppResult<usize> {
        let cutoff = now_ms() - days * 24 * 60 * 60 * 1000;
        let old: Vec<(String, String, String)> = self.db.read(|c| {
            let mut stmt = c.prepare(
                "SELECT c.id, c.ref_name, p.path FROM checkpoints c
                   JOIN workspaces w ON w.id = c.workspace_id
                   JOIN projects p ON p.id = w.project_id
                 WHERE c.created_at < ?1",
            )?;
            let rows =
                stmt.query_map(params![cutoff], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            rows.collect()
        })?;
        for (_, name, repo) in &old {
            let _ = snapshot::delete_ref(&self.git, Path::new(repo), name).await;
        }
        let old_reverts: Vec<(String, String, String)> = self.db.read(|c| {
            let mut stmt = c.prepare(
                "SELECT r.id, r.pre_revert_ref, p.path FROM reverts r
                   JOIN workspaces w ON w.id = r.workspace_id
                   JOIN projects p ON p.id = w.project_id
                 WHERE r.created_at < ?1",
            )?;
            let rows = stmt.query_map(params![cutoff], |rw| {
                Ok((rw.get(0)?, rw.get(1)?, rw.get(2)?))
            })?;
            rows.collect()
        })?;
        for (_, name, repo) in &old_reverts {
            let _ = snapshot::delete_ref(&self.git, Path::new(repo), name).await;
        }
        self.db.write(|c| {
            c.execute("DELETE FROM reverts WHERE created_at < ?1", params![cutoff])?;
            c.execute(
                "DELETE FROM checkpoints WHERE created_at < ?1",
                params![cutoff],
            )
        })?;
        Ok(old.len())
    }
}

#[async_trait]
impl TurnHook for Checkpoints {
    async fn before_turn(
        &self,
        workspace_id: &str,
        session_id: &str,
        claude_session_id: Option<&str>,
        cwd: &Path,
    ) -> Option<String> {
        // A snapshot that cannot be taken never stops the message from being sent.
        match self
            .record_turn(workspace_id, session_id, claude_session_id, cwd)
            .await
        {
            Ok(id) => Some(id),
            Err(e) => {
                tracing::warn!(error = %e, "cannot take the checkpoint for this turn");
                None
            }
        }
    }

    fn after_message(&self, checkpoint_id: &str, message_seq: i64) {
        if let Err(e) = self.attach_message(checkpoint_id, message_seq) {
            tracing::warn!(error = %e, "cannot link the checkpoint to its message");
        }
    }
}

#[cfg(test)]
mod tests;
