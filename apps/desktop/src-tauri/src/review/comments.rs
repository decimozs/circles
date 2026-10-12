//! Line comments on a workspace's files.

use std::sync::Arc;

use rusqlite::params;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::ipc::{new_id, now_ms};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CommentStatus {
    Open,
    Resolved,
}

impl CommentStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Resolved => "resolved",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CommentSource {
    User,
    Agent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct Comment {
    pub id: String,
    pub workspace_id: String,
    pub path: String,
    pub line_start: u32,
    pub line_end: u32,
    pub body: String,
    pub status: CommentStatus,
    pub source: CommentSource,
    /// When the comment was sent to a chat, if it was.
    #[ts(type = "number | null")]
    pub sent_at: Option<i64>,
    #[ts(type = "number")]
    pub created_at: i64,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct NewComment {
    pub path: String,
    pub line_start: u32,
    pub line_end: u32,
    pub body: String,
    pub source: Option<CommentSource>,
}

#[derive(Clone)]
pub struct CommentStore {
    db: Arc<Db>,
}

const COLUMNS: &str =
    "id, workspace_id, path, line_start, line_end, body, status, source, sent_at, created_at";

fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Comment> {
    let status: String = row.get(6)?;
    let source: String = row.get(7)?;
    Ok(Comment {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        path: row.get(2)?,
        line_start: row.get(3)?,
        line_end: row.get(4)?,
        body: row.get(5)?,
        status: if status == "resolved" {
            CommentStatus::Resolved
        } else {
            CommentStatus::Open
        },
        source: if source == "agent" {
            CommentSource::Agent
        } else {
            CommentSource::User
        },
        sent_at: row.get(8)?,
        created_at: row.get(9)?,
    })
}

impl CommentStore {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    pub fn list(&self, workspace_id: &str) -> AppResult<Vec<Comment>> {
        self.db.read(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {COLUMNS} FROM review_comments WHERE workspace_id = ?1
                 ORDER BY path, line_start, created_at"
            ))?;
            let rows = stmt.query_map(params![workspace_id], from_row)?;
            rows.collect()
        })
    }

    pub fn get(&self, id: &str) -> AppResult<Comment> {
        self.db
            .read(|c| {
                c.query_row(
                    &format!("SELECT {COLUMNS} FROM review_comments WHERE id = ?1"),
                    params![id],
                    from_row,
                )
            })
            .map_err(|e| match e {
                AppError::Db(rusqlite::Error::QueryReturnedNoRows) => {
                    AppError::NotFound("That comment no longer exists.".into())
                }
                other => other,
            })
    }

    pub fn create(&self, workspace_id: &str, new: NewComment) -> AppResult<Comment> {
        let body = new.body.trim();
        if body.is_empty() {
            return Err(AppError::Invalid("Write a comment first.".into()));
        }
        if new.line_start == 0 || new.line_end < new.line_start {
            return Err(AppError::Invalid("That line range is not valid.".into()));
        }
        let id = new_id();
        self.db.write(|c| {
            c.execute(
                "INSERT INTO review_comments
                   (id, workspace_id, path, line_start, line_end, body, source, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    id,
                    workspace_id,
                    new.path,
                    new.line_start,
                    new.line_end,
                    body,
                    if new.source == Some(CommentSource::Agent) {
                        "agent"
                    } else {
                        "user"
                    },
                    now_ms()
                ],
            )
        })?;
        self.get(&id)
    }

    pub fn set_status(&self, id: &str, status: CommentStatus) -> AppResult<Comment> {
        self.get(id)?;
        self.db.write(|c| {
            c.execute(
                "UPDATE review_comments SET status = ?1 WHERE id = ?2",
                params![status.as_str(), id],
            )
        })?;
        self.get(id)
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        self.db
            .write(|c| c.execute("DELETE FROM review_comments WHERE id = ?1", params![id]))?;
        Ok(())
    }

    pub fn mark_sent(&self, ids: &[String]) -> AppResult<()> {
        self.db.write(|c| {
            for id in ids {
                c.execute(
                    "UPDATE review_comments SET sent_at = ?1 WHERE id = ?2",
                    params![now_ms(), id],
                )?;
            }
            Ok(())
        })
    }
}

