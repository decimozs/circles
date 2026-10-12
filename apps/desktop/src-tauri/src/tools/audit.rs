//! Every tool call, kept so a person can see what a Composer did.

use std::sync::Arc;

use rusqlite::params;
use serde::Serialize;
use ts_rs::TS;

use crate::db::Db;
use crate::error::AppResult;
use crate::ipc::{new_id, now_ms};

#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[ts(export)]
pub struct AuditEntry {
    pub id: String,
    #[ts(type = "number")]
    pub at: i64,
    pub composer_id: Option<String>,
    pub session_id: Option<String>,
    pub tool: String,
    pub tier: Option<String>,
    pub input: String,
    /// `ok`, `error`, `denied` or `expired`.
    pub outcome: String,
    pub summary: String,
    pub approval_id: Option<String>,
}

/// What the dispatcher writes.
pub struct NewEntry<'a> {
    pub composer_id: Option<&'a str>,
    pub session_id: Option<&'a str>,
    pub tool: &'a str,
    pub tier: Option<&'a str>,
    pub input: &'a str,
    pub outcome: &'a str,
    pub summary: &'a str,
    pub approval_id: Option<&'a str>,
}

#[derive(Clone)]
pub struct AuditStore {
    db: Arc<Db>,
}

const COLUMNS: &str =
    "id, at, composer_id, session_id, tool, tier, input, outcome, summary, approval_id";

fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AuditEntry> {
    Ok(AuditEntry {
        id: row.get(0)?,
        at: row.get(1)?,
        composer_id: row.get(2)?,
        session_id: row.get(3)?,
        tool: row.get(4)?,
        tier: row.get(5)?,
        input: row.get(6)?,
        outcome: row.get(7)?,
        summary: row.get(8)?,
        approval_id: row.get(9)?,
    })
}

impl AuditStore {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    pub fn record(&self, entry: NewEntry<'_>) -> AppResult<String> {
        let id = new_id();
        let summary: String = entry.summary.chars().take(500).collect();
        self.db.write(|c| {
            c.execute(
                "INSERT INTO tool_audit
                   (id, at, composer_id, session_id, tool, tier, input, outcome, summary, approval_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    id,
                    now_ms(),
                    entry.composer_id,
                    entry.session_id,
                    entry.tool,
                    entry.tier,
                    entry.input,
                    entry.outcome,
                    summary,
                    entry.approval_id
                ],
            )
        })?;
        Ok(id)
    }

    /// Newest first. Every filter is optional. `search` looks in the tool, inputs and summary.
    pub fn list(
        &self,
        composer_id: Option<&str>,
        session_id: Option<&str>,
        search: Option<&str>,
        limit: u32,
    ) -> AppResult<Vec<AuditEntry>> {
        let like = search
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| format!("%{}%", s.replace('%', "\\%").replace('_', "\\_")));
        self.db.read(|c| {
            let sql = format!(
                "SELECT {COLUMNS} FROM tool_audit
                 WHERE (?1 IS NULL OR composer_id = ?1)
                   AND (?2 IS NULL OR session_id = ?2)
                   AND (?3 IS NULL OR tool LIKE ?3 ESCAPE '\\' OR input LIKE ?3 ESCAPE '\\'
                        OR summary LIKE ?3 ESCAPE '\\')
                 ORDER BY at DESC, rowid DESC LIMIT ?4"
            );
            let mut stmt = c.prepare(&sql)?;
            let rows = stmt.query_map(params![composer_id, session_id, like, limit], from_row)?;
            rows.collect()
        })
    }

    /// Calls a run has made that were not refused, for its step limit.
    pub fn steps_in(&self, session_id: &str) -> AppResult<u32> {
        self.db.read(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM tool_audit WHERE session_id = ?1 AND outcome IN ('ok', 'error')",
                params![session_id],
                |r| r.get(0),
            )
        })
    }

    pub fn export_csv(&self, composer_id: Option<&str>) -> AppResult<String> {
        let rows = self.list(composer_id, None, None, 100_000)?;
        let cell = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
        let mut out = String::from("time,composer,run,tool,tier,outcome,input,summary\n");
        for r in rows.iter().rev() {
            out.push_str(&format!(
                "{},{},{},{},{},{},{},{}\n",
                r.at,
                cell(r.composer_id.as_deref().unwrap_or("")),
                cell(r.session_id.as_deref().unwrap_or("")),
                cell(&r.tool),
                cell(r.tier.as_deref().unwrap_or("")),
                cell(&r.outcome),
                cell(&r.input),
                cell(&r.summary),
            ));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> AuditStore {
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::open(&dir.path().join("t.db")).unwrap());
        std::mem::forget(dir);
        AuditStore::new(db)
    }

    fn put(s: &AuditStore, session: &str, tool: &str, outcome: &str, input: &str) {
        s.record(NewEntry {
            composer_id: None,
            session_id: Some(session),
            tool,
            tier: Some("read"),
            input,
            outcome,
            summary: "s",
            approval_id: None,
        })
        .unwrap();
    }

    #[test]
    fn calls_are_listed_newest_first_and_can_be_searched() {
        let s = store();
        put(&s, "r1", "list_projects", "ok", "{}");
        put(
            &s,
            "r1",
            "create_workspace",
            "ok",
            "{\"project_id\":\"alpha\"}",
        );
        put(&s, "r2", "delete_workspace", "denied", "{}");
        let all = s.list(None, None, None, 10).unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].tool, "delete_workspace");
        assert_eq!(s.list(None, Some("r1"), None, 10).unwrap().len(), 2);
        assert_eq!(s.list(None, None, Some("alpha"), 10).unwrap().len(), 1);
        assert_eq!(s.list(None, None, Some("100%"), 10).unwrap().len(), 0);
    }

    #[test]
    fn only_calls_that_ran_count_as_steps() {
        let s = store();
        put(&s, "r1", "a", "ok", "{}");
        put(&s, "r1", "b", "error", "{}");
        put(&s, "r1", "c", "denied", "{}");
        put(&s, "r1", "d", "expired", "{}");
        assert_eq!(s.steps_in("r1").unwrap(), 2);
        assert_eq!(s.steps_in("other").unwrap(), 0);
    }

    #[test]
    fn the_export_quotes_what_needs_quoting() {
        let s = store();
        put(&s, "r1", "t", "ok", "{\"a\":\"say \\\"hi\\\"\"}");
        let csv = s.export_csv(None).unwrap();
        assert!(csv.starts_with("time,composer,run,tool"));
        assert!(csv.contains(r#"\""hi\"""#), "{csv}");
    }
}
