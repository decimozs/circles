//! Taking snapshots of a working tree and putting one back, without touching the branch, the index
//! or the history of the repository.

use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use ts_rs::TS;

use crate::error::{AppError, AppResult};
use crate::git::Git;

/// Untracked files bigger than this are left out of a snapshot.
pub const SIZE_CAP: u64 = 10 * 1024 * 1024;

const IDENTITY: [(&str, &str); 4] = [
    ("GIT_AUTHOR_NAME", "Circles"),
    ("GIT_AUTHOR_EMAIL", "circles@localhost"),
    ("GIT_COMMITTER_NAME", "Circles"),
    ("GIT_COMMITTER_EMAIL", "circles@localhost"),
];

/// A private index file, removed when dropped, so the user's index is never involved.
struct TempIndex(PathBuf);

impl TempIndex {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("circles-index-{}", uuid::Uuid::new_v4().simple())))
    }

    fn env(&self) -> [(&'static str, String); 1] {
        [("GIT_INDEX_FILE", self.0.to_string_lossy().into_owned())]
    }
}

impl Drop for TempIndex {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

async fn run_with_index(
    git: &Git,
    cwd: &Path,
    args: &[&str],
    index: &TempIndex,
) -> AppResult<String> {
    let env = index.env();
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    Ok(git.run_env(cwd, args, &env).await?)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub commit: String,
    pub tree: String,
    /// Large untracked files that were left out.
    pub skipped: Vec<String>,
}

/// The tree object of everything in the working tree that git would not ignore: tracked, modified
/// and untracked files. Deleted files are simply not in it.
pub async fn tree_of_worktree(git: &Git, cwd: &Path) -> AppResult<(String, Vec<String>)> {
    let untracked = git
        .run_env(
            cwd,
            &["ls-files", "--others", "--exclude-standard", "-z"],
            &[],
        )
        .await?;
    let skipped: Vec<String> = untracked
        .split('\0')
        .filter(|p| !p.is_empty())
        .filter(|p| std::fs::symlink_metadata(cwd.join(p)).is_ok_and(|m| m.len() > SIZE_CAP))
        .map(str::to_string)
        .collect();

    let index = TempIndex::new();
    let excludes: Vec<String> = skipped
        .iter()
        .map(|p| format!(":(exclude,literal){p}"))
        .collect();
    let mut args = vec!["add", "-A", "--", "."];
    args.extend(excludes.iter().map(String::as_str));
    run_with_index(git, cwd, &args, &index).await?;
    let tree = run_with_index(git, cwd, &["write-tree"], &index).await?;
    Ok((tree.trim().to_string(), skipped))
}

/// Writes a commit of the working tree and returns it. No ref is made.
pub async fn snapshot(git: &Git, cwd: &Path, message: &str) -> AppResult<Snapshot> {
    let (tree, skipped) = tree_of_worktree(git, cwd).await?;
    let mut args = vec!["commit-tree", tree.as_str(), "-m", message];
    let parent = git.rev_parse(cwd, "HEAD").await.ok();
    if let Some(parent) = parent.as_deref() {
        args.extend(["-p", parent]);
    }
    let commit = git.run_env(cwd, &args, &IDENTITY).await?.trim().to_string();
    Ok(Snapshot {
        commit,
        tree,
        skipped,
    })
}

pub async fn set_ref(git: &Git, cwd: &Path, name: &str, commit: &str) -> AppResult<()> {
    git.run_env(cwd, &["update-ref", name, commit], &[]).await?;
    Ok(())
}

pub async fn delete_ref(git: &Git, cwd: &Path, name: &str) -> AppResult<()> {
    git.run_env(cwd, &["update-ref", "-d", name], &[]).await?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ChangeKind {
    /// The file exists in the snapshot and not now: it comes back.
    Restored,
    /// The file changed since the snapshot: it goes back to how it was.
    Modified,
    /// The file did not exist then: it is removed.
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct FileChange {
    pub path: String,
    pub kind: ChangeKind,
}

/// What turning the working tree into `target_tree` would do, file by file.
async fn changes_to(
    git: &Git,
    cwd: &Path,
    current_tree: &str,
    target_tree: &str,
) -> AppResult<Vec<FileChange>> {
    let out = git
        .run_env(
            cwd,
            &[
                "diff-tree",
                "-r",
                "--no-renames",
                "--name-status",
                "-z",
                current_tree,
                target_tree,
            ],
            &[],
        )
        .await?;
    let mut tokens = out.split('\0').filter(|t| !t.is_empty());
    let mut changes = Vec::new();
    while let (Some(status), Some(path)) = (tokens.next(), tokens.next()) {
        let kind = match status.chars().next() {
            Some('A') => ChangeKind::Restored,
            Some('D') => ChangeKind::Removed,
            _ => ChangeKind::Modified,
        };
        changes.push(FileChange {
            path: path.to_string(),
            kind,
        });
    }
    Ok(changes)
}

async fn tree_of(git: &Git, cwd: &Path, commit: &str) -> AppResult<String> {
    Ok(git.rev_parse(cwd, &format!("{commit}^{{tree}}")).await?)
}

/// The files that restoring `commit` would change, without changing anything.
pub async fn preview(git: &Git, cwd: &Path, commit: &str) -> AppResult<Vec<FileChange>> {
    let (current, _) = tree_of_worktree(git, cwd).await?;
    let target = tree_of(git, cwd, commit).await?;
    changes_to(git, cwd, &current, &target).await
}

fn safe(path: &str) -> AppResult<&Path> {
    let p = Path::new(path);
    if p.is_absolute()
        || p.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::Prefix(_) | Component::RootDir
            )
        })
    {
        return Err(AppError::Invalid(format!(
            "{path} is not a path inside the workspace."
        )));
    }
    Ok(p)
}