/// The message that hands comments to the agent: where each one is and what it says.
pub fn compose_message(comments: &[Comment]) -> String {
    let mut out =
        String::from("Please address these review comments. Each one names a file and lines.\n");
    for comment in comments {
        let lines = if comment.line_start == comment.line_end {
            format!("line {}", comment.line_start)
        } else {
            format!("lines {}-{}", comment.line_start, comment.line_end)
        };
        out.push_str(&format!(
            "\n- {}, {}: {}",
            comment.path,
            lines,
            comment.body.trim()
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, CommentStore) {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::open(&tmp.path().join("t.db")).unwrap());
        db.write(|c| {
            c.execute_batch(
                "INSERT INTO projects VALUES ('p','n','/p','main',NULL,1,1);
                 INSERT INTO workspaces (id,project_id,city,branch,base_branch,path,state,created_at,updated_at)
                   VALUES ('w1','p','Oslo','b','main','/w1','ready',1,1),
                          ('w2','p','Rome','b2','main','/w2','ready',1,1);",
            )
        })
        .unwrap();
        (tmp, CommentStore::new(db))
    }

    fn new(path: &str, start: u32, end: u32, body: &str) -> NewComment {
        NewComment {
            path: path.into(),
            line_start: start,
            line_end: end,
            body: body.into(),
            source: None,
        }
    }

    #[test]
    fn comments_are_stored_per_workspace_in_file_order() {
        let (_tmp, store) = store();
        store
            .create("w1", new("src/b.rs", 9, 9, "second file"))
            .unwrap();
        store
            .create("w1", new("src/a.rs", 20, 22, "later line"))
            .unwrap();
        store
            .create("w1", new("src/a.rs", 3, 3, "  early line  "))
            .unwrap();
        store
            .create("w2", new("src/a.rs", 1, 1, "other workspace"))
            .unwrap();

        let list = store.list("w1").unwrap();
        let order: Vec<_> = list
            .iter()
            .map(|c| (c.path.as_str(), c.line_start))
            .collect();
        assert_eq!(order, [("src/a.rs", 3), ("src/a.rs", 20), ("src/b.rs", 9)]);
        assert_eq!(list[0].body, "early line");
        assert_eq!(list[0].status, CommentStatus::Open);
        assert_eq!(list[0].source, CommentSource::User);
        assert_eq!(store.list("w2").unwrap().len(), 1);
    }

    #[test]
    fn bad_comments_are_refused() {
        let (_tmp, store) = store();
        assert_eq!(
            store.create("w1", new("a", 1, 1, "  ")).unwrap_err().code(),
            "invalid"
        );
        assert_eq!(
            store.create("w1", new("a", 0, 1, "x")).unwrap_err().code(),
            "invalid"
        );
        assert_eq!(
            store.create("w1", new("a", 5, 4, "x")).unwrap_err().code(),
            "invalid"
        );
        assert_eq!(store.get("nope").unwrap_err().code(), "not_found");
    }

    #[test]
    fn comments_can_be_resolved_reopened_and_deleted() {
        let (_tmp, store) = store();
        let comment = store.create("w1", new("a.rs", 1, 2, "fix")).unwrap();
        assert_eq!(
            store
                .set_status(&comment.id, CommentStatus::Resolved)
                .unwrap()
                .status,
            CommentStatus::Resolved
        );
        assert_eq!(
            store
                .set_status(&comment.id, CommentStatus::Open)
                .unwrap()
                .status,
            CommentStatus::Open
        );
        store.delete(&comment.id).unwrap();
        assert!(store.list("w1").unwrap().is_empty());
    }

    #[test]
    fn sending_marks_comments_and_agent_comments_keep_their_source() {
        let (_tmp, store) = store();
        let mut agent = new("a.rs", 4, 4, "from review");
        agent.source = Some(CommentSource::Agent);
        let a = store.create("w1", agent).unwrap();
        let b = store.create("w1", new("a.rs", 8, 8, "mine")).unwrap();
        assert_eq!(a.source, CommentSource::Agent);
        store.mark_sent(std::slice::from_ref(&a.id)).unwrap();
        assert!(store.get(&a.id).unwrap().sent_at.is_some());
        assert!(store.get(&b.id).unwrap().sent_at.is_none());
    }

    #[test]
    fn deleting_a_workspace_removes_its_comments() {
        let (_tmp, store) = store();
        store.create("w1", new("a.rs", 1, 1, "x")).unwrap();
        store
            .db
            .write(|c| c.execute("DELETE FROM workspaces WHERE id = 'w1'", []))
            .unwrap();
        assert!(store.list("w1").unwrap().is_empty());
    }

    #[test]
    fn the_message_names_each_file_and_line() {
        let (_tmp, store) = store();
        let one = store
            .create("w1", new("src/a.rs", 7, 7, "rename this"))
            .unwrap();
        let range = store
            .create("w1", new("src/b.rs", 10, 14, "extract a function"))
            .unwrap();
        let text = compose_message(&[one, range]);
        assert!(text.contains("- src/a.rs, line 7: rename this"));
        assert!(text.contains("- src/b.rs, lines 10-14: extract a function"));
    }
}
