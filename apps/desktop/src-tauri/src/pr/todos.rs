//! A workspace's todos. Completed means done, and a todo that no longer matters is deleted.

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
pub enum TodoSource {
    User,
    Agent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct Todo {
    pub id: String,
    pub workspace_id: String,
    pub text: String,
    pub done: bool,
    pub source: TodoSource,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number | null")]
    pub completed_at: Option<i64>,
}

#[derive(Clone)]
pub struct TodoStore {
    db: Arc<Db>,
}

const COLUMNS: &str = "id, workspace_id, text, done, source, created_at, completed_at";

fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Todo> {
    let source: String = row.get(4)?;
    Ok(Todo {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        text: row.get(2)?,
        done: row.get::<_, i64>(3)? != 0,
        source: if source == "agent" {
            TodoSource::Agent
        } else {
            TodoSource::User
        },
        created_at: row.get(5)?,
        completed_at: row.get(6)?,
    })
}

fn clean(text: &str) -> AppResult<String> {
    let text = text.trim();
    if text.is_empty() {
        return Err(AppError::Invalid("Write what needs to be done.".into()));
    }
    if text.chars().count() > 500 {
        return Err(AppError::Invalid(
            "A todo can have at most 500 characters.".into(),
        ));
    }
    Ok(text.to_string())
}

impl TodoStore {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    /// Open todos first, each group oldest first.
    pub fn list(&self, workspace_id: &str) -> AppResult<Vec<Todo>> {
        self.db.read(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {COLUMNS} FROM todos WHERE workspace_id = ?1 ORDER BY done, created_at, rowid"
            ))?;
            let rows = stmt.query_map(params![workspace_id], from_row)?;
            rows.collect()
        })
    }

    pub fn open_count(&self, workspace_id: &str) -> AppResult<u32> {
        self.db.read(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM todos WHERE workspace_id = ?1 AND done = 0",
                params![workspace_id],
                |r| r.get(0),
            )
        })
    }

    pub fn get(&self, id: &str) -> AppResult<Todo> {
        self.db
            .read(|c| {
                c.query_row(
                    &format!("SELECT {COLUMNS} FROM todos WHERE id = ?1"),
                    params![id],
                    from_row,
                )
            })
            .map_err(|e| match e {
                AppError::Db(rusqlite::Error::QueryReturnedNoRows) => {
                    AppError::NotFound("That todo no longer exists.".into())
                }
                other => other,
            })
    }

    pub fn create(&self, workspace_id: &str, text: &str, source: TodoSource) -> AppResult<Todo> {
        let text = clean(text)?;
        let id = new_id();
        self.db.write(|c| {
            c.execute(
                "INSERT INTO todos (id, workspace_id, text, source, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    id,
                    workspace_id,
                    text,
                    if source == TodoSource::Agent {
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

    pub fn set_text(&self, id: &str, text: &str) -> AppResult<Todo> {
        let text = clean(text)?;
        self.get(id)?;
        self.db.write(|c| {
            c.execute(
                "UPDATE todos SET text = ?1 WHERE id = ?2",
                params![text, id],
            )
        })?;
        self.get(id)
    }

    pub fn set_done(&self, id: &str, done: bool) -> AppResult<Todo> {
        self.get(id)?;
        self.db.write(|c| {
            c.execute(
                "UPDATE todos SET done = ?1, completed_at = ?2 WHERE id = ?3",
                params![done as i64, if done { Some(now_ms()) } else { None }, id],
            )
        })?;
        self.get(id)
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        self.db
            .write(|c| c.execute("DELETE FROM todos WHERE id = ?1", params![id]))?;
        Ok(())
    }
}

/// The todos in a `circles-todos` block of an agent's reply: a JSON array of strings.
pub fn parse_agent_todos(reply: &str) -> Vec<String> {
    let mut found = Vec::new();
    for part in reply.split("```circles-todos").skip(1) {
        let Some(body) = part.split("```").next() else {
            continue;
        };
        if let Ok(items) = serde_json::from_str::<Vec<serde_json::Value>>(body.trim()) {
            found.extend(
                items
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty()),
            );
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, TodoStore) {
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
        (tmp, TodoStore::new(db))
    }

    #[test]
    fn todos_are_added_listed_and_counted_per_workspace() {
        let (_tmp, store) = store();
        let a = store
            .create("w1", "  Write tests ", TodoSource::User)
            .unwrap();
        store
            .create("w1", "Update docs", TodoSource::Agent)
            .unwrap();
        store.create("w2", "Elsewhere", TodoSource::User).unwrap();
        assert_eq!(a.text, "Write tests");
        let list = store.list("w1").unwrap();
        assert_eq!(
            list.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(),
            ["Write tests", "Update docs"]
        );
        assert_eq!(list[1].source, TodoSource::Agent);
        assert_eq!(store.open_count("w1").unwrap(), 2);
        assert_eq!(store.open_count("w2").unwrap(), 1);
    }

    #[test]
    fn completing_moves_a_todo_after_the_open_ones_and_stops_it_counting() {
        let (_tmp, store) = store();
        let first = store.create("w1", "first", TodoSource::User).unwrap();
        store.create("w1", "second", TodoSource::User).unwrap();
        let done = store.set_done(&first.id, true).unwrap();
        assert!(done.done && done.completed_at.is_some());
        assert_eq!(
            store
                .list("w1")
                .unwrap()
                .iter()
                .map(|t| t.text.as_str())
                .collect::<Vec<_>>(),
            ["second", "first"]
        );
        assert_eq!(store.open_count("w1").unwrap(), 1);
        let reopened = store.set_done(&first.id, false).unwrap();
        assert!(!reopened.done && reopened.completed_at.is_none());
        assert_eq!(store.open_count("w1").unwrap(), 2);
    }

    #[test]
    fn todos_can_be_edited_and_deleted() {
        let (_tmp, store) = store();
        let todo = store.create("w1", "old", TodoSource::User).unwrap();
        assert_eq!(store.set_text(&todo.id, "new").unwrap().text, "new");
        store.delete(&todo.id).unwrap();
        assert!(store.list("w1").unwrap().is_empty());
        assert_eq!(store.get(&todo.id).unwrap_err().code(), "not_found");
    }

    #[test]
    fn empty_and_huge_todos_are_refused() {
        let (_tmp, store) = store();
        assert_eq!(
            store
                .create("w1", "   ", TodoSource::User)
                .unwrap_err()
                .code(),
            "invalid"
        );
        assert_eq!(
            store
                .create("w1", &"x".repeat(501), TodoSource::User)
                .unwrap_err()
                .code(),
            "invalid"
        );
        let todo = store.create("w1", "ok", TodoSource::User).unwrap();
        assert_eq!(store.set_text(&todo.id, "").unwrap_err().code(), "invalid");
    }

    #[test]
    fn deleting_a_workspace_removes_its_todos() {
        let (_tmp, store) = store();
        store.create("w1", "x", TodoSource::User).unwrap();
        store
            .db
            .write(|c| c.execute("DELETE FROM workspaces WHERE id = 'w1'", []))
            .unwrap();
        assert!(store.list("w1").unwrap().is_empty());
    }

    #[test]
    fn todos_an_agent_suggests_are_read_from_its_reply() {
        let reply = "Done.\n```circles-todos\n[\"Add a migration\", \"  \", 5, \"Update the README\"]\n```\nand\n```circles-todos\nnot json\n```";
        assert_eq!(
            parse_agent_todos(reply),
            ["Add a migration", "Update the README"]
        );
        assert!(parse_agent_todos("nothing").is_empty());
    }
}