struct Entry {
    mode: String,
    sha: String,
    path: String,
}

/// Mode, blob and path of each of `paths` in `tree`.
async fn entries(git: &Git, cwd: &Path, tree: &str, paths: &[&str]) -> AppResult<Vec<Entry>> {
    let mut found = Vec::new();
    for chunk in paths.chunks(100) {
        let mut args = vec!["ls-tree", "-r", "-z", tree, "--"];
        args.extend(chunk.iter().copied());
        let out = git.run_env(cwd, &args, &[]).await?;
        for record in out.split('\0').filter(|r| !r.is_empty()) {
            let Some((meta, path)) = record.split_once('\t') else {
                continue;
            };
            let mut parts = meta.split_whitespace();
            let (Some(mode), Some(_kind), Some(sha)) = (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            found.push(Entry {
                mode: mode.into(),
                sha: sha.into(),
                path: path.into(),
            });
        }
    }
    Ok(found)
}

#[cfg(unix)]
fn set_executable(path: &Path, executable: bool) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_mode(if executable { 0o755 } else { 0o644 });
    std::fs::set_permissions(path, permissions)
}

/// Removes now-empty folders above a deleted file, up to `root`.
fn prune_empty_parents(root: &Path, from: &Path) {
    let mut dir = from.parent();
    while let Some(d) = dir {
        if d == root || std::fs::remove_dir(d).is_err() {
            break;
        }
        dir = d.parent();
    }
}

