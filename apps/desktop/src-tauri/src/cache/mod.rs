//! A small store for answers that are slow to get and change little, such as what GitHub says
//! about a pull request. The next time one is asked for it is shown at once and fetched again in
//! the background. Everything in it can be deleted: it is only ever a copy.

use std::sync::Arc;

use rusqlite::params;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::db::Db;
use crate::error::AppResult;
use crate::ipc::now_ms;

/// The most the store keeps, in bytes. The least recently used entries go first.
pub const MAX_BYTES: i64 = 100 * 1024 * 1024;
/// An entry nobody asked for in this long is dropped.
pub const MAX_AGE_MS: i64 = 90 * 24 * 60 * 60 * 1000;

/// An answer, and when it was fetched (milliseconds).
#[derive(Debug, Clone, PartialEq)]
pub struct Entry<T> {
    pub value: T,
    pub fetched_at: i64,
}

#[derive(Clone)]
pub struct CacheStore {
    db: Arc<Db>,
}

impl CacheStore {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    /// The answer kept under `key`, if there is one that can still be read.
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> AppResult<Option<Entry<T>>> {
        let row: Option<(String, i64)> = self.db.read(|c| {
            let found: rusqlite::Result<(String, i64)> = c.query_row(
                "SELECT value, fetched_at FROM cache WHERE key = ?1",
                [key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            );
            Ok(found.ok())
        })?;
        let Some((text, fetched_at)) = row else {
            return Ok(None);
        };
        // An entry written by an older version that cannot be read is as good as absent.
        let Ok(value) = serde_json::from_str(&text) else {
            self.delete(key)?;
            return Ok(None);
        };
        self.db.write(|c| {
            c.execute(
                "UPDATE cache SET last_used = ?2 WHERE key = ?1",
                params![key, now_ms()],
            )
            .map(|_| ())
        })?;
        Ok(Some(Entry { value, fetched_at }))
    }

    /// Keeps `value` under `key`, replacing what was there, and returns when it was kept.
    pub fn put<T: Serialize>(&self, key: &str, value: &T) -> AppResult<i64> {
        let text = serde_json::to_string(value)
            .map_err(|e| crate::error::AppError::Internal(format!("cannot keep an answer: {e}")))?;
        let now = now_ms();
        self.db.write(|c| {
            c.execute(
                "INSERT INTO cache (key, value, fetched_at, last_used, bytes)
                 VALUES (?1, ?2, ?3, ?3, ?4)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value,
                    fetched_at = excluded.fetched_at, last_used = excluded.last_used,
                    bytes = excluded.bytes",
                params![key, text, now, text.len() as i64],
            )
            .map(|_| ())
        })?;
        Ok(now)
    }

    pub fn delete(&self, key: &str) -> AppResult<()> {
        self.db.write(|c| {
            c.execute("DELETE FROM cache WHERE key = ?1", [key])
                .map(|_| ())
        })
    }

    /// Entries and bytes kept.
    pub fn info(&self) -> AppResult<(u32, u64)> {
        self.db.read(|c| {
            c.query_row(
                "SELECT COUNT(*), COALESCE(SUM(bytes), 0) FROM cache",
                [],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?.max(0) as u32,
                        r.get::<_, i64>(1)?.max(0) as u64,
                    ))
                },
            )
        })
    }

    pub fn clear(&self) -> AppResult<()> {
        self.db
            .write(|c| c.execute("DELETE FROM cache", []).map(|_| ()))
    }

    /// Drops what nobody asked for lately, then the least recently used until it is small enough.
    /// Returns how many entries went.
    pub fn prune(&self, max_bytes: i64, max_age_ms: i64) -> AppResult<u32> {
        self.db.write(|c| {
            let tx = c.unchecked_transaction()?;
            let mut gone = tx.execute(
                "DELETE FROM cache WHERE last_used < ?1",
                [now_ms() - max_age_ms],
            )?;
            let mut total: i64 =
                tx.query_row("SELECT COALESCE(SUM(bytes), 0) FROM cache", [], |r| {
                    r.get(0)
                })?;
            if total > max_bytes {
                let victims: Vec<(String, i64)> = {
                    let mut stmt = tx.prepare("SELECT key, bytes FROM cache ORDER BY last_used")?;
                    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
                    rows.collect::<rusqlite::Result<_>>()?
                };
                for (key, bytes) in victims {
                    if total <= max_bytes {
                        break;
                    }
                    tx.execute("DELETE FROM cache WHERE key = ?1", [&key])?;
                    total -= bytes;
                    gone += 1;
                }
            }
            tx.commit()?;
            Ok(gone as u32)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Thing {
        name: String,
        n: u32,
    }

    fn store() -> (tempfile::TempDir, CacheStore) {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::open(&tmp.path().join("t.db")).unwrap());
        (tmp, CacheStore::new(db))
    }

    fn thing(name: &str) -> Thing {
        Thing {
            name: name.into(),
            n: 1,
        }
    }

    #[test]
    fn what_was_kept_comes_back_with_when_it_was_kept() {
        let (_tmp, cache) = store();
        assert_eq!(cache.get::<Thing>("a").unwrap(), None);
        let at = cache.put("a", &thing("first")).unwrap();
        let got = cache.get::<Thing>("a").unwrap().unwrap();
        assert_eq!((got.value, got.fetched_at), (thing("first"), at));
        // Keeping again replaces it.
        cache.put("a", &thing("second")).unwrap();
        assert_eq!(
            cache.get::<Thing>("a").unwrap().unwrap().value.name,
            "second"
        );
        assert_eq!(cache.info().unwrap().0, 1);
    }

    #[test]
    fn an_entry_that_cannot_be_read_any_more_is_dropped_not_an_error() {
        let (_tmp, cache) = store();
        cache.put("a", &thing("x")).unwrap();
        #[derive(Debug, Deserialize)]
        #[allow(dead_code)]
        struct Newer {
            missing_field: String,
        }
        assert!(cache.get::<Newer>("a").unwrap().is_none());
        assert_eq!(cache.info().unwrap().0, 0);
    }

    #[test]
    fn delete_and_clear_remove_entries() {
        let (_tmp, cache) = store();
        cache.put("a", &thing("a")).unwrap();
        cache.put("b", &thing("b")).unwrap();
        cache.delete("a").unwrap();
        assert!(cache.get::<Thing>("a").unwrap().is_none());
        assert!(cache.get::<Thing>("b").unwrap().is_some());
        cache.clear().unwrap();
        assert_eq!(cache.info().unwrap(), (0, 0));
    }

    #[test]
    fn pruning_drops_the_old_and_then_the_least_recently_used_until_it_fits() {
        let (_tmp, cache) = store();
        for key in ["old", "a", "b", "c"] {
            cache.put(key, &thing(key)).unwrap();
        }
        // `old` was last asked for long ago; of the others `a` is the least recently used.
        cache
            .db
            .write(|c| {
                c.execute("UPDATE cache SET last_used = 1 WHERE key = 'old'", [])?;
                c.execute(
                    "UPDATE cache SET last_used = ?1 WHERE key = 'a'",
                    [now_ms() - 3000],
                )?;
                c.execute(
                    "UPDATE cache SET last_used = ?1 WHERE key = 'b'",
                    [now_ms() - 2000],
                )?;
                c.execute(
                    "UPDATE cache SET last_used = ?1 WHERE key = 'c'",
                    [now_ms() - 1000],
                )
            })
            .unwrap();
        let (entries, bytes) = cache.info().unwrap();
        assert_eq!(entries, 4);
        let each = (bytes / 4) as i64;
        // Old goes by age. Room for two more of the same size drops `a`.
        let gone = cache.prune(each * 2 + 1, 10_000).unwrap();
        assert_eq!(gone, 2);
        assert!(cache.get::<Thing>("old").unwrap().is_none());
        assert!(cache.get::<Thing>("a").unwrap().is_none());
        assert!(cache.get::<Thing>("b").unwrap().is_some());
        assert!(cache.get::<Thing>("c").unwrap().is_some());
        // Nothing to prune when it fits.
        assert_eq!(cache.prune(MAX_BYTES, MAX_AGE_MS).unwrap(), 0);
    }
}
