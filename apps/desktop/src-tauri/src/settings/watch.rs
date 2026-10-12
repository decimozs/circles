//! Tells the UI when a settings file changes, including by hand in an editor.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};

use crate::error::{AppError, AppResult};

const DEBOUNCE: Duration = Duration::from_millis(250);
const FILE_NAME: &str = "settings.toml";

type Handle = Debouncer<notify::RecommendedWatcher>;

/// Watches the settings files of the levels the settings page is showing.
#[derive(Default)]
pub struct SettingsWatcher {
    current: Mutex<Option<Handle>>,
}

impl SettingsWatcher {
    /// Starts watching `files`, replacing what was watched before. A folder that does not exist yet
    /// is not watched. `on_change` runs once per burst of changes.
    pub fn watch(&self, files: &[PathBuf], on_change: impl Fn() + Send + 'static) -> AppResult<()> {
        let mut debouncer = new_debouncer(DEBOUNCE, move |result: DebounceEventResult| {
            if let Ok(events) = result {
                if events
                    .iter()
                    .any(|e| e.path.file_name().is_some_and(|n| n == FILE_NAME))
                {
                    on_change();
                }
            }
        })
        .map_err(|e| AppError::Internal(format!("cannot watch settings: {e}")))?;
        for file in files {
            let Some(folder) = file.parent().filter(|f| f.is_dir()) else {
                continue;
            };
            let folder = std::fs::canonicalize(folder)?;
            debouncer
                .watcher()
                .watch(&folder, RecursiveMode::NonRecursive)
                .map_err(|e| {
                    AppError::Internal(format!("cannot watch {}: {e}", folder.display()))
                })?;
        }
        *self.current.lock().expect("settings watcher poisoned") = Some(debouncer);
        Ok(())
    }

    pub fn stop(&self) {
        *self.current.lock().expect("settings watcher poisoned") = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn wait_for(count: &AtomicUsize, at_least: usize) -> bool {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            if count.load(Ordering::SeqCst) >= at_least {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }

    #[test]
    fn an_edit_by_hand_is_reported_and_other_files_are_not() {
        let tmp = tempfile::tempdir().unwrap();
        let folder = std::fs::canonicalize(tmp.path()).unwrap();
        let file = folder.join("settings.toml");
        std::fs::write(&file, "[general]\n").unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        let watcher = SettingsWatcher::default();
        watcher
            .watch(std::slice::from_ref(&file), move || {
                counter.fetch_add(1, Ordering::SeqCst);
            })
            .unwrap();
        std::thread::sleep(Duration::from_millis(300));

        std::fs::write(folder.join("notes.txt"), "not a settings file").unwrap();
        std::thread::sleep(Duration::from_millis(700));
        assert_eq!(hits.load(Ordering::SeqCst), 0);

        std::fs::write(&file, "[general]\nsound = false\n").unwrap();
        assert!(wait_for(&hits, 1), "the edit is reported");
        std::thread::sleep(Duration::from_millis(700));
        assert!(
            hits.load(Ordering::SeqCst) <= 2,
            "one burst is not reported many times"
        );

        watcher.stop();
        let before = hits.load(Ordering::SeqCst);
        std::fs::write(&file, "[general]\nsound = true\n").unwrap();
        std::thread::sleep(Duration::from_millis(800));
        assert_eq!(
            hits.load(Ordering::SeqCst),
            before,
            "nothing is reported after it stops"
        );
    }

    #[test]
    fn a_missing_folder_is_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let watcher = SettingsWatcher::default();
        watcher
            .watch(&[tmp.path().join("not-yet/.circles/settings.toml")], || {})
            .unwrap();
    }
}
