//! What was read from the transcripts, kept in the app's database, and how it is brought up to
//! date. The Usage page reads this, so it opens at once; a refresh adds only what is new.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use rusqlite::params;

use super::scan::{read_from, transcripts, Record};
use crate::db::Db;
use crate::error::AppResult;

const REFRESHED_AT: &str = "usage.refreshed_at";

#[derive(Clone)]
pub struct UsageStore {
    db: Arc<Db>,
}

/// What a refresh did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RefreshStats {
    pub files: u32,
    /// Files that had something new to read.
    pub read: u32,
    pub messages: u32,
}

impl UsageStore {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    /// Answers at or after `since` (unix seconds), for one Claude folder or all of them.
    pub fn records(&self, since: Option<i64>, folder: Option<&str>) -> AppResult<Vec<Record>> {
        self.db.read(|c| {
            let mut stmt = c.prepare(
                "SELECT id, model, at, day, session, input, output, cache_read, cache_write_5m,
                        cache_write_1h
                 FROM usage_messages
                 WHERE at >= ?1 AND (?2 IS NULL OR folder = ?2)",
            )?;
            let rows = stmt.query_map(params![since.unwrap_or(0), folder], |r| {
                let count = |i: usize| r.get::<_, i64>(i).map(|v| v.max(0) as u64);
                Ok(Record {
                    id: r.get(0)?,
                    model: r.get(1)?,
                    at: r.get(2)?,
                    day: r.get(3)?,
                    session: r.get(4)?,
                    input: count(5)?,
                    output: count(6)?,
                    cache_read: count(7)?,
                    cache_write_5m: count(8)?,
                    cache_write_1h: count(9)?,
                })
            })?;
            rows.collect()
        })
    }

    /// The Claude folders that have history here, including ones that no longer exist.
    pub fn folders(&self) -> AppResult<Vec<String>> {
        self.db.read(|c| {
            let mut stmt =
                c.prepare("SELECT DISTINCT folder FROM usage_messages ORDER BY folder")?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            rows.collect()
        })
    }

    /// Lines that looked like usage and could not be read, over all files.
    pub fn skipped(&self) -> AppResult<u32> {
        self.db.read(|c| {
            c.query_row(
                "SELECT COALESCE(SUM(skipped), 0) FROM usage_files",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n.max(0) as u32)
        })
    }

    /// When the last refresh finished, in milliseconds.
    pub fn refreshed_at(&self) -> AppResult<Option<i64>> {
        self.db.read(|c| {
            let value: rusqlite::Result<String> =
                c.query_row("SELECT value FROM kv WHERE key = ?1", [REFRESHED_AT], |r| {
                    r.get(0)
                });
            Ok(value.ok().and_then(|v| v.parse().ok()))
        })
    }

    fn mark_refreshed(&self, at_ms: i64) -> AppResult<()> {
        self.db.write(|c| {
            c.execute(
                "INSERT INTO kv (key, value, updated_at) VALUES (?1, ?2, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                params![REFRESHED_AT, at_ms.to_string()],
            )
            .map(|_| ())
        })
    }

    fn file_state(&self, path: &str) -> AppResult<Option<(u64, i64, u64)>> {
        self.db.read(|c| {
            let row: rusqlite::Result<(i64, i64, i64)> = c.query_row(
                "SELECT len, mtime_ms, offset FROM usage_files WHERE path = ?1",
                [path],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            );
            Ok(row
                .ok()
                .map(|(l, m, o)| (l.max(0) as u64, m, o.max(0) as u64)))
        })
    }

