use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

use super::model::{ProgressEvent, StateChange};

pub const STATE_CHANGED_EVENT: &str = "workspace:state-changed";
pub const PROGRESS_EVENT: &str = "workspace:progress";

/// Where state changes are announced. The app emits a Tauri event, tests collect them.
pub trait EventSink: Send + Sync {
    fn state_changed(&self, change: &StateChange);
    fn progress(&self, event: &ProgressEvent);
}

pub struct TauriSink(pub AppHandle);

impl EventSink for TauriSink {
    fn state_changed(&self, change: &StateChange) {
        if let Err(e) = self.0.emit(STATE_CHANGED_EVENT, change) {
            tracing::warn!(error = %e, "cannot emit {STATE_CHANGED_EVENT}");
        }
    }

    fn progress(&self, event: &ProgressEvent) {
        if let Err(e) = self.0.emit(PROGRESS_EVENT, event) {
            tracing::warn!(error = %e, "cannot emit {PROGRESS_EVENT}");
        }
    }
}

pub struct NullSink;

impl EventSink for NullSink {
    fn state_changed(&self, _change: &StateChange) {}
    fn progress(&self, _event: &ProgressEvent) {}
}

/// Named async locks. A workspace operation holds the lock for its id, and git operations that
/// mutate a repository's worktrees hold the project lock, so two requests never interleave.
#[derive(Default)]
pub struct Locks {
    map: Mutex<HashMap<String, Arc<AsyncMutex<()>>>>,
}

impl Locks {
    pub async fn acquire(&self, key: &str) -> OwnedMutexGuard<()> {
        let lock = {
            let mut map = self.map.lock().expect("lock map poisoned");
            map.entry(key.to_string()).or_default().clone()
        };
        lock.lock_owned().await
    }
}

#[cfg(test)]
pub struct CollectSink(pub Mutex<Vec<StateChange>>, pub Mutex<Vec<ProgressEvent>>);

#[cfg(test)]
impl CollectSink {
    pub fn new() -> Arc<Self> {
        Arc::new(Self(Mutex::new(Vec::new()), Mutex::new(Vec::new())))
    }

    pub fn stages(&self) -> Vec<String> {
        self.1
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.stage.clone())
            .collect()
    }

    pub fn changes(&self) -> Vec<StateChange> {
        self.0.lock().unwrap().clone()
    }
}

#[cfg(test)]
impl EventSink for CollectSink {
    fn state_changed(&self, change: &StateChange) {
        self.0.lock().unwrap().push(change.clone());
    }

    fn progress(&self, event: &ProgressEvent) {
        self.1.lock().unwrap().push(event.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn locks_serialize_work_per_key() {
        let locks = Arc::new(Locks::default());
        let active = Arc::new(AtomicUsize::new(0));
        let max_seen = Arc::new(AtomicUsize::new(0));

        let mut tasks = Vec::new();
        for _ in 0..8 {
            let (locks, active, max_seen) = (locks.clone(), active.clone(), max_seen.clone());
            tasks.push(tokio::spawn(async move {
                let _guard = locks.acquire("w1").await;
                let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                max_seen.fetch_max(now, Ordering::SeqCst);
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                active.fetch_sub(1, Ordering::SeqCst);
            }));
        }
        for t in tasks {
            t.await.unwrap();
        }
        assert_eq!(max_seen.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn different_keys_do_not_block_each_other() {
        let locks = Locks::default();
        let _a = locks.acquire("a").await;
        let b =
            tokio::time::timeout(std::time::Duration::from_millis(200), locks.acquire("b")).await;
        assert!(b.is_ok());
    }
}
