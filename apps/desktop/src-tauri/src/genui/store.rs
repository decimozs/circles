//! Cards in the database.

use std::sync::Arc;

use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use serde_json::Value;
use ts_rs::TS;

use super::Card;
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::ipc::{new_id, now_ms};

/// A card, as stored.
#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[ts(export)]
pub struct StoredCard {
    pub uid: String,
    pub session_id: String,
    pub composer_id: Option<String>,
    pub card: Card,
    /// `pending`, `answered`, `dismissed`, `expired` or `shown`.
    pub status: String,
    #[ts(type = "unknown")]
    pub answer: Option<Value>,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number | null")]
    pub answered_at: Option<i64>,
}

#[derive(Clone)]
pub struct CardStore {
    db: Arc<Db>,
}

const COLUMNS: &str = "uid, session_id, composer_id, card, status, answer, created_at, answered_at";

fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredCard> {
    let card: String = row.get(3)?;
    let answer: Option<String> = row.get(5)?;
    Ok(StoredCard {
        uid: row.get(0)?,
        session_id: row.get(1)?,
        composer_id: row.get(2)?,
        card: serde_json::from_str(&card).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
        })?,
        status: row.get(4)?,
        answer: answer.and_then(|a| serde_json::from_str(&a).ok()),
        created_at: row.get(6)?,
        answered_at: row.get(7)?,
    })
}

impl CardStore {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    pub fn create(
        &self,
        session_id: &str,
        composer_id: Option<&str>,
        card: &Card,
        status: &str,
    ) -> AppResult<StoredCard> {
        let uid = new_id();
        let text = serde_json::to_string(card).map_err(|e| AppError::Internal(e.to_string()))?;
        self.db.write(|c| {
            c.execute(
                "INSERT INTO ui_cards (uid, session_id, composer_id, card, status, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![uid, session_id, composer_id, text, status, now_ms()],
            )
        })?;
        self.get(&uid)
    }

    pub fn get(&self, uid: &str) -> AppResult<StoredCard> {
        self.db
            .read(|c| {
                c.query_row(
                    &format!("SELECT {COLUMNS} FROM ui_cards WHERE uid = ?1"),
                    params![uid],
                    from_row,
                )
                .optional()
            })?
            .ok_or_else(|| AppError::NotFound(format!("card {uid}")))
    }

    /// Moves a pending card to a final state, once. Returns whether this call did it, so two
    /// answers at the same time cannot both count.
    pub fn finish(&self, uid: &str, status: &str, answer: Option<&Value>) -> AppResult<bool> {
        let text = answer.map(Value::to_string);
        let rows = self.db.write(|c| {
            c.execute(
                "UPDATE ui_cards SET status = ?2, answer = ?3, answered_at = ?4
                 WHERE uid = ?1 AND status = 'pending'",
                params![uid, status, text, now_ms()],
            )
        })?;
        Ok(rows > 0)
    }

    pub fn pending_in(&self, session_id: &str) -> AppResult<usize> {
        self.db.read(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM ui_cards WHERE session_id = ?1 AND status = 'pending'",
                params![session_id],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n as usize)
        })
    }

    pub fn list(&self, session_id: &str, limit: usize) -> AppResult<Vec<StoredCard>> {
        self.db.read(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {COLUMNS} FROM (
                     SELECT * FROM ui_cards WHERE session_id = ?1 ORDER BY created_at DESC LIMIT ?2
                 ) ORDER BY created_at"
            ))?;
            let rows = stmt.query_map(params![session_id, limit as i64], from_row)?;
            rows.collect()
        })
    }

    /// Every card that still waits, for the "waiting for you" views.
    pub fn pending(&self) -> AppResult<Vec<StoredCard>> {
        self.db.read(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {COLUMNS} FROM ui_cards WHERE status = 'pending' ORDER BY created_at"
            ))?;
            let rows = stmt.query_map([], from_row)?;
            rows.collect()
        })
    }

    /// What waited for an answer before the app stopped is gone.
    pub fn expire_pending(&self) -> AppResult<usize> {
        self.db.write(|c| {
            c.execute(
                "UPDATE ui_cards SET status = 'expired', answered_at = ?1 WHERE status = 'pending'",
                params![now_ms()],
            )
        })
    }

    pub fn pending_count_for_composer(&self, composer_id: &str) -> AppResult<usize> {
        self.db.read(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM ui_cards WHERE composer_id = ?1 AND status = 'pending'",
                params![composer_id],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n as usize)
        })
    }
}