    /// Forgets what a file gave, for when it was replaced and must be read again.
    fn forget_file(&self, path: &str) -> AppResult<()> {
        self.db.write(|c| {
            c.execute("DELETE FROM usage_messages WHERE file = ?1", [path])?;
            c.execute("DELETE FROM usage_files WHERE path = ?1", [path])?;
            Ok(())
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn save(
        &self,
        folder: &str,
        path: &str,
        len: u64,
        mtime_ms: i64,
        offset: u64,
        skipped: u32,
        records: &[Record],
    ) -> AppResult<()> {
        self.db.write(|c| {
            let tx = c.unchecked_transaction()?;
            {
                let mut insert = tx.prepare(
                    "INSERT OR REPLACE INTO usage_messages
                     (id, folder, file, model, at, day, session, input, output, cache_read,
                      cache_write_5m, cache_write_1h)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                )?;
                for r in records {
                    insert.execute(params![
                        r.id,
                        folder,
                        path,
                        r.model,
                        r.at,
                        r.day,
                        r.session,
                        r.input as i64,
                        r.output as i64,
                        r.cache_read as i64,
                        r.cache_write_5m as i64,
                        r.cache_write_1h as i64,
                    ])?;
                }
            }
            tx.execute(
                "INSERT INTO usage_files (path, len, mtime_ms, offset, skipped)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(path) DO UPDATE SET len = excluded.len, mtime_ms = excluded.mtime_ms,
                    offset = excluded.offset, skipped = usage_files.skipped + excluded.skipped",
                params![path, len as i64, mtime_ms, offset as i64, skipped as i64],
            )?;
            tx.commit()
        })
    }

    /// Deletes everything kept, so the next refresh reads the transcripts again. History of
    /// transcripts Claude Code has deleted is lost for good.
    pub fn clear(&self) -> AppResult<()> {
        self.db.write(|c| {
            c.execute("DELETE FROM usage_messages", [])?;
            c.execute("DELETE FROM usage_files", [])?;
            c.execute("DELETE FROM kv WHERE key = ?1", [REFRESHED_AT])?;
            Ok(())
        })
    }

    /// Brings what is kept up to date with the transcripts in `dirs`. A file that did not change
    /// is skipped, one that grew is read from where it stopped, and one that got shorter was
    /// replaced and is read again whole. A file that is gone keeps what it gave. `progress` is
    /// called after every file with how many are done of how many.
    pub fn refresh(
        &self,
        dirs: &[PathBuf],
        progress: &mut dyn FnMut(u32, u32),
    ) -> AppResult<RefreshStats> {
        let mut jobs: Vec<(String, PathBuf)> = Vec::new();
        let mut seen: HashSet<PathBuf> = HashSet::new();
        for dir in dirs {
            let folder = dir.to_string_lossy().into_owned();
            for path in transcripts(dir) {
                if seen.insert(path.clone()) {
                    jobs.push((folder.clone(), path));
                }
            }
        }
        let total = jobs.len() as u32;
        let mut stats = RefreshStats {
            files: total,
            ..RefreshStats::default()
        };
        for (done, (folder, path)) in jobs.iter().enumerate() {
            if let Some(change) = self.refresh_file(folder, path)? {
                stats.read += 1;
                stats.messages += change;
            }
            progress(done as u32 + 1, total);
        }
        self.mark_refreshed(crate::ipc::now_ms())?;
        Ok(stats)
    }

    /// The number of answers added or changed, or `None` when the file had nothing new.
    fn refresh_file(&self, folder: &str, path: &Path) -> AppResult<Option<u32>> {
        let Ok(meta) = std::fs::metadata(path) else {
            return Ok(None);
        };
        let len = meta.len();
        let mtime_ms = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_millis() as i64);
        let key = path.to_string_lossy().into_owned();
        let mut from = 0;
        if let Some((old_len, old_mtime, offset)) = self.file_state(&key)? {
            if old_len == len && old_mtime == mtime_ms {
                return Ok(None);
            }
            if len < offset {
                self.forget_file(&key)?;
            } else {
                from = offset;
            }
        }
        let Ok(read) = read_from(path, from) else {
            return Ok(None);
        };
        self.save(
            folder,
            &key,
            len,
            mtime_ms,
            read.offset,
            read.skipped,
            &read.records,
        )?;
        Ok(Some(read.records.len() as u32))
    }
}

static RUNNING: AtomicBool = AtomicBool::new(false);
static DONE: AtomicU32 = AtomicU32::new(0);
static TOTAL: AtomicU32 = AtomicU32::new(0);

/// Held while a refresh runs. Only one can at a time.
pub struct Running(());

impl Running {
    /// `None` when a refresh is already running.
    pub fn start() -> Option<Self> {
        RUNNING
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()?;
        DONE.store(0, Ordering::SeqCst);
        TOTAL.store(0, Ordering::SeqCst);
        Some(Running(()))
    }

    pub fn progress(&self, done: u32, total: u32) {
        DONE.store(done, Ordering::SeqCst);
        TOTAL.store(total, Ordering::SeqCst);
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        RUNNING.store(false, Ordering::SeqCst);
    }
}

/// Whether a refresh is running, and how far it is: files done of files to read.
pub fn status() -> (bool, u32, u32) {
    (
        RUNNING.load(Ordering::SeqCst),
        DONE.load(Ordering::SeqCst),
        TOTAL.load(Ordering::SeqCst),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(id: &str, stamp: &str, output: u64) -> String {
        format!(
            r#"{{"type":"assistant","timestamp":"{stamp}","sessionId":"s1","message":{{"id":"{id}","model":"claude-sonnet-5-5","usage":{{"input_tokens":2,"output_tokens":{output},"cache_read_input_tokens":10,"cache_creation_input_tokens":4}}}}}}"#
        )
    }

    struct Fixture {
        _tmp: tempfile::TempDir,
        dir: PathBuf,
        store: UsageStore,
    }

    impl Fixture {
        fn new() -> Self {
            let tmp = tempfile::tempdir().unwrap();
            let dir = tmp.path().join("cfg");
            std::fs::create_dir_all(dir.join("projects/p")).unwrap();
            let db = Arc::new(Db::open(&tmp.path().join("t.db")).unwrap());
            Fixture {
                _tmp: tmp,
                dir,
                store: UsageStore::new(db),
            }
        }

        fn file(&self, name: &str) -> PathBuf {
            self.dir.join("projects/p").join(name)
        }

        fn refresh(&self) -> RefreshStats {
            self.store
                .refresh(std::slice::from_ref(&self.dir), &mut |_, _| {})
                .unwrap()
        }

        fn ids(&self) -> Vec<String> {
            let mut ids: Vec<String> = self
                .store
                .records(None, None)
                .unwrap()
                .into_iter()
                .map(|r| r.id)
                .collect();
            ids.sort();
            ids
        }
    }

    #[test]
    fn a_first_refresh_reads_everything_and_a_second_reads_nothing() {
        let f = Fixture::new();
        std::fs::write(
            f.file("a.jsonl"),
            format!(
                "{}\n{}\n",
                line("m1", "2026-10-08T00:00:00Z", 5),
                line("m2", "2026-10-09T00:00:00Z", 6)
            ),
        )
        .unwrap();
        assert_eq!(f.store.refreshed_at().unwrap(), None);
        let first = f.refresh();
        assert_eq!((first.files, first.read, first.messages), (1, 1, 2));
        assert_eq!(f.ids(), ["m1", "m2"]);
        assert!(f.store.refreshed_at().unwrap().is_some());

        let second = f.refresh();
        assert_eq!((second.files, second.read, second.messages), (1, 0, 0));
        assert_eq!(f.ids().len(), 2);
    }

    #[test]
    fn a_file_that_grew_is_read_from_where_it_stopped() {
        let f = Fixture::new();
        let path = f.file("a.jsonl");
        std::fs::write(
            &path,
            format!("{}\n", line("m1", "2026-10-08T00:00:00Z", 5)),
        )
        .unwrap();
        f.refresh();
        let mut text = std::fs::read_to_string(&path).unwrap();
        text.push_str(&format!("{}\n", line("m2", "2026-10-08T00:00:01Z", 6)));
        std::fs::write(&path, text).unwrap();
        let stats = f.refresh();
        // Only the new answer was read, not the first again.
        assert_eq!((stats.read, stats.messages), (1, 1));
        assert_eq!(f.ids(), ["m1", "m2"]);
    }

    #[test]
    fn a_line_still_being_written_is_counted_when_it_is_finished() {
        let f = Fixture::new();
        let path = f.file("a.jsonl");
        let whole = line("m2", "2026-10-08T00:00:01Z", 6);
        let half = &whole[..whole.len() / 2];
        std::fs::write(
            &path,
            format!("{}\n{half}", line("m1", "2026-10-08T00:00:00Z", 5)),
        )
        .unwrap();
        f.refresh();
        assert_eq!(f.ids(), ["m1"]);
        std::fs::write(
            &path,
            format!("{}\n{whole}\n", line("m1", "2026-10-08T00:00:00Z", 5)),
        )
        .unwrap();
        f.refresh();
        assert_eq!(f.ids(), ["m1", "m2"]);
        assert_eq!(f.store.skipped().unwrap(), 0);
    }

    #[test]
    fn a_message_written_again_keeps_its_last_copy() {
        let f = Fixture::new();
        let path = f.file("a.jsonl");
        std::fs::write(
            &path,
            format!("{}\n", line("m1", "2026-10-08T00:00:00Z", 5)),
        )
        .unwrap();
        f.refresh();
        let mut text = std::fs::read_to_string(&path).unwrap();
        text.push_str(&format!("{}\n", line("m1", "2026-10-08T00:00:00Z", 99)));
        std::fs::write(&path, text).unwrap();
        f.refresh();
        let records = f.store.records(None, None).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].output, 99);
    }

    #[test]
    fn a_file_that_was_replaced_by_a_shorter_one_is_read_again_whole() {
        let f = Fixture::new();
        let path = f.file("a.jsonl");
        std::fs::write(
            &path,
            format!(
                "{}\n{}\n",
                line("old1", "2026-10-08T00:00:00Z", 5),
                line("old2", "2026-10-08T00:00:01Z", 5)
            ),
        )
        .unwrap();
        f.refresh();
        std::fs::write(
            &path,
            format!("{}\n", line("new", "2026-10-09T00:00:00Z", 7)),
        )
        .unwrap();
        f.refresh();
        assert_eq!(f.ids(), ["new"]);
    }

    #[test]
    fn what_a_deleted_file_gave_is_kept() {
        let f = Fixture::new();
        let path = f.file("a.jsonl");
        std::fs::write(
            &path,
            format!("{}\n", line("m1", "2026-10-08T00:00:00Z", 5)),
        )
        .unwrap();
        f.refresh();
        std::fs::remove_file(&path).unwrap();
        let stats = f.refresh();
        assert_eq!(stats.files, 0);
        assert_eq!(f.ids(), ["m1"]);
    }

    #[test]
    fn a_range_and_a_folder_narrow_what_is_read() {
        let f = Fixture::new();
        std::fs::write(
            f.file("a.jsonl"),
            format!(
                "{}\n{}\n",
                line("early", "2026-10-01T00:00:00Z", 5),
                line("late", "2026-10-20T00:00:00Z", 5)
            ),
        )
        .unwrap();
        f.refresh();
        let since = super::super::scan::parse_timestamp("2026-10-10T00:00:00Z");
        let late = f.store.records(since, None).unwrap();
        assert_eq!(late.len(), 1);
        assert_eq!(late[0].id, "late");
        let folder = f.dir.to_string_lossy().into_owned();
        assert_eq!(f.store.records(None, Some(&folder)).unwrap().len(), 2);
        assert_eq!(f.store.records(None, Some("/elsewhere")).unwrap().len(), 0);
        assert_eq!(f.store.folders().unwrap(), [folder]);
    }

    #[test]
    fn what_could_not_be_read_is_counted_once_and_clear_forgets_all() {
        let f = Fixture::new();
        std::fs::write(
            f.file("a.jsonl"),
            format!(
                "{}\nnot json but has \"usage\" and \"assistant\"\n",
                line("m1", "2026-10-08T00:00:00Z", 5)
            ),
        )
        .unwrap();
        f.refresh();
        f.refresh();
        assert_eq!(f.store.skipped().unwrap(), 1);
        f.store.clear().unwrap();
        assert_eq!(
            (
                f.ids().len(),
                f.store.skipped().unwrap(),
                f.store.refreshed_at().unwrap()
            ),
            (0, 0, None)
        );
        // After a clear, the same files are read again.
        f.refresh();
        assert_eq!(f.ids(), ["m1"]);
    }

    #[test]
    fn progress_is_reported_after_every_file() {
        let f = Fixture::new();
        for name in ["a.jsonl", "b.jsonl", "c.jsonl"] {
            std::fs::write(
                f.file(name),
                format!("{}\n", line(name, "2026-10-08T00:00:00Z", 5)),
            )
            .unwrap();
        }
        let mut seen = Vec::new();
        f.store
            .refresh(std::slice::from_ref(&f.dir), &mut |done, total| {
                seen.push((done, total))
            })
            .unwrap();
        assert_eq!(seen, [(1, 3), (2, 3), (3, 3)]);
    }

    #[test]
    fn only_one_refresh_runs_at_a_time() {
        let first = Running::start().expect("none was running");
        assert!(Running::start().is_none());
        first.progress(2, 9);
        assert_eq!(status(), (true, 2, 9));
        drop(first);
        assert!(!status().0);
        assert!(Running::start().is_some());
    }
}
