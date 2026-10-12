//! Tells the UI when files in a workspace change, so the tree and the changes list stay current
//! while an agent edits.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};

use crate::error::{AppError, AppResult};

/// Events are gathered for this long before the UI hears about them.
pub const DEBOUNCE: Duration = Duration::from_millis(200);

/// Folders whose churn does not matter to a review.
const NOISE: &[&str] = &[
    ".git",
    ".context",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".turbo",
    ".venv",
    "__pycache__",
];

/// Whether a change at `path` matters. Changes inside noisy folders do not, except the git
/// directory's `index` and `HEAD`, which move when something is staged or committed.
pub fn is_relevant(root: &Path, git_dir: Option<&Path>, path: &Path) -> bool {
    if let Some(git_dir) = git_dir {
        if let Ok(inside) = path.strip_prefix(git_dir) {
            return matches!(inside.to_str(), Some("index") | Some("HEAD"));
        }
    }
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    !relative.components().any(|c| {
        c.as_os_str()
            .to_str()
            .is_some_and(|name| NOISE.contains(&name))
    })
}

/// Where git keeps this worktree's own files. A linked worktree has a `.git` file that points
/// there.
pub fn git_dir_of(root: &Path) -> Option<PathBuf> {
    let dot_git = root.join(".git");
    if dot_git.is_dir() {
        return Some(dot_git);
    }
    let text = std::fs::read_to_string(&dot_git).ok()?;
    let target = text.trim().strip_prefix("gitdir:")?.trim();
    let path = Path::new(target);
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    std::fs::canonicalize(path).ok()
}

/// Where branches, tags and remotes live. A linked worktree has its own git directory, and a
/// `commondir` file there points at the shared one.
pub fn common_dir_of(git_dir: &Path) -> PathBuf {
    std::fs::read_to_string(git_dir.join("commondir"))
        .ok()
        .and_then(|text| {
            let target = Path::new(text.trim());
            let joined = if target.is_absolute() {
                target.to_path_buf()
            } else {
                git_dir.join(target)
            };
            std::fs::canonicalize(joined).ok()
        })
        .unwrap_or_else(|| git_dir.to_path_buf())
}

/// Whether `path` is a branch, tag or remote moving: a commit, a push, a fetch.
pub fn is_ref_change(common: &Path, path: &Path) -> bool {
    path.strip_prefix(common)
        .is_ok_and(|inside| inside.starts_with("refs") || inside == Path::new("packed-refs"))
}

type Handle = Debouncer<notify::RecommendedWatcher>;

/// One watcher per workspace the UI has open.
#[derive(Default)]
pub struct Watchers {
    map: Mutex<HashMap<String, Handle>>,
}

impl Watchers {
    /// Starts watching `root`. `on_change` runs, at most once per debounce window, when something
    /// relevant changed. A second call for the same workspace replaces the first.
    pub fn watch(
        &self,
        workspace_id: &str,
        root: &Path,
        on_change: impl Fn() + Send + 'static,
    ) -> AppResult<()> {
        let root = std::fs::canonicalize(root)?;
        let git_dir = git_dir_of(&root);
        let common = git_dir.as_deref().map(common_dir_of);
        let watch_common = common.clone();
        let (watch_root, watch_git) = (root.clone(), git_dir.clone());
        let mut debouncer = new_debouncer(DEBOUNCE, move |result: DebounceEventResult| {
            if let Ok(events) = result {
                if events.iter().any(|e| {
                    is_relevant(&watch_root, watch_git.as_deref(), &e.path)
                        || watch_common
                            .as_deref()
                            .is_some_and(|c| is_ref_change(c, &e.path))
                }) {
                    on_change();
                }
            }
        })
        .map_err(|e| AppError::Internal(format!("cannot watch files: {e}")))?;
        debouncer
            .watcher()
            .watch(&root, RecursiveMode::Recursive)
            .map_err(|e| AppError::Internal(format!("cannot watch {}: {e}", root.display())))?;
        if let Some(git_dir) = git_dir.filter(|g| !g.starts_with(&root)) {
            // Best effort: without it, commits are only noticed when a file changes too.
            let _ = debouncer
                .watcher()
                .watch(&git_dir, RecursiveMode::NonRecursive);
        }
        if let Some(common) = common {
            // Best effort: the graph is also refreshed when the window gets focus.
            let _ = debouncer
                .watcher()
                .watch(&common.join("refs"), RecursiveMode::Recursive);
        }
        self.map
            .lock()
            .expect("watchers poisoned")
            .insert(workspace_id.to_string(), debouncer);
        Ok(())
    }

    pub fn unwatch(&self, workspace_id: &str) {
        self.map
            .lock()
            .expect("watchers poisoned")
            .remove(workspace_id);
    }

    pub fn clear(&self) {
        self.map.lock().expect("watchers poisoned").clear();
    }

