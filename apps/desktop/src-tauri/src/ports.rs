//! Port blocks for workspaces. Each workspace gets ten consecutive ports, starting at
//! `CIRCLES_PORT`, so two dev servers in two workspaces never fight over a port.

use std::collections::HashSet;
use std::net::TcpListener;

use rusqlite::{params, OptionalExtension};

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::ipc::now_ms;

pub const FIRST_PORT: u16 = 55_000;
pub const BLOCK_SIZE: u16 = 10;
const LAST_PORT: u16 = 65_535;

/// Whether a port can be bound right now. A trait so tests can simulate busy ports.
pub trait PortProbe {
    fn is_free(&self, port: u16) -> bool;
}

pub struct SystemProbe;

impl PortProbe for SystemProbe {
    fn is_free(&self, port: u16) -> bool {
        // Both addresses, because a server on 0.0.0.0 blocks 127.0.0.1 and the reverse.
        TcpListener::bind(("127.0.0.1", port)).is_ok()
            && TcpListener::bind(("0.0.0.0", port)).is_ok()
    }
}

pub fn get(db: &Db, workspace_id: &str) -> AppResult<Option<u16>> {
    db.read(|c| {
        c.query_row(
            "SELECT base_port FROM ports WHERE workspace_id = ?1",
            params![workspace_id],
            |r| r.get::<_, u16>(0),
        )
        .optional()
    })
}

/// The workspace's port block. Allocates the lowest block whose ten ports are free and not held
/// by another workspace. Calling it again returns the same block.
pub fn allocate(db: &Db, workspace_id: &str, probe: &dyn PortProbe) -> AppResult<u16> {
    db.write(|c| {
        if let Some(existing) = c
            .query_row(
                "SELECT base_port FROM ports WHERE workspace_id = ?1",
                params![workspace_id],
                |r| r.get::<_, u16>(0),
            )
            .optional()?
        {
            return Ok(Some(existing));
        }

        let taken: HashSet<u16> = {
            let mut stmt = c.prepare("SELECT base_port FROM ports")?;
            let rows = stmt.query_map([], |r| r.get::<_, u16>(0))?;
            rows.collect::<Result<_, _>>()?
        };

        // u32 so the arithmetic cannot overflow near the top of the port range.
        let mut base = u32::from(FIRST_PORT);
        while base + u32::from(BLOCK_SIZE) - 1 <= u32::from(LAST_PORT) {
            let first = base as u16;
            if !taken.contains(&first) && (first..first + BLOCK_SIZE).all(|p| probe.is_free(p)) {
                c.execute(
                    "INSERT INTO ports (workspace_id, base_port, allocated_at) VALUES (?1, ?2, ?3)",
                    params![workspace_id, first, now_ms()],
                )?;
                return Ok(Some(first));
            }
            base += u32::from(BLOCK_SIZE);
        }
        Ok(None)
    })?
    .ok_or_else(|| AppError::Internal("no free port block is left".into()))
}

pub fn release(db: &Db, workspace_id: &str) -> AppResult<()> {
    db.write(|c| {
        c.execute(
            "DELETE FROM ports WHERE workspace_id = ?1",
            params![workspace_id],
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    struct Busy(HashSet<u16>);

    impl PortProbe for Busy {
        fn is_free(&self, port: u16) -> bool {
            !self.0.contains(&port)
        }
    }

    fn db_with_workspaces(ids: &[&str]) -> (tempfile::TempDir, Db) {
        let tmp = tempfile::tempdir().unwrap();
        let db = Db::open(&tmp.path().join("t.db")).unwrap();
        db.write(|c| {
            c.execute("INSERT INTO projects VALUES ('p','n','/p','main',NULL,1,1)", [])?;
            for id in ids {
                c.execute(
                    "INSERT INTO workspaces (id,project_id,city,branch,base_branch,path,state,created_at,updated_at)
                     VALUES (?1,'p',?1,'b','main',?1,'ready',1,1)",
                    params![id],
                )?;
            }
            Ok(())
        })
        .unwrap();
        (tmp, db)
    }

    #[test]
    fn blocks_are_ten_ports_wide_and_distinct() {
        let (_tmp, db) = db_with_workspaces(&["a", "b", "c"]);
        let free = Busy(HashSet::new());
        assert_eq!(allocate(&db, "a", &free).unwrap(), 55_000);
        assert_eq!(allocate(&db, "b", &free).unwrap(), 55_010);
        assert_eq!(allocate(&db, "c", &free).unwrap(), 55_020);
    }

    #[test]
    fn allocating_twice_returns_the_same_block() {
        let (_tmp, db) = db_with_workspaces(&["a"]);
        let free = Busy(HashSet::new());
        let first = allocate(&db, "a", &free).unwrap();
        assert_eq!(allocate(&db, "a", &free).unwrap(), first);
        assert_eq!(get(&db, "a").unwrap(), Some(first));
        assert_eq!(get(&db, "missing").unwrap(), None);
    }

    #[test]
    fn a_block_with_any_busy_port_is_skipped() {
        let (_tmp, db) = db_with_workspaces(&["a", "b"]);
        // Port 55003 is in use, so the whole first block is unusable.
        let busy = Busy([55_003].into());
        assert_eq!(allocate(&db, "a", &busy).unwrap(), 55_010);
        // The last port of a block counts too.
        let busy = Busy([55_003, 55_029].into());
        assert_eq!(allocate(&db, "b", &busy).unwrap(), 55_030);
    }

    #[test]
    fn released_blocks_are_reused() {
        let (_tmp, db) = db_with_workspaces(&["a", "b", "c"]);
        let free = Busy(HashSet::new());
        allocate(&db, "a", &free).unwrap();
        allocate(&db, "b", &free).unwrap();
        release(&db, "a").unwrap();
        assert_eq!(get(&db, "a").unwrap(), None);
        assert_eq!(allocate(&db, "c", &free).unwrap(), 55_000);
        release(&db, "missing").unwrap();
    }

    #[test]
    fn running_out_of_blocks_is_an_error() {
        let (_tmp, db) = db_with_workspaces(&["a"]);
        let all_busy = Busy((FIRST_PORT..=LAST_PORT).collect());
        let err = allocate(&db, "a", &all_busy).unwrap_err();
        assert_eq!(err.code(), "internal");
        assert!(err.to_string().contains("no free port block"));
    }

    #[test]
    fn the_last_block_fits_inside_the_port_range() {
        let (_tmp, db) = db_with_workspaces(&["a"]);
        // Everything below the last whole block is busy.
        let last_base = FIRST_PORT + ((LAST_PORT - FIRST_PORT + 1) / BLOCK_SIZE - 1) * BLOCK_SIZE;
        let busy = Busy((FIRST_PORT..last_base).collect());
        let base = allocate(&db, "a", &busy).unwrap();
        assert_eq!(base, last_base);
        assert!(u32::from(base) + u32::from(BLOCK_SIZE) - 1 <= u32::from(LAST_PORT));
    }

    #[test]
    fn deleting_a_workspace_frees_its_ports() {
        let (_tmp, db) = db_with_workspaces(&["a"]);
        allocate(&db, "a", &Busy(HashSet::new())).unwrap();
        db.write(|c| c.execute("DELETE FROM workspaces WHERE id = 'a'", []))
            .unwrap();
        assert_eq!(get(&db, "a").unwrap(), None);
    }

    #[test]
    fn the_system_probe_sees_a_listening_port() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(!SystemProbe.is_free(port));
        drop(listener);
        assert!(SystemProbe.is_free(port));
    }
}
