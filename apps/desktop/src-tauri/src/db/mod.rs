//! SQLite access: one writer connection, a small pool of read-only connections, WAL mode,
//! and versioned migrations tracked with `PRAGMA user_version`.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use rusqlite::{Connection, OpenFlags};

use crate::error::{AppError, AppResult};

const READ_POOL_SIZE: usize = 4;

/// Append only. Never edit a migration that has shipped.
const MIGRATIONS: &[(&str, &str)] = &[
    ("0001_init", include_str!("migrations/0001_init.sql")),
    (
        "0002_workspaces",
        include_str!("migrations/0002_workspaces.sql"),
    ),
    (
        "0003_first_prompt",
        include_str!("migrations/0003_first_prompt.sql"),
    ),
    (
        "0004_terminals",
        include_str!("migrations/0004_terminals.sql"),
    ),
    (
        "0005_sessions",
        include_str!("migrations/0005_sessions.sql"),
    ),
    (
        "0006_review_comments",
        include_str!("migrations/0006_review_comments.sql"),
    ),
    ("0007_todos", include_str!("migrations/0007_todos.sql")),
    (
        "0008_checkpoints",
        include_str!("migrations/0008_checkpoints.sql"),
    ),
    (
        "0009_composers",
        include_str!("migrations/0009_composers.sql"),
    ),
    (
        "0010_composer_mode",
        include_str!("migrations/0010_composer_mode.sql"),
    ),
    (
        "0011_composer_integrations",
        include_str!("migrations/0011_composer_integrations.sql"),
    ),
    (
        "0012_title_source",
        include_str!("migrations/0012_title_source.sql"),
    ),
    (
        "0013_ui_cards",
        include_str!("migrations/0013_ui_cards.sql"),
    ),
    (
        "0014_drop_todos",
        include_str!("migrations/0014_drop_todos.sql"),
    ),
];

pub struct Db {
    writer: Mutex<Connection>,
    readers: Vec<Mutex<Connection>>,
    next_reader: AtomicUsize,
}

fn configure(conn: &Connection) -> rusqlite::Result<()> {
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

impl Db {
    /// Opens the database, enabling WAL, and applies pending migrations.
    pub fn open(path: &Path) -> AppResult<Self> {
        let writer = Connection::open(path)?;
        writer.pragma_update(None, "journal_mode", "WAL")?;
        writer.pragma_update(None, "synchronous", "NORMAL")?;
        configure(&writer)?;
        migrate(&writer)?;

        let readers = (0..READ_POOL_SIZE)
            .map(|_| {
                let conn = Connection::open_with_flags(
                    path,
                    OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
                )?;
                configure(&conn)?;
                Ok(Mutex::new(conn))
            })
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(Self {
            writer: Mutex::new(writer),
            readers,
            next_reader: AtomicUsize::new(0),
        })
    }

    pub fn write<T>(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> AppResult<T> {
        let conn = self
            .writer
            .lock()
            .map_err(|_| AppError::Internal("database writer lock poisoned".into()))?;
        Ok(f(&conn)?)
    }

    pub fn read<T>(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> AppResult<T> {
        let idx = self.next_reader.fetch_add(1, Ordering::Relaxed) % self.readers.len();
        let conn = self.readers[idx]
            .lock()
            .map_err(|_| AppError::Internal("database reader lock poisoned".into()))?;
        Ok(f(&conn)?)
    }

    pub fn schema_version(&self) -> AppResult<u32> {
        self.read(|c| c.pragma_query_value(None, "user_version", |r| r.get(0)))
    }

    pub fn journal_mode(&self) -> AppResult<String> {
        self.read(|c| c.pragma_query_value(None, "journal_mode", |r| r.get(0)))
    }
}

fn migrate(conn: &Connection) -> AppResult<()> {
    let current = conn.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))? as usize;
    if current > MIGRATIONS.len() {
        return Err(AppError::Internal(format!(
            "database schema version {current} is newer than this app supports ({})",
            MIGRATIONS.len()
        )));
    }
    for (index, (name, sql)) in MIGRATIONS.iter().enumerate().skip(current) {
        tracing::info!(migration = name, "applying migration");
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (index + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_temp() -> (tempfile::TempDir, Db) {
        let tmp = tempfile::tempdir().unwrap();
        let db = Db::open(&tmp.path().join("t.db")).unwrap();
        (tmp, db)
    }

    #[test]
    fn creates_schema_and_enables_wal() {
        let (_tmp, db) = open_temp();
        assert_eq!(db.schema_version().unwrap() as usize, MIGRATIONS.len());
        assert_eq!(db.journal_mode().unwrap(), "wal");
        let tables: Vec<String> = db
            .read(|c| {
                let mut stmt = c.prepare(
                    "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
                )?;
                let rows = stmt.query_map([], |r| r.get(0))?;
                rows.collect()
            })
            .unwrap();
        for expected in [
            "checkpoints",
            "events",
            "kv",
            "messages",
            "ports",
            "processes",
            "projects",
            "sessions",
            "workspaces",
        ] {
            assert!(
                tables.contains(&expected.to_string()),
                "missing table {expected}"
            );
        }
    }

    #[test]
    fn reopening_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("t.db");
        let first = Db::open(&path).unwrap();
        first
            .write(|c| {
                c.execute(
                    "INSERT INTO kv (key, value, updated_at) VALUES ('a', 'b', 1)",
                    [],
                )
            })
            .unwrap();
        drop(first);
        let second = Db::open(&path).unwrap();
        let value: String = second
            .read(|c| c.query_row("SELECT value FROM kv WHERE key = 'a'", [], |r| r.get(0)))
            .unwrap();
        assert_eq!(value, "b");
        assert_eq!(second.schema_version().unwrap() as usize, MIGRATIONS.len());
    }

    #[test]
    fn rejects_a_newer_schema() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("t.db");
        let conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "user_version", (MIGRATIONS.len() + 1) as i64)
            .unwrap();
        drop(conn);
        assert!(Db::open(&path).is_err());
    }

    #[test]
    fn foreign_keys_cascade_from_workspace() {
        let (_tmp, db) = open_temp();
        db.write(|c| {
            c.execute_batch(
                "INSERT INTO projects VALUES ('p','n','/p','main',NULL,1,1);
                 INSERT INTO workspaces (id,project_id,city,branch,base_branch,path,state,created_at,updated_at)
                   VALUES ('w','p','Oslo','b','main','/w','ready',1,1);
                 INSERT INTO ports (workspace_id,base_port,allocated_at) VALUES ('w',3000,1);
                 DELETE FROM workspaces WHERE id='w';",
            )
        })
        .unwrap();
        let ports: i64 = db
            .read(|c| c.query_row("SELECT COUNT(*) FROM ports", [], |r| r.get(0)))
            .unwrap();
        assert_eq!(ports, 0);
    }

    #[test]
    fn readers_cannot_write() {
        let (_tmp, db) = open_temp();
        let result = db.read(|c| c.execute("INSERT INTO kv VALUES ('x','y',1)", []));
        assert!(result.is_err());
    }
}