    #[cfg(test)]
    pub fn count(&self) -> usize {
        self.map.lock().expect("watchers poisoned").len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn noisy_folders_do_not_count() {
        let root = Path::new("/w");
        let git = Path::new("/g");
        assert!(is_relevant(root, Some(git), Path::new("/w/src/a.rs")));
        assert!(is_relevant(root, None, Path::new("/w/README.md")));
        for noise in [
            "/w/node_modules/x/y.js",
            "/w/target/debug/a",
            "/w/.git/objects/ab",
            "/w/a/dist/b.js",
            "/w/.context/c",
        ] {
            assert!(!is_relevant(root, Some(git), Path::new(noise)), "{noise}");
        }
        assert!(!is_relevant(root, None, Path::new("/elsewhere/a.rs")));
    }

    #[test]
    fn only_the_index_and_head_matter_in_the_git_directory() {
        let root = Path::new("/w");
        let git = Path::new("/main/.git/worktrees/w");
        assert!(is_relevant(root, Some(git), &git.join("index")));
        assert!(is_relevant(root, Some(git), &git.join("HEAD")));
        assert!(!is_relevant(root, Some(git), &git.join("logs")));
        assert!(!is_relevant(root, Some(git), &git.join("ORIG_HEAD")));
    }

    #[test]
    fn a_linked_worktree_finds_its_git_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("main/.git/worktrees/w");
        std::fs::create_dir_all(&real).unwrap();
        let work = tmp.path().join("w");
        std::fs::create_dir_all(&work).unwrap();
        std::fs::write(work.join(".git"), format!("gitdir: {}\n", real.display())).unwrap();
        assert_eq!(
            git_dir_of(&work),
            Some(std::fs::canonicalize(&real).unwrap())
        );

        let plain = tmp.path().join("plain");
        std::fs::create_dir_all(plain.join(".git")).unwrap();
        assert_eq!(git_dir_of(&plain), Some(plain.join(".git")));
        assert_eq!(git_dir_of(tmp.path().join("none").as_path()), None);
    }

    #[test]
    fn a_worktree_shares_its_branches_with_the_main_repository() {
        let tmp = tempfile::tempdir().unwrap();
        let main = std::fs::canonicalize(tmp.path()).unwrap().join("main/.git");
        let wt = main.join("worktrees/w");
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::write(wt.join("commondir"), "../..\n").unwrap();
        let common = common_dir_of(&wt);
        assert_eq!(common, std::fs::canonicalize(&main).unwrap());
        assert_eq!(common_dir_of(&main), main, "no commondir means itself");

        assert!(is_ref_change(&common, &common.join("refs/heads/feature")));
        assert!(is_ref_change(
            &common,
            &common.join("refs/remotes/origin/main")
        ));
        assert!(is_ref_change(&common, &common.join("packed-refs")));
        assert!(!is_ref_change(&common, &common.join("objects/ab/cdef")));
        assert!(!is_ref_change(&common, &common.join("config")));
    }

    #[test]
    fn a_branch_moving_is_reported_even_though_git_is_noise() {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        std::fs::create_dir_all(root.join(".git/refs/heads")).unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let watchers = Watchers::default();
        let counter = hits.clone();
        watchers
            .watch("w1", &root, move || {
                counter.fetch_add(1, Ordering::SeqCst);
            })
            .unwrap();
        let baseline = settle(&hits);
        std::fs::write(root.join(".git/refs/heads/feature"), "abc\n").unwrap();
        assert!(wait_for(&hits, baseline + 1), "a branch update is reported");
        watchers.unwatch("w1");
    }

    /// Waits until nothing more is reported. A folder made just before watching can have its
    /// creation delivered late by macOS, so what is counted before a check is not always zero.
    fn settle(count: &AtomicUsize) -> usize {
        let mut last = count.load(Ordering::SeqCst);
        for _ in 0..10 {
            std::thread::sleep(Duration::from_millis(600));
            let now = count.load(Ordering::SeqCst);
            if now == last {
                return now;
            }
            last = now;
        }
        last
    }

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
    fn edits_are_reported_once_and_noise_is_not() {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        std::fs::create_dir_all(root.join("node_modules")).unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let watchers = Watchers::default();
        let counter = hits.clone();
        watchers
            .watch("w1", &root, move || {
                counter.fetch_add(1, Ordering::SeqCst);
            })
            .unwrap();
        assert_eq!(watchers.count(), 1);
        let baseline = settle(&hits);

        std::fs::write(root.join("node_modules/junk.js"), "x").unwrap();
        std::thread::sleep(Duration::from_millis(700));
        assert_eq!(hits.load(Ordering::SeqCst), baseline, "noise is ignored");

        for i in 0..5 {
            std::fs::write(root.join(format!("file{i}.txt")), "x").unwrap();
        }
        assert!(wait_for(&hits, baseline + 1), "an edit is reported");
        std::thread::sleep(Duration::from_millis(700));
        assert!(
            hits.load(Ordering::SeqCst) <= baseline + 2,
            "a burst is gathered, not reported five times"
        );

        watchers.unwatch("w1");
        assert_eq!(watchers.count(), 0);
    }
}