/// Puts the working tree back to how `commit` had it: files that changed are restored, files that
/// were deleted come back, and files that did not exist then are removed. Files git ignores are not
/// touched.
///
/// It happens in two steps. First everything that has to be written is written to `staging`, so a
/// failure there changes nothing. Then the files are moved into place.
pub async fn restore(
    git: &Git,
    cwd: &Path,
    commit: &str,
    staging: &Path,
) -> AppResult<Vec<FileChange>> {
    let (current, _) = tree_of_worktree(git, cwd).await?;
    let target = tree_of(git, cwd, commit).await?;
    let changes = changes_to(git, cwd, &current, &target).await?;
    if changes.is_empty() {
        return Ok(changes);
    }
    for change in &changes {
        safe(&change.path)?;
    }

    // Step 1: write what comes back into the staging folder.
    let to_write: Vec<&str> = changes
        .iter()
        .filter(|c| c.kind != ChangeKind::Removed)
        .map(|c| c.path.as_str())
        .collect();
    let stored = entries(git, cwd, &target, &to_write).await?;
    let _ = std::fs::remove_dir_all(staging);
    for entry in &stored {
        let file = staging.join(&entry.path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let bytes = git.blob_bytes(cwd, &entry.sha).await?;
        if entry.mode == "120000" {
            #[cfg(unix)]
            std::os::unix::fs::symlink(String::from_utf8_lossy(&bytes).as_ref(), &file)?;
        } else {
            std::fs::write(&file, &bytes)?;
            #[cfg(unix)]
            set_executable(&file, entry.mode == "100755")?;
        }
    }

    // Step 2: move it into place.
    for change in &changes {
        let dest = cwd.join(&change.path);
        match change.kind {
            ChangeKind::Removed => {
                match std::fs::symlink_metadata(&dest) {
                    Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(&dest)?,
                    Ok(_) => std::fs::remove_file(&dest)?,
                    Err(_) => {}
                }
                prune_empty_parents(cwd, &dest);
            }
            ChangeKind::Restored | ChangeKind::Modified => {
                // A folder where a file belongs, or a file where a folder belongs, is in the way.
                if let Ok(meta) = std::fs::symlink_metadata(&dest) {
                    if meta.is_dir() {
                        std::fs::remove_dir_all(&dest)?;
                    }
                }
                let mut ancestor = dest.parent();
                while let Some(a) = ancestor {
                    if a == cwd {
                        break;
                    }
                    if std::fs::symlink_metadata(a).is_ok_and(|m| !m.is_dir()) {
                        std::fs::remove_file(a)?;
                    }
                    ancestor = a.parent();
                }
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::rename(staging.join(&change.path), &dest)?;
            }
        }
    }
    let _ = std::fs::remove_dir_all(staging);
    Ok(changes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::testutil::{repo_with_commit, sh, test_git};
    use std::fs;

    struct Repo {
        _tmp: tempfile::TempDir,
        git: Git,
        dir: PathBuf,
    }

    async fn repo() -> Repo {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let git = test_git(&home);
        let dir = fs::canonicalize(tmp.path()).unwrap().join("repo");
        fs::create_dir_all(&dir).unwrap();
        repo_with_commit(&git, &dir).await;
        // The way a workspace keeps `.context` out of git.
        fs::write(dir.join(".git/info/exclude"), ".context/\n").unwrap();
        fs::write(dir.join(".gitignore"), "*.log\n").unwrap();
        sh(&dir, &["add", "."]);
        sh(&dir, &["commit", "-q", "-m", "ignore logs"]);
        Repo {
            _tmp: tmp,
            git,
            dir,
        }
    }

    fn out(dir: &Path, args: &[&str]) -> String {
        let o = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        String::from_utf8_lossy(&o.stdout).into_owned()
    }

    fn files_in(git_tree_ls: &str) -> Vec<String> {
        git_tree_ls
            .lines()
            .map(|l| l.split('\t').next_back().unwrap().to_string())
            .collect()
    }

    #[tokio::test]
    async fn a_snapshot_has_everything_git_does_not_ignore_and_changes_nothing_else() {
        let r = repo().await;
        fs::write(r.dir.join("tracked.txt"), "one\n").unwrap();
        sh(&r.dir, &["add", "tracked.txt"]);
        sh(&r.dir, &["commit", "-q", "-m", "tracked"]);
        fs::write(r.dir.join("tracked.txt"), "two\n").unwrap();
        fs::write(r.dir.join("new.txt"), "new\n").unwrap();
        fs::write(r.dir.join("noise.log"), "ignored").unwrap();
        fs::create_dir_all(r.dir.join(".context")).unwrap();
        fs::write(r.dir.join(".context/notes.md"), "private").unwrap();
        sh(&r.dir, &["add", "new.txt"]); // something the user staged on purpose

        let before = (
            out(&r.dir, &["status", "--porcelain"]),
            out(&r.dir, &["rev-parse", "HEAD"]),
            out(&r.dir, &["log", "--oneline"]),
        );
        let snap = snapshot(&r.git, &r.dir, "Before turn 1").await.unwrap();
        set_ref(
            &r.git,
            &r.dir,
            "refs/circles/checkpoints/w1/s1/1",
            &snap.commit,
        )
        .await
        .unwrap();
        let after = (
            out(&r.dir, &["status", "--porcelain"]),
            out(&r.dir, &["rev-parse", "HEAD"]),
            out(&r.dir, &["log", "--oneline"]),
        );
        assert_eq!(
            before, after,
            "the index, the branch and its history are as they were"
        );
        assert_eq!(
            out(&r.dir, &["branch", "--list"]).lines().count(),
            1,
            "no branch was made"
        );

        let listing = files_in(&out(&r.dir, &["ls-tree", "-r", &snap.tree]));
        assert!(listing.contains(&"tracked.txt".to_string()));
        assert!(listing.contains(&"new.txt".to_string()));
        assert!(
            !listing.contains(&"noise.log".to_string()),
            "ignored files stay out"
        );
        assert!(
            !listing.iter().any(|p| p.starts_with(".context")),
            ".context stays out"
        );
        assert_eq!(
            out(&r.dir, &["show", &format!("{}:tracked.txt", snap.commit)]),
            "two\n",
            "it has the modified content"
        );
        assert_eq!(
            out(&r.dir, &["rev-parse", "refs/circles/checkpoints/w1/s1/1"]).trim(),
            snap.commit
        );
        assert_eq!(
            out(&r.dir, &["rev-parse", &format!("{}^", snap.commit)]).trim(),
            out(&r.dir, &["rev-parse", "HEAD"]).trim()
        );
        assert!(snap.skipped.is_empty());
    }

    #[tokio::test]
    async fn very_large_untracked_files_are_left_out_and_reported() {
        let r = repo().await;
        let big = fs::File::create(r.dir.join("huge.bin")).unwrap();
        big.set_len(SIZE_CAP + 1).unwrap();
        fs::write(r.dir.join("small.txt"), "x").unwrap();
        let snap = snapshot(&r.git, &r.dir, "m").await.unwrap();
        assert_eq!(snap.skipped, ["huge.bin"]);
        let listing = files_in(&out(&r.dir, &["ls-tree", "-r", &snap.tree]));
        assert!(listing.contains(&"small.txt".to_string()));
        assert!(!listing.contains(&"huge.bin".to_string()));
    }

    #[tokio::test]
    async fn restoring_puts_back_edits_deletions_and_removes_new_files_exactly() {
        let r = repo().await;
        fs::write(r.dir.join("a.txt"), "a original\n").unwrap();
        fs::write(r.dir.join("b.txt"), "b original\n").unwrap();
        fs::create_dir_all(r.dir.join("keep")).unwrap();
        fs::write(r.dir.join("keep/deep.txt"), "deep\n").unwrap();
        fs::write(r.dir.join("blob.bin"), [0u8, 159, 146, 150, 255, 0, 1]).unwrap();
        fs::write(r.dir.join("run.sh"), "#!/bin/sh\necho hi\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(r.dir.join("run.sh"), fs::Permissions::from_mode(0o755)).unwrap();
        }
        let before = snapshot(&r.git, &r.dir, "before").await.unwrap();

        // What an agent might do next.
        fs::write(r.dir.join("a.txt"), "a changed\n").unwrap();
        fs::remove_file(r.dir.join("b.txt")).unwrap();
        fs::remove_dir_all(r.dir.join("keep")).unwrap();
        fs::write(r.dir.join("blob.bin"), [9u8, 9]).unwrap();
        fs::write(r.dir.join("run.sh"), "changed\n").unwrap();
        fs::create_dir_all(r.dir.join("made/inner")).unwrap();
        fs::write(r.dir.join("made/inner/new.txt"), "new\n").unwrap();
        fs::write(r.dir.join("fresh.txt"), "fresh\n").unwrap();
        fs::write(r.dir.join("kept.log"), "an ignored file").unwrap();

        let staging = r.dir.join(".context/staging");
        let changes = restore(&r.git, &r.dir, &before.commit, &staging)
            .await
            .unwrap();
        assert!(changes
            .iter()
            .any(|c| c.path == "b.txt" && c.kind == ChangeKind::Restored));
        assert!(changes
            .iter()
            .any(|c| c.path == "a.txt" && c.kind == ChangeKind::Modified));
        assert!(changes
            .iter()
            .any(|c| c.path == "fresh.txt" && c.kind == ChangeKind::Removed));

        assert_eq!(
            fs::read_to_string(r.dir.join("a.txt")).unwrap(),
            "a original\n"
        );
        assert_eq!(
            fs::read_to_string(r.dir.join("b.txt")).unwrap(),
            "b original\n"
        );
        assert_eq!(
            fs::read_to_string(r.dir.join("keep/deep.txt")).unwrap(),
            "deep\n"
        );
        assert_eq!(
            fs::read(r.dir.join("blob.bin")).unwrap(),
            [0u8, 159, 146, 150, 255, 0, 1],
            "binary files come back byte for byte"
        );
        assert!(!r.dir.join("fresh.txt").exists());
        assert!(
            !r.dir.join("made").exists(),
            "folders that only held new files are gone too"
        );
        assert_eq!(
            fs::read_to_string(r.dir.join("kept.log")).unwrap(),
            "an ignored file",
            "ignored files are not touched"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(r.dir.join("run.sh"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o111,
                0o111,
                "the executable bit comes back"
            );
        }
        assert!(!staging.exists(), "the staging folder is cleaned up");
        let (now, _) = tree_of_worktree(&r.git, &r.dir).await.unwrap();
        assert_eq!(now, before.tree, "the working tree is exactly the snapshot");
        assert!(
            restore(&r.git, &r.dir, &before.commit, &staging)
                .await
                .unwrap()
                .is_empty(),
            "restoring again changes nothing"
        );
    }

    #[tokio::test]
    async fn a_file_that_became_a_folder_and_back_is_put_right() {
        let r = repo().await;
        fs::write(r.dir.join("thing"), "a file\n").unwrap();
        fs::create_dir_all(r.dir.join("folder")).unwrap();
        fs::write(r.dir.join("folder/inside.txt"), "inside\n").unwrap();
        let before = snapshot(&r.git, &r.dir, "before").await.unwrap();
        fs::remove_file(r.dir.join("thing")).unwrap();
        fs::create_dir_all(r.dir.join("thing")).unwrap();
        fs::write(r.dir.join("thing/now-a-folder.txt"), "x").unwrap();
        fs::remove_dir_all(r.dir.join("folder")).unwrap();
        fs::write(r.dir.join("folder"), "now a file\n").unwrap();

        restore(
            &r.git,
            &r.dir,
            &before.commit,
            &r.dir.join(".context/staging"),
        )
        .await
        .unwrap();
        assert_eq!(fs::read_to_string(r.dir.join("thing")).unwrap(), "a file\n");
        assert_eq!(
            fs::read_to_string(r.dir.join("folder/inside.txt")).unwrap(),
            "inside\n"
        );
    }

    #[tokio::test]
    async fn a_failure_while_preparing_leaves_the_working_tree_unchanged() {
        let r = repo().await;
        fs::write(r.dir.join("a.txt"), "a original\n").unwrap();
        let before = snapshot(&r.git, &r.dir, "before").await.unwrap();
        fs::write(r.dir.join("a.txt"), "a changed\n").unwrap();
        fs::remove_file(r.dir.join(".gitignore")).unwrap();
        fs::write(r.dir.join("later.txt"), "later\n").unwrap();
        // A file where the staging folder should go: preparing cannot write there.
        fs::create_dir_all(r.dir.join(".context")).unwrap();
        fs::write(r.dir.join(".context/staging"), "in the way").unwrap();
        let staging = r.dir.join(".context/staging/inner");

        let result = restore(&r.git, &r.dir, &before.commit, &staging).await;
        assert!(result.is_err());
        assert_eq!(
            fs::read_to_string(r.dir.join("a.txt")).unwrap(),
            "a changed\n"
        );
        assert!(!r.dir.join(".gitignore").exists());
        assert_eq!(
            fs::read_to_string(r.dir.join("later.txt")).unwrap(),
            "later\n"
        );
    }

    #[tokio::test]
    async fn the_preview_lists_what_would_change_and_changes_nothing() {
        let r = repo().await;
        fs::write(r.dir.join("a.txt"), "a\n").unwrap();
        let before = snapshot(&r.git, &r.dir, "before").await.unwrap();
        fs::write(r.dir.join("a.txt"), "changed\n").unwrap();
        fs::write(r.dir.join("new.txt"), "n\n").unwrap();
        let changes = preview(&r.git, &r.dir, &before.commit).await.unwrap();
        let by = |p: &str| changes.iter().find(|c| c.path == p).map(|c| c.kind);
        assert_eq!(by("a.txt"), Some(ChangeKind::Modified));
        assert_eq!(by("new.txt"), Some(ChangeKind::Removed));
        assert_eq!(
            fs::read_to_string(r.dir.join("a.txt")).unwrap(),
            "changed\n"
        );
        assert!(r.dir.join("new.txt").exists());
    }

    #[test]
    fn paths_that_leave_the_workspace_are_refused() {
        assert!(safe("src/a.rs").is_ok());
        assert!(safe("../x").is_err());
        assert!(safe("a/../../x").is_err());
        assert!(safe("/etc/hosts").is_err());
    }
}
