//! The files a workspace changed, and the two sides of each file for the diff viewer.

use std::path::Path;

use serde::Serialize;
use ts_rs::TS;

use crate::error::AppResult;
use crate::files::{looks_like_text, TEXT_LIMIT};
use crate::git::parse::{FileKind, FileStat};
use crate::git::Git;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ChangeStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ChangeScope {
    /// Everything since the branch left its base branch, uncommitted work included.
    All,
    /// Only what is not committed yet.
    Uncommitted,
}

impl ChangeScope {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "all" => Some(Self::All),
            "uncommitted" => Some(Self::Uncommitted),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ChangedFile {
    pub path: String,
    pub old_path: Option<String>,
    pub status: ChangeStatus,
    pub added: u32,
    pub removed: u32,
    pub binary: bool,
}

/// Status letters from `git diff --name-status -z`, keyed by the new path.
pub fn parse_name_status(text: &str) -> Vec<(String, Option<String>, ChangeStatus)> {
    let mut out = Vec::new();
    let mut tokens = text.split('\0').filter(|t| !t.is_empty());
    while let Some(letter) = tokens.next() {
        let status = match letter.chars().next() {
            Some('A') => ChangeStatus::Added,
            Some('D') => ChangeStatus::Deleted,
            Some('R') | Some('C') => ChangeStatus::Renamed,
            _ => ChangeStatus::Modified,
        };
        if status == ChangeStatus::Renamed {
            let old = tokens.next().unwrap_or("").to_string();
            let new = tokens.next().unwrap_or("").to_string();
            out.push((new, Some(old), status));
        } else if let Some(path) = tokens.next() {
            out.push((path.to_string(), None, status));
        }
    }
    out
}

/// The branch a workspace is compared with: the remote copy when there is one.
pub(crate) async fn base_ref(git: &Git, cwd: &Path, base_branch: &str) -> String {
    // The base of a pull request is what is on GitHub. The local copy of the branch is only moved
    // when someone pulls, and a merge made on GitHub never moves it, so comparing with it counts
    // commits that are already merged as new ones.
    if git
        .remote_tracking_exists(cwd, "origin", base_branch)
        .await
        .unwrap_or(false)
    {
        format!("origin/{base_branch}")
    } else {
        base_branch.to_string()
    }
}

/// The revision a scope is compared against.
pub async fn base_rev(
    git: &Git,
    cwd: &Path,
    base_branch: &str,
    scope: ChangeScope,
) -> AppResult<String> {
    match scope {
        ChangeScope::Uncommitted => Ok("HEAD".into()),
        ChangeScope::All => {
            let base = base_ref(git, cwd, base_branch).await;
            // A branch with no common history with its base is compared with the base itself.
            Ok(git.merge_base(cwd, "HEAD", &base).await.unwrap_or(base))
        }
    }
}

fn count_lines(path: &Path) -> (u32, bool) {
    match std::fs::read(path) {
        Ok(bytes) if bytes.len() as u64 <= TEXT_LIMIT && looks_like_text(&bytes) => {
            let text = String::from_utf8_lossy(&bytes);
            (text.lines().count() as u32, false)
        }
        Ok(_) => (0, true),
        Err(_) => (0, false),
    }
}

/// Changed files of the workspace folder `cwd`, sorted by path.
pub async fn changed_files(
    git: &Git,
    cwd: &Path,
    base_branch: &str,
    scope: ChangeScope,
) -> AppResult<Vec<ChangedFile>> {
    let base = base_rev(git, cwd, base_branch, scope).await?;
    let stats: Vec<FileStat> = git.diff_numstat(cwd, &base, None).await?;
    let statuses = parse_name_status(&git.diff_name_status(cwd, &base).await?);

    let mut files: Vec<ChangedFile> = stats
        .into_iter()
        .map(|stat| {
            let status = statuses
                .iter()
                .find(|(path, _, _)| *path == stat.path)
                .map(|(_, _, status)| *status)
                .unwrap_or(if stat.old_path.is_some() {
                    ChangeStatus::Renamed
                } else {
                    ChangeStatus::Modified
                });
            ChangedFile {
                path: stat.path,
                old_path: stat.old_path,
                status,
                added: stat.added,
                removed: stat.deleted,
                binary: stat.binary,
            }
        })
        .collect();

    // Files git has not been told about yet are new work too.
    for file in git.status(cwd).await?.files {
        if file.kind == FileKind::Untracked && !files.iter().any(|f| f.path == file.path) {
            let (added, binary) = count_lines(&cwd.join(&file.path));
            files.push(ChangedFile {
                path: file.path,
                old_path: None,
                status: ChangeStatus::Added,
                added,
                removed: 0,
                binary,
            });
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct FilePair {
    pub path: String,
    /// The file before the changes. Unset when it was added.
    pub original: Option<String>,
    /// The file now. Unset when it was deleted.
    pub modified: Option<String>,
    pub language: String,
    pub binary: bool,
    pub too_large: bool,
}

/// Both sides of one changed file.
pub async fn file_pair(
    git: &Git,
    cwd: &Path,
    base_branch: &str,
    scope: ChangeScope,
    path: &str,
    old_path: Option<&str>,
) -> AppResult<FilePair> {
    let language = crate::files::language_for(path).to_string();
    let base = base_rev(git, cwd, base_branch, scope).await?;
    let original = git.show_blob(cwd, &base, old_path.unwrap_or(path)).await?;

    // The current side is read through the safe path check, and may be missing when deleted.
    let modified = match crate::files::resolve_inside(cwd, path) {
        Ok(real) => {
            let size = std::fs::metadata(&real)?.len();
            if size > TEXT_LIMIT {
                return Ok(FilePair {
                    path: path.into(),
                    original: None,
                    modified: None,
                    language,
                    binary: false,
                    too_large: true,
                });
            }
            Some(std::fs::read(real)?)
        }
        Err(e) if e.code() == "not_found" => None,
        Err(e) => return Err(e),
    };

    let binary = modified.as_deref().is_some_and(|b| !looks_like_text(b))
        || original
            .as_deref()
            .is_some_and(|t| t.contains('\0') || t.contains('\u{FFFD}'));
    let too_large = original
        .as_deref()
        .is_some_and(|t| t.len() as u64 > TEXT_LIMIT);
    if binary || too_large {
        return Ok(FilePair {
            path: path.into(),
            original: None,
            modified: None,
            language,
            binary,
            too_large,
        });
    }
    Ok(FilePair {
        path: path.into(),
        original,
        modified: modified.map(|b| String::from_utf8_lossy(&b).into_owned()),
        language,
        binary: false,
        too_large: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::testutil::{repo_with_commit, sh, test_git};

    #[test]
    fn name_status_reads_every_kind() {
        let parsed = parse_name_status("A\0new.rs\0M\0old.rs\0D\0gone.rs\0R100\0was.rs\0now.rs\0");
        assert_eq!(
            parsed,
            vec![
                ("new.rs".into(), None, ChangeStatus::Added),
                ("old.rs".into(), None, ChangeStatus::Modified),
                ("gone.rs".into(), None, ChangeStatus::Deleted),
                (
                    "now.rs".into(),
                    Some("was.rs".into()),
                    ChangeStatus::Renamed
                ),
            ]
        );
        assert!(parse_name_status("").is_empty());
    }

    struct Repo {
        _tmp: tempfile::TempDir,
        git: Git,
        dir: std::path::PathBuf,
    }

    async fn repo() -> Repo {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let git = test_git(&home);
        let dir = tmp.path().join("repo");
        std::fs::create_dir_all(&dir).unwrap();
        repo_with_commit(&git, &dir).await;
        Repo {
            _tmp: tmp,
            git,
            dir,
        }
    }

    fn branch_name(dir: &Path) -> String {
        let out = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(dir)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    #[tokio::test]
    async fn the_base_is_the_remote_copy_when_there_is_one() {
        let r = repo().await;
        let base = branch_name(&r.dir);
        // Nothing fetched yet: the local branch is the base.
        assert_eq!(base_ref(&r.git, &r.dir, &base).await, base);
        // Someone merged on GitHub: the remote copy is ahead of the local one.
        std::fs::write(r.dir.join("merged.txt"), "x\n").unwrap();
        sh(&r.dir, &["add", "."]);
        sh(&r.dir, &["commit", "-q", "-m", "merged on GitHub"]);
        sh(
            &r.dir,
            &["update-ref", &format!("refs/remotes/origin/{base}"), "HEAD"],
        );
        sh(&r.dir, &["reset", "-q", "--hard", "HEAD~1"]);
        sh(&r.dir, &["checkout", "-q", "-b", "feature"]);
        sh(
            &r.dir,
            &["merge", "-q", "--ff-only", &format!("origin/{base}")],
        );
        assert_eq!(
            base_ref(&r.git, &r.dir, &base).await,
            format!("origin/{base}")
        );
        // Against it the branch has nothing new, though it is ahead of the stale local branch.
        let (ahead_remote, _) = r
            .git
            .ahead_behind(&r.dir, &format!("origin/{base}"), "HEAD")
            .await
            .unwrap();
        let (ahead_local, _) = r.git.ahead_behind(&r.dir, &base, "HEAD").await.unwrap();
        assert_eq!((ahead_remote, ahead_local), (0, 1));
    }

    #[tokio::test]
    async fn uncommitted_and_branch_changes_are_told_apart() {
        let r = repo().await;
        let base = branch_name(&r.dir);
        sh(&r.dir, &["checkout", "-q", "-b", "feature"]);
        std::fs::write(r.dir.join("committed.txt"), "one\ntwo\n").unwrap();
        sh(&r.dir, &["add", "."]);
        sh(&r.dir, &["commit", "-q", "-m", "work"]);
        std::fs::write(r.dir.join("loose.txt"), "a\nb\nc\n").unwrap();

        let all = changed_files(&r.git, &r.dir, &base, ChangeScope::All)
            .await
            .unwrap();
        let paths: Vec<_> = all.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["committed.txt", "loose.txt"]);
        assert_eq!(all[0].status, ChangeStatus::Added);
        assert_eq!(all[0].added, 2);
        assert_eq!(all[1].added, 3, "an untracked file counts its lines");

        let loose = changed_files(&r.git, &r.dir, &base, ChangeScope::Uncommitted)
            .await
            .unwrap();
        assert_eq!(
            loose.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
            ["loose.txt"]
        );
    }

    #[tokio::test]
    async fn edits_deletes_and_renames_have_their_own_status() {
        let r = repo().await;
        let base = branch_name(&r.dir);
        std::fs::write(r.dir.join("keep.txt"), "1\n2\n3\n4\n5\n6\n").unwrap();
        std::fs::write(r.dir.join("drop.txt"), "bye\n").unwrap();
        std::fs::write(
            r.dir.join("move.txt"),
            "same content here\nand more\nlines of it\n",
        )
        .unwrap();
        sh(&r.dir, &["add", "."]);
        sh(&r.dir, &["commit", "-q", "-m", "files"]);
        sh(&r.dir, &["checkout", "-q", "-b", "feature"]);
        std::fs::write(r.dir.join("keep.txt"), "1\n2\nthree\n4\n5\n6\n").unwrap();
        std::fs::remove_file(r.dir.join("drop.txt")).unwrap();
        sh(&r.dir, &["mv", "move.txt", "moved.txt"]);

        let files = changed_files(&r.git, &r.dir, "HEAD", ChangeScope::Uncommitted)
            .await
            .unwrap();
        let by = |p: &str| files.iter().find(|f| f.path == p).unwrap();
        assert_eq!(by("keep.txt").status, ChangeStatus::Modified);
        assert_eq!((by("keep.txt").added, by("keep.txt").removed), (1, 1));
        assert_eq!(by("drop.txt").status, ChangeStatus::Deleted);
        assert_eq!(by("moved.txt").status, ChangeStatus::Renamed);
        assert_eq!(by("moved.txt").old_path.as_deref(), Some("move.txt"));
        let _ = base;
    }

    #[tokio::test]
    async fn both_sides_of_a_file_are_available() {
        let r = repo().await;
        std::fs::write(r.dir.join("a.ts"), "export const a = 1;\n").unwrap();
        sh(&r.dir, &["add", "."]);
        sh(&r.dir, &["commit", "-q", "-m", "a"]);
        std::fs::write(r.dir.join("a.ts"), "export const a = 2;\n").unwrap();
        std::fs::write(r.dir.join("new.ts"), "new\n").unwrap();

        let pair = file_pair(
            &r.git,
            &r.dir,
            "HEAD",
            ChangeScope::Uncommitted,
            "a.ts",
            None,
        )
        .await
        .unwrap();
        assert_eq!(pair.original.as_deref(), Some("export const a = 1;\n"));
        assert_eq!(pair.modified.as_deref(), Some("export const a = 2;\n"));
        assert_eq!(pair.language, "typescript");

        let added = file_pair(
            &r.git,
            &r.dir,
            "HEAD",
            ChangeScope::Uncommitted,
            "new.ts",
            None,
        )
        .await
        .unwrap();
        assert_eq!(added.original, None);
        assert_eq!(added.modified.as_deref(), Some("new\n"));

        std::fs::remove_file(r.dir.join("a.ts")).unwrap();
        let deleted = file_pair(
            &r.git,
            &r.dir,
            "HEAD",
            ChangeScope::Uncommitted,
            "a.ts",
            None,
        )
        .await
        .unwrap();
        assert_eq!(deleted.modified, None);
        assert_eq!(deleted.original.as_deref(), Some("export const a = 1;\n"));
    }

    #[tokio::test]
    async fn binary_and_huge_files_come_back_described() {
        let r = repo().await;
        std::fs::write(r.dir.join("blob.bin"), [0u8, 1, 2, 0, 255]).unwrap();
        let pair = file_pair(
            &r.git,
            &r.dir,
            "HEAD",
            ChangeScope::Uncommitted,
            "blob.bin",
            None,
        )
        .await
        .unwrap();
        assert!(pair.binary);
        assert!(pair.modified.is_none());

        let big = std::fs::File::create(r.dir.join("big.txt")).unwrap();
        big.set_len(TEXT_LIMIT + 1).unwrap();
        let pair = file_pair(
            &r.git,
            &r.dir,
            "HEAD",
            ChangeScope::Uncommitted,
            "big.txt",
            None,
        )
        .await
        .unwrap();
        assert!(pair.too_large);
    }

    #[tokio::test]
    async fn a_path_outside_the_workspace_is_refused() {
        let r = repo().await;
        let err = file_pair(
            &r.git,
            &r.dir,
            "HEAD",
            ChangeScope::Uncommitted,
            "../../etc/hosts",
            None,
        )
        .await
        .unwrap_err();
        assert_eq!(err.code(), "invalid");
    }
}
