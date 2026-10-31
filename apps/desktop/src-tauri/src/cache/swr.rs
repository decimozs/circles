//! Answer from what is kept, and ask again in the background when it is old.

use std::collections::HashSet;
use std::future::Future;
use std::sync::{Mutex, OnceLock};

use serde::de::DeserializeOwned;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};

use super::CacheStore;
use crate::error::AppResult;
use crate::ipc::now_ms;

fn in_flight() -> &'static Mutex<HashSet<String>> {
    static SET: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    SET.get_or_init(Default::default)
}

/// Held while a key is being fetched in the background, so the same one is not fetched twice at
/// once.
pub struct Flight(String);

impl Flight {
    pub fn start(key: &str) -> Option<Self> {
        in_flight()
            .lock()
            .ok()?
            .insert(key.to_string())
            .then(|| Flight(key.to_string()))
    }
}

impl Drop for Flight {
    fn drop(&mut self) {
        if let Ok(mut set) = in_flight().lock() {
            set.remove(&self.0);
        }
    }
}

/// What is kept under `key` when there is something, asking again in the background (and telling
/// the window with `cache:changed` and `event`) when it is older than `max_age_ms`. With nothing
/// kept, `fetch` is waited for and its answer kept. `fetch` makes a new request each time it is
/// called.
pub async fn serve<R, T, F, Fut>(
    app: &AppHandle<R>,
    cache: &CacheStore,
    key: &str,
    max_age_ms: i64,
    event: &'static str,
    fetch: F,
) -> AppResult<T>
where
    R: Runtime,
    T: Serialize + DeserializeOwned + Send + 'static,
    F: Fn() -> Fut + Send + 'static,
    Fut: Future<Output = AppResult<T>> + Send + 'static,
{
    if let Some(kept) = cache.get::<T>(key)? {
        if now_ms() - kept.fetched_at > max_age_ms {
            if let Some(flight) = Flight::start(key) {
                let (app, cache, key) = (app.clone(), cache.clone(), key.to_string());
                tauri::async_runtime::spawn(async move {
                    if let Ok(fresh) = fetch().await {
                        let _ = cache.put(&key, &fresh);
                    }
                    drop(flight);
                    let _ = app.emit("cache:changed", event);
                });
            }
        }
        return Ok(kept.value);
    }
    let fresh = fetch().await?;
    cache.put(key, &fresh)?;
    Ok(fresh)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    use std::time::Duration;
    use tauri::Listener;

    fn store() -> (tempfile::TempDir, CacheStore) {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(crate::db::Db::open(&tmp.path().join("t.db")).unwrap());
        (tmp, CacheStore::new(db))
    }

    /// A fetch that counts how often it ran and answers with the count.
    fn counting() -> (
        Arc<AtomicU32>,
        impl Fn() -> std::future::Ready<AppResult<u32>> + Send + 'static,
    ) {
        let calls = Arc::new(AtomicU32::new(0));
        let seen = calls.clone();
        (calls, move || {
            std::future::ready(Ok(seen.fetch_add(1, Ordering::SeqCst) + 100))
        })
    }

    #[tokio::test]
    async fn with_nothing_kept_it_waits_for_the_answer_and_keeps_it() {
        let app = tauri::test::mock_app();
        let (_tmp, cache) = store();
        let (calls, fetch) = counting();
        let got: u32 = serve(app.handle(), &cache, "swr-test-0", 1_000, "ev", fetch)
            .await
            .unwrap();
        assert_eq!(got, 100);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(cache.get::<u32>("swr-test-0").unwrap().unwrap().value, 100);
    }

    #[tokio::test]
    async fn what_is_kept_and_young_is_returned_without_asking_again() {
        let app = tauri::test::mock_app();
        let (_tmp, cache) = store();
        cache.put("swr-test-1", &7u32).unwrap();
        let (calls, fetch) = counting();
        let got: u32 = serve(app.handle(), &cache, "swr-test-1", 60_000, "ev", fetch)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(got, 7);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn what_is_kept_and_old_is_returned_at_once_and_replaced_in_the_background() {
        let app = tauri::test::mock_app();
        let (_tmp, cache) = store();
        cache.put("swr-test-2", &7u32).unwrap();
        let (told, heard) = tokio::sync::mpsc::unbounded_channel::<String>();
        app.listen("cache:changed", move |e| {
            let _ = told.send(e.payload().to_string());
        });
        let mut heard = heard;
        // Nothing may be older than "now", so a maximum age below zero makes it old.
        let (calls, fetch) = counting();
        let got: u32 = serve(
            app.handle(),
            &cache,
            "swr-test-2",
            -1,
            "github-repos",
            fetch,
        )
        .await
        .unwrap();
        // The old answer comes back, not the new one.
        assert_eq!(got, 7);
        let payload = tokio::time::timeout(Duration::from_secs(5), heard.recv())
            .await
            .expect("the window is told")
            .unwrap();
        assert_eq!(payload, "\"github-repos\"");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(cache.get::<u32>("swr-test-2").unwrap().unwrap().value, 100);
    }

    #[tokio::test]
    async fn a_failing_refresh_keeps_what_was_there() {
        let app = tauri::test::mock_app();
        let (_tmp, cache) = store();
        cache.put("swr-test-3", &7u32).unwrap();
        let got: u32 = serve(app.handle(), &cache, "swr-test-3", -1, "ev", || {
            std::future::ready(Err::<u32, _>(crate::error::AppError::Internal(
                "down".into(),
            )))
        })
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(got, 7);
        assert_eq!(cache.get::<u32>("swr-test-3").unwrap().unwrap().value, 7);
    }

    #[tokio::test]
    async fn with_nothing_kept_a_failure_is_an_error() {
        let app = tauri::test::mock_app();
        let (_tmp, cache) = store();
        let got = serve(app.handle(), &cache, "swr-test-4", 1_000, "ev", || {
            std::future::ready(Err::<u32, _>(crate::error::AppError::Internal(
                "down".into(),
            )))
        })
        .await;
        assert!(got.is_err());
        assert!(cache.get::<u32>("swr-test-4").unwrap().is_none());
    }
}
