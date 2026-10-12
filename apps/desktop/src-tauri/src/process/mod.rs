//! Registry of child processes we started. After a crash, `reap_orphans` kills any that are
//! still alive so no agent, dev server or shell outlives the app.

use std::time::Duration;

use rusqlite::params;
use serde::Serialize;
use ts_rs::TS;

use crate::db::Db;
use crate::error::AppResult;
use crate::ipc::now_ms;

#[derive(Debug, Clone)]
pub struct ProcessRecord {
    pub pid: i32,
    pub kind: String,
    pub owner_id: Option<String>,
    /// Executable name or a distinctive part of its command line. Used to avoid killing an
    /// unrelated process that reused the pid.
    pub command: String,
    pub started_at: i64,
}

impl ProcessRecord {
    pub fn new(pid: i32, kind: &str, owner_id: Option<&str>, command: &str) -> Self {
        Self {
            pid,
            kind: kind.to_string(),
            owner_id: owner_id.map(str::to_string),
            command: command.to_string(),
            started_at: now_ms(),
        }
    }
}

pub fn register(db: &Db, rec: &ProcessRecord) -> AppResult<()> {
    db.write(|c| {
        c.execute(
            "INSERT OR REPLACE INTO processes (pid, kind, owner_id, command, started_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![rec.pid, rec.kind, rec.owner_id, rec.command, rec.started_at],
        )
    })?;
    Ok(())
}

pub fn unregister(db: &Db, pid: i32) -> AppResult<()> {
    db.write(|c| c.execute("DELETE FROM processes WHERE pid = ?1", params![pid]))?;
    Ok(())
}

pub fn list(db: &Db) -> AppResult<Vec<ProcessRecord>> {
    db.read(|c| {
        let mut stmt = c.prepare(
            "SELECT pid, kind, owner_id, command, started_at FROM processes ORDER BY started_at",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(ProcessRecord {
                pid: r.get(0)?,
                kind: r.get(1)?,
                owner_id: r.get(2)?,
                command: r.get(3)?,
                started_at: r.get(4)?,
            })
        })?;
        rows.collect()
    })
}

/// System calls the reaper needs. A trait so tests never touch real processes.
pub trait ProcessOps {
    fn is_alive(&self, pid: i32) -> bool;
    fn command_line(&self, pid: i32) -> Option<String>;
    /// Terminates the whole process group led by `pid`, escalating to SIGKILL.
    fn kill_group(&self, pid: i32);
}

pub struct SystemOps;

impl ProcessOps for SystemOps {
    fn is_alive(&self, pid: i32) -> bool {
        // Signal 0 only checks existence. EPERM still means the process exists.
        let rc = unsafe { libc::kill(pid, 0) };
        rc == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }

    fn command_line(&self, pid: i32) -> Option<String> {
        let out = std::process::Command::new("ps")
            .args(["-o", "command=", "-p", &pid.to_string()])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (!text.is_empty()).then_some(text)
    }

    fn kill_group(&self, pid: i32) {
        unsafe {
            libc::killpg(pid, libc::SIGTERM);
        }
        std::thread::sleep(Duration::from_millis(500));
        if self.is_alive(pid) {
            unsafe {
                libc::killpg(pid, libc::SIGKILL);
                libc::kill(pid, libc::SIGKILL);
            }
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ReapReport {
    /// Orphans that were still running and got terminated.
    pub killed: u32,
    /// Records whose process was already gone, or whose pid was reused by another program.
    pub stale: u32,
}

pub fn reap_orphans(db: &Db, ops: &dyn ProcessOps) -> AppResult<ReapReport> {
    let mut report = ReapReport::default();
    for rec in list(db)? {
        let ours = ops.is_alive(rec.pid)
            && ops
                .command_line(rec.pid)
                .is_some_and(|line| line.contains(&rec.command));
        if ours {
            tracing::warn!(pid = rec.pid, kind = %rec.kind, "terminating orphaned process");
            ops.kill_group(rec.pid);
            report.killed += 1;
        } else {
            report.stale += 1;
        }
        unregister(db, rec.pid)?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    struct FakeOps {
        alive: HashMap<i32, String>,
        killed: RefCell<Vec<i32>>,
    }

    impl ProcessOps for FakeOps {
        fn is_alive(&self, pid: i32) -> bool {
            self.alive.contains_key(&pid)
        }
        fn command_line(&self, pid: i32) -> Option<String> {
            self.alive.get(&pid).cloned()
        }
        fn kill_group(&self, pid: i32) {
            self.killed.borrow_mut().push(pid);
        }
    }

    fn temp_db() -> (tempfile::TempDir, Db) {
        let tmp = tempfile::tempdir().unwrap();
        let db = Db::open(&tmp.path().join("t.db")).unwrap();
        (tmp, db)
    }

    #[test]
    fn register_list_unregister() {
        let (_tmp, db) = temp_db();
        register(&db, &ProcessRecord::new(10, "pty", Some("w1"), "zsh")).unwrap();
        register(&db, &ProcessRecord::new(11, "agent", None, "claude")).unwrap();
        assert_eq!(list(&db).unwrap().len(), 2);
        unregister(&db, 10).unwrap();
        let left = list(&db).unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].pid, 11);
    }

    #[test]
    fn reaps_live_orphans_and_ignores_reused_pids() {
        let (_tmp, db) = temp_db();
        register(&db, &ProcessRecord::new(100, "agent", Some("w"), "claude")).unwrap();
        register(&db, &ProcessRecord::new(200, "pty", Some("w"), "zsh")).unwrap();
        register(&db, &ProcessRecord::new(300, "run", Some("w"), "pnpm")).unwrap();

        let ops = FakeOps {
            alive: HashMap::from([
                (100, "claude -p --verbose".to_string()),
                // pid 200 was reused by an unrelated program
                (200, "/Applications/Safari".to_string()),
            ]),
            killed: RefCell::new(vec![]),
        };
        let report = reap_orphans(&db, &ops).unwrap();

        assert_eq!(
            report,
            ReapReport {
                killed: 1,
                stale: 2
            }
        );
        assert_eq!(*ops.killed.borrow(), vec![100]);
        assert!(list(&db).unwrap().is_empty());
    }

    #[test]
    fn system_ops_detects_this_process() {
        let me = std::process::id() as i32;
        assert!(SystemOps.is_alive(me));
        assert!(!SystemOps.is_alive(i32::MAX));
    }
}
