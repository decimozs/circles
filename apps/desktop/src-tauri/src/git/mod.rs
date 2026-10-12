//! Git service. Shells out to the `git` CLI (never libgit2) with the resolved login
//! environment, so behaviour matches what agents and the user see in a terminal.

pub mod error;
pub mod parse;

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use tokio::process::Command;

use crate::env::ResolvedEnv;
pub use error::{classify, GitError, GitErrorKind};
pub use parse::{Commit, FileStat, StatusInfo, Worktree};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const NETWORK_TIMEOUT: Duration = Duration::from_secs(120);

pub type GitResult<T> = Result<T, GitError>;

struct Raw {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// A branch or ref taken from the user must never be read by git as an option (`--output=...`).
pub fn not_an_option(values: &[&str]) -> GitResult<()> {
    if let Some(bad) = values.iter().find(|v| v.starts_with('-')) {
        return Err(GitError::new(
            GitErrorKind::Other,
            format!("\"{bad}\" is not a valid branch name."),
        ));
    }
    Ok(())
}

#[derive(Clone)]
pub struct Git {
    bin: PathBuf,
    env: Arc<ResolvedEnv>,
}

fn s(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

impl Git {
    pub fn new(env: Arc<ResolvedEnv>) -> Self {
        let bin = env.which("git").unwrap_or_else(|| PathBuf::from("git"));
        Self { bin, env }
    }

    async fn exec(&self, cwd: Option<&Path>, args: &[&str], timeout: Duration) -> GitResult<Raw> {
        self.exec_env(cwd, args, &[], timeout).await
    }

    async fn exec_env(
        &self,
        cwd: Option<&Path>,
        args: &[&str],
        extra_env: &[(&str, &str)],
        timeout: Duration,
    ) -> GitResult<Raw> {
        let mut cmd = Command::new(&self.bin);
        self.env.apply(&mut cmd);
        for (key, value) in extra_env {
            cmd.env(key, value);
        }
        cmd.env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GCM_INTERACTIVE", "never")
            .env("LC_ALL", "C");
        if self.env.get("GIT_SSH_COMMAND").is_none() {
            cmd.env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes");
        }
        cmd.args(["-c", "core.quotepath=false"])
            .args(args)
            .stdin(Stdio::null())
            .kill_on_drop(true);
        if let Some(cwd) = cwd {
            cmd.current_dir(cwd);
        }

        let output = tokio::time::timeout(timeout, cmd.output())
            .await
            .map_err(|_| {
                GitError::new(
                    GitErrorKind::Timeout,
                    format!("git {} timed out after {}s", args.first().unwrap_or(&""), timeout.as_secs()),
                )
            })?
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    GitError::new(GitErrorKind::GitMissing, "git was not found. Install the Xcode Command Line Tools with `xcode-select --install`.")
                } else {
                    GitError::new(GitErrorKind::Other, format!("cannot run git: {e}"))
                }
            })?;

        Ok(Raw {
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }

    async fn run_with(
        &self,
        cwd: Option<&Path>,
        args: &[&str],
        timeout: Duration,
    ) -> GitResult<String> {
        let raw = self.exec(cwd, args, timeout).await?;
        if raw.code == Some(0) {
            Ok(raw.stdout)
        } else {
            Err(classify(&raw.stderr))
        }
    }

    async fn run(&self, cwd: &Path, args: &[&str]) -> GitResult<String> {
        self.run_with(Some(cwd), args, DEFAULT_TIMEOUT).await
    }

    /// The raw bytes of a stored file, which a text result would damage for a binary one.
    pub async fn blob_bytes(&self, cwd: &Path, sha: &str) -> GitResult<Vec<u8>> {
        let mut cmd = Command::new(&self.bin);
        self.env.apply(&mut cmd);
        cmd.args(["cat-file", "blob", sha])
            .current_dir(cwd)
            .stdin(Stdio::null())
            .kill_on_drop(true);
        let output = tokio::time::timeout(DEFAULT_TIMEOUT, cmd.output())
            .await
            .map_err(|_| GitError::new(GitErrorKind::Timeout, "git cat-file timed out"))?
            .map_err(|e| GitError::new(GitErrorKind::Other, format!("cannot run git: {e}")))?;
        if output.status.success() {
            Ok(output.stdout)
        } else {
            Err(classify(&String::from_utf8_lossy(&output.stderr)))
        }
    }

    /// Runs git with extra environment variables, such as a private `GIT_INDEX_FILE`.
    pub(crate) async fn run_env(
        &self,
        cwd: &Path,
        args: &[&str],
        extra_env: &[(&str, &str)],
    ) -> GitResult<String> {
        let raw = self
            .exec_env(Some(cwd), args, extra_env, Duration::from_secs(120))
            .await?;
        if raw.code == Some(0) {
            Ok(raw.stdout)
        } else {
            Err(classify(&raw.stderr))
        }
    }

    async fn run_trim(&self, cwd: &Path, args: &[&str]) -> GitResult<String> {
        Ok(self.run(cwd, args).await?.trim().to_string())
    }

    // Repository facts

    /// Absolute path of the working tree root containing `path`.
    pub async fn toplevel(&self, path: &Path) -> GitResult<PathBuf> {
        Ok(PathBuf::from(
            self.run_trim(path, &["rev-parse", "--show-toplevel"])
                .await?,
        ))
    }

    /// True when `path` is a linked worktree rather than a repository's main working tree.
    pub async fn is_linked_worktree(&self, path: &Path) -> GitResult<bool> {
        let out = self
            .run_trim(
                path,
                &[
                    "rev-parse",
                    "--path-format=absolute",
                    "--git-dir",
                    "--git-common-dir",
                ],
            )
            .await?;
        let mut lines = out.lines();
        let (git_dir, common) = (lines.next().unwrap_or(""), lines.next().unwrap_or(""));
        Ok(git_dir != common)
    }

    /// Main working tree of the repository `path` belongs to (resolves linked worktrees).
    pub async fn main_worktree(&self, path: &Path) -> GitResult<PathBuf> {
        let list = self.worktree_list(path).await?;
        list.into_iter()
            .next()
            .map(|w| PathBuf::from(w.path))
            .ok_or_else(|| {
                GitError::new(
                    GitErrorKind::NotARepo,
                    "This folder is not a git repository.",
                )
            })
    }

    pub async fn has_commits(&self, repo: &Path) -> GitResult<bool> {
        let raw = self
            .exec(
                Some(repo),
                &["rev-parse", "--verify", "--quiet", "HEAD"],
                DEFAULT_TIMEOUT,
            )
            .await?;
        Ok(raw.code == Some(0))
    }

    pub async fn rev_parse(&self, repo: &Path, rev: &str) -> GitResult<String> {
        self.run_trim(repo, &["rev-parse", "--verify", rev]).await
    }

    pub async fn current_branch(&self, repo: &Path) -> GitResult<Option<String>> {
        let raw = self
            .exec(
                Some(repo),
                &["symbolic-ref", "--short", "-q", "HEAD"],
                DEFAULT_TIMEOUT,
            )
            .await?;
        Ok((raw.code == Some(0)).then(|| raw.stdout.trim().to_string()))
    }

    pub async fn remote_url(&self, repo: &Path, remote: &str) -> GitResult<Option<String>> {
        let key = format!("remote.{remote}.url");
        let raw = self
            .exec(Some(repo), &["config", "--get", &key], DEFAULT_TIMEOUT)
            .await?;
        let url = raw.stdout.trim();
        Ok((raw.code == Some(0) && !url.is_empty()).then(|| url.to_string()))
    }

    async fn ref_exists(&self, repo: &Path, full_ref: &str) -> GitResult<bool> {
        let raw = self
            .exec(
                Some(repo),
                &["show-ref", "--verify", "--quiet", full_ref],
                DEFAULT_TIMEOUT,
            )
            .await?;
        Ok(raw.code == Some(0))
    }

    /// Detects the branch new workspaces should start from.
    /// Order: `origin/HEAD`, then `main`, then `master`, then the current branch, else `main`.
    pub async fn detect_default_branch(&self, repo: &Path) -> GitResult<String> {
        let raw = self
            .exec(
                Some(repo),
                &["symbolic-ref", "--short", "-q", "refs/remotes/origin/HEAD"],
                DEFAULT_TIMEOUT,
            )
            .await?;
        if raw.code == Some(0) {
            let name = raw.stdout.trim();
            if let Some(branch) = name.strip_prefix("origin/") {
                if !branch.is_empty() {
                    return Ok(branch.to_string());
                }
            }
        }
        for candidate in ["main", "master"] {
            if self
                .ref_exists(repo, &format!("refs/heads/{candidate}"))
                .await?
                || self
                    .ref_exists(repo, &format!("refs/remotes/origin/{candidate}"))
                    .await?
            {
                return Ok(candidate.to_string());
            }
        }
        if let Some(current) = self.current_branch(repo).await? {
            return Ok(current);
        }
        Ok("main".to_string())
    }

    /// True when `branch` exists locally or as `origin/<branch>`.
    pub async fn branch_resolvable(&self, repo: &Path, branch: &str) -> GitResult<bool> {
        Ok(self.branch_exists(repo, branch).await?
            || self
                .ref_exists(repo, &format!("refs/remotes/origin/{branch}"))
                .await?)
    }

    /// True when the last fetch left a copy of `branch` of `remote`, which is what `origin/main`
    /// names. Local only: nothing is asked of the network.
    pub async fn remote_tracking_exists(
        &self,
        repo: &Path,
        remote: &str,
        branch: &str,
    ) -> GitResult<bool> {
        self.ref_exists(repo, &format!("refs/remotes/{remote}/{branch}"))
            .await
    }

    // Network

    /// Runs a command that talks to the remote, as the GitHub account that can see the repository
    /// when `gh` holds several (a work and a personal one).
    async fn run_network(
        &self,
        repo: &Path,
        args: &[&str],
        timeout: Duration,
    ) -> GitResult<String> {
        let token = crate::github::account_token_for(&self.env, repo).await;
        let extra: Vec<(&str, &str)> = token.iter().map(|t| ("GH_TOKEN", t.as_str())).collect();
        let raw = self.exec_env(Some(repo), args, &extra, timeout).await?;
        if raw.code == Some(0) {
            Ok(raw.stdout)
        } else {
            Err(classify(&raw.stderr))
        }
    }

    pub async fn fetch(&self, repo: &Path, remote: &str, branch: Option<&str>) -> GitResult<()> {
        let mut args = vec!["fetch", "--prune", remote];
        not_an_option(&[remote, branch.unwrap_or("")])?;
        if let Some(branch) = branch {
            args.push(branch);
        }
        self.run_network(repo, &args, NETWORK_TIMEOUT)
            .await
            .map(|_| ())
    }

    // Branches

    pub async fn branch_exists(&self, repo: &Path, name: &str) -> GitResult<bool> {
        self.ref_exists(repo, &format!("refs/heads/{name}")).await
    }

    pub async fn branch_create(&self, repo: &Path, name: &str, start: &str) -> GitResult<()> {
        not_an_option(&[name, start])?;
        self.run(repo, &["branch", name, start]).await.map(|_| ())
    }

    pub async fn branch_rename(&self, repo: &Path, old: &str, new: &str) -> GitResult<()> {
        self.run(repo, &["branch", "-m", old, new])
            .await
            .map(|_| ())
    }

    pub async fn branch_delete(&self, repo: &Path, name: &str, force: bool) -> GitResult<()> {
        let flag = if force { "-D" } else { "-d" };
        self.run(repo, &["branch", flag, name]).await.map(|_| ())
    }

    // Worktrees

    pub async fn worktree_list(&self, repo: &Path) -> GitResult<Vec<Worktree>> {
        let out = self.run(repo, &["worktree", "list", "--porcelain"]).await?;
        Ok(parse::parse_worktrees(&out))
    }

    /// Creates `path` with a new branch `branch` starting at `start`.
    pub async fn worktree_add_new_branch(
        &self,
        repo: &Path,
        path: &Path,
        branch: &str,
        start: &str,
    ) -> GitResult<()> {
        not_an_option(&[branch, start])?;
        // `--no-track` keeps the new branch from silently tracking `origin/<base>`.
        self.run(
            repo,
            &[
                "worktree",
                "add",
                "--no-track",
                "-b",
                branch,
                &s(path),
                start,
            ],
        )
        .await
        .map(|_| ())
    }

    /// Creates `path` checking out an existing branch.
    pub async fn worktree_add_existing(
        &self,
        repo: &Path,
        path: &Path,
        branch: &str,
    ) -> GitResult<()> {
        not_an_option(&[branch])?;
        self.run(repo, &["worktree", "add", &s(path), branch])
            .await
            .map(|_| ())
    }

    /// Creates `path` with a new local branch `branch` that tracks `remote_ref` (for example
    /// `origin/feature`), so pushing and pulling work as usual.
    pub async fn worktree_add_tracking(
        &self,
        repo: &Path,
        path: &Path,
        branch: &str,
        remote_ref: &str,
    ) -> GitResult<()> {
        self.run(
            repo,
            &[
                "worktree",
                "add",
                "--track",
                "-b",
                branch,
                &s(path),
                remote_ref,
            ],
        )
        .await
        .map(|_| ())
    }

    pub async fn worktree_remove(&self, repo: &Path, path: &Path, force: bool) -> GitResult<()> {
        let path = s(path);
        let mut args = vec!["worktree", "remove"];
        if force {
            args.push("--force");
        }
        args.push(&path);
        self.run(repo, &args).await.map(|_| ())
    }

    pub async fn worktree_prune(&self, repo: &Path) -> GitResult<()> {
        self.run(repo, &["worktree", "prune"]).await.map(|_| ())
    }

    /// Fetches a pull request's head into the local branch `local` (force updated).
    pub async fn fetch_pull_head(&self, repo: &Path, number: u32, local: &str) -> GitResult<()> {
        let refspec = format!("+pull/{number}/head:refs/heads/{local}");
        self.run_network(repo, &["fetch", "origin", &refspec], NETWORK_TIMEOUT)
            .await
            .map(|_| ())
    }

    /// Local and `origin` branches, most recently committed first. Each name appears once.
    pub async fn branch_refs(&self, repo: &Path) -> GitResult<Vec<parse::BranchRef>> {
        let out = self
            .run(
                repo,
                &[
                    "for-each-ref",
                    "--sort=-committerdate",
                    "--format=%(refname)",
                    "refs/heads",
                    "refs/remotes/origin",
                ],
            )
            .await?;
        Ok(parse::parse_branch_refs(&out))
    }

    // Inspection

    pub async fn status(&self, cwd: &Path) -> GitResult<StatusInfo> {
        let out = self
            .run(
                cwd,
                &[
                    "status",
                    "--porcelain=v2",
                    "--branch",
                    "-z",
                    "--untracked-files=all",
                ],
            )
            .await?;
        Ok(parse::parse_status(&out))
    }

    /// Per file line counts between `base` and `head` (or the working tree when `head` is `None`).
    pub async fn diff_numstat(
        &self,
        cwd: &Path,
        base: &str,
        head: Option<&str>,
    ) -> GitResult<Vec<FileStat>> {
        let mut args = vec!["diff", "--numstat", "-z", "--find-renames", base];
        if let Some(head) = head {
            args.push(head);
        }
        let out = self.run(cwd, &args).await?;
        Ok(parse::parse_numstat(&out))
    }

    /// Pushes `branch` to `remote` and sets it as the upstream, with the user's own credentials.
    pub async fn push(&self, cwd: &Path, remote: &str, branch: &str) -> GitResult<()> {
        not_an_option(&[remote, branch])?;
        self.run_network(
            cwd,
            &["push", "--set-upstream", remote, branch],
            Duration::from_secs(120),
        )
        .await
        .map(|_| ())
    }

    /// Whether `branch` has been pushed to `remote`.
    pub async fn remote_branch_exists(
        &self,
        cwd: &Path,
        remote: &str,
        branch: &str,
    ) -> GitResult<bool> {
        let name = format!("refs/remotes/{remote}/{branch}");
        let raw = self
            .exec(
                Some(cwd),
                &["show-ref", "--verify", "--quiet", &name],
                DEFAULT_TIMEOUT,
            )
            .await?;
        Ok(raw.code == Some(0))
    }

    /// Whether merging `other` into `HEAD` would conflict, found without touching the working tree.
    /// Needs git 2.38 or newer. Older versions answer `false`, so the pull request's own
    /// mergeability is what shows.
    pub async fn merge_conflicts(&self, cwd: &Path, other: &str) -> GitResult<bool> {
        let raw = self
            .exec(
                Some(cwd),
                &["merge-tree", "--write-tree", "--no-messages", "HEAD", other],
                DEFAULT_TIMEOUT,
            )
            .await?;
        Ok(raw.code == Some(1))
    }

    /// Status letters per changed file between `base` and the working tree, as `git diff
    /// --name-status -z` prints them.
    pub async fn diff_name_status(&self, cwd: &Path, base: &str) -> GitResult<String> {
        self.run(
            cwd,
            &["diff", "--name-status", "-z", "--find-renames", base],
        )
        .await
    }

    /// The content of `path` at `rev`, or `None` when the file did not exist there.
    pub async fn show_blob(&self, cwd: &Path, rev: &str, path: &str) -> GitResult<Option<String>> {
        let spec = format!("{rev}:{path}");
        let raw = self
            .exec(Some(cwd), &["show", &spec], DEFAULT_TIMEOUT)
            .await?;
        Ok((raw.code == Some(0)).then_some(raw.stdout))
    }

    /// Unified patch, optionally limited to one path.
    pub async fn diff_patch(
        &self,
        cwd: &Path,
        base: &str,
        head: Option<&str>,
        path: Option<&str>,
    ) -> GitResult<String> {
        let mut args = vec![
            "diff",
            "--no-color",
            "--no-ext-diff",
            "--find-renames",
            base,
        ];
        if let Some(head) = head {
            args.push(head);
        }
        if let Some(path) = path {
            args.push("--");
            args.push(path);
        }
        self.run(cwd, &args).await
    }

    pub async fn log(&self, cwd: &Path, rev: &str, limit: u32) -> GitResult<Vec<Commit>> {
        let format = format!("--format={}", parse::LOG_FORMAT);
        let limit = format!("-n{limit}");
        let out = self.run(cwd, &["log", &format, &limit, rev]).await?;
        Ok(parse::parse_log(&out))
    }

    /// The commits reachable from `revs`, newest first with children before parents, stopping
    /// before `since` when it can be resolved. Each carries the branches and tags on it.
    pub async fn graph(
        &self,
        cwd: &Path,
        revs: &[String],
        since: Option<&str>,
        limit: u32,
    ) -> GitResult<Vec<parse::GraphCommit>> {
        not_an_option(&revs.iter().map(String::as_str).collect::<Vec<_>>())?;
        let format = format!("--format={}", parse::GRAPH_FORMAT);
        let limit = format!("-n{limit}");
        let mut args: Vec<String> = ["log", "--topo-order", "--decorate=short", &format, &limit]
            .iter()
            .map(|a| a.to_string())
            .collect();
        args.extend(revs.iter().cloned());
        if let Some(since) = since {
            // Stop a few commits before the point the branch left its base, for context. A
            // history that is not that long just goes on to the root.
            let mut with_stop = args.clone();
            with_stop.push(format!("^{since}~3"));
            let refs: Vec<&str> = with_stop.iter().map(String::as_str).collect();
            if let Ok(out) = self.run(cwd, &refs).await {
                return Ok(parse::parse_graph(&out));
            }
        }
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = self.run(cwd, &refs).await?;
        Ok(parse::parse_graph(&out))
    }

    pub async fn merge_base(&self, cwd: &Path, a: &str, b: &str) -> GitResult<String> {
        self.run_trim(cwd, &["merge-base", a, b]).await
    }

    /// Commits `head` has that `base` lacks (ahead), and the reverse (behind).
    pub async fn ahead_behind(&self, cwd: &Path, base: &str, head: &str) -> GitResult<(u32, u32)> {
        let range = format!("{base}...{head}");
        let out = self
            .run_trim(cwd, &["rev-list", "--left-right", "--count", &range])
            .await?;
        let mut parts = out.split_whitespace();
        let behind = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
        let ahead = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
        Ok((ahead, behind))
    }

    /// Ignored files and directories, with fully ignored directories collapsed to `dir/`.
    pub async fn ignored_entries(&self, repo: &Path) -> GitResult<Vec<String>> {
        let out = self
            .run(
                repo,
                &[
                    "ls-files",
                    "--others",
                    "--ignored",
                    "--exclude-standard",
                    "--directory",
                    "--no-empty-directory",
                    "-z",
                ],
            )
            .await?;
        Ok(out
            .split('\0')
            .filter(|e| !e.is_empty())
            .map(str::to_string)
            .collect())
    }

    pub async fn config_get(&self, repo: &Path, key: &str) -> GitResult<Option<String>> {
        let raw = self
            .exec(Some(repo), &["config", "--get", key], DEFAULT_TIMEOUT)
            .await?;
        let value = raw.stdout.trim();
        Ok((raw.code == Some(0) && !value.is_empty()).then(|| value.to_string()))
    }

    /// A value set in the repository's own config, not one that comes from a global file.
    pub async fn config_get_local(&self, repo: &Path, key: &str) -> GitResult<Option<String>> {
        let raw = self
            .exec(
                Some(repo),
                &["config", "--local", "--get", key],
                DEFAULT_TIMEOUT,
            )
            .await?;
        let value = raw.stdout.trim();
        Ok((raw.code == Some(0) && !value.is_empty()).then(|| value.to_string()))
    }

    /// Sets a value in the repository's own config, which every worktree of it shares.
    pub async fn config_set_local(&self, repo: &Path, key: &str, value: &str) -> GitResult<()> {
        self.run(repo, &["config", "--local", key, value])
            .await
            .map(|_| ())
    }

    /// Stages everything and commits. Used to keep work safe before archiving.
    pub async fn commit_all(&self, cwd: &Path, message: &str) -> GitResult<()> {
        self.run(cwd, &["add", "-A"]).await?;
        self.run(cwd, &["commit", "-m", message]).await.map(|_| ())
    }

    /// Unstages everything and leaves the files as they are.
    pub async fn unstage_all(&self, cwd: &Path) -> GitResult<()> {
        self.run(cwd, &["reset", "-q"]).await.map(|_| ())
    }

    /// Stages exactly these paths, additions, edits and deletions alike.
    pub async fn stage_paths(&self, cwd: &Path, paths: &[String]) -> GitResult<()> {
        let mut args = vec!["add", "-A", "--"];
        args.extend(paths.iter().map(String::as_str));
        self.run(cwd, &args).await.map(|_| ())
    }

    /// Commits what is staged.
    pub async fn commit_staged(&self, cwd: &Path, message: &str) -> GitResult<()> {
        self.run(cwd, &["commit", "-m", message]).await.map(|_| ())
    }

    /// Whether `name` is a valid branch name according to git.
    pub async fn is_valid_branch_name(&self, repo: &Path, name: &str) -> GitResult<bool> {
        let raw = self
            .exec(
                Some(repo),
                &["check-ref-format", "--branch", name],
                DEFAULT_TIMEOUT,
            )
            .await?;
        Ok(raw.code == Some(0))
    }

    /// The per-clone exclude file (`.git/info/exclude`) shared by every worktree of the repository.
    pub async fn info_exclude_path(&self, cwd: &Path) -> GitResult<PathBuf> {
        let out = self
            .run_trim(
                cwd,
                &[
                    "rev-parse",
                    "--path-format=absolute",
                    "--git-path",
                    "info/exclude",
                ],
            )
            .await?;
        Ok(PathBuf::from(out))
    }

    // Creating repositories

    /// Creates `dir` as a new repository with `main` and one initial commit containing a README.
    pub async fn init_with_commit(&self, dir: &Path, readme: &str) -> GitResult<()> {
        std::fs::create_dir_all(dir).map_err(|e| {
            GitError::new(
                GitErrorKind::Other,
                format!("cannot create {}: {e}", dir.display()),
            )
        })?;
        self.run(dir, &["init", "-b", "main"]).await?;
        std::fs::write(dir.join("README.md"), readme).map_err(|e| {
            GitError::new(GitErrorKind::Other, format!("cannot write README.md: {e}"))
        })?;
        self.run(dir, &["add", "-A"]).await?;
        self.run(dir, &["commit", "-m", "Initial commit"]).await?;
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod testutil {
    use super::*;

    /// A `Git` that ignores the developer's global and system git config.
    pub fn test_git(home: &Path) -> Git {
        let path = std::env::var("PATH").unwrap_or_default();
        let env = ResolvedEnv::for_test(&[
            ("PATH", path.as_str()),
            ("HOME", &s(home)),
            ("GIT_CONFIG_GLOBAL", "/dev/null"),
            ("GIT_CONFIG_SYSTEM", "/dev/null"),
            ("GIT_AUTHOR_NAME", "Test"),
            ("GIT_AUTHOR_EMAIL", "test@example.com"),
            ("GIT_COMMITTER_NAME", "Test"),
            ("GIT_COMMITTER_EMAIL", "test@example.com"),
        ]);
        Git::new(Arc::new(env))
    }

    /// Repository with one commit on `main`.
    pub async fn repo_with_commit(git: &Git, dir: &Path) {
        git.init_with_commit(dir, "# test\n").await.unwrap();
    }

    pub fn sh(dir: &Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_name_that_looks_like_an_option_is_refused() {
        assert!(not_an_option(&["main", "feature/x"]).is_ok());
        assert!(not_an_option(&["main", "--output=/tmp/x"]).is_err());
        assert!(not_an_option(&["-D"]).is_err());
    }

    use super::testutil::*;
    use super::*;

    struct Fixture {
        _tmp: tempfile::TempDir,
        root: PathBuf,
        git: Git,
    }

    fn fixture() -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let git = test_git(&root);
        Fixture {
            _tmp: tmp,
            root,
            git,
        }
    }

    async fn repo(f: &Fixture) -> PathBuf {
        let dir = f.root.join("repo");
        repo_with_commit(&f.git, &dir).await;
        dir
    }

    /// A repository with a bare `origin` whose default branch is `default`.
    async fn repo_with_origin(f: &Fixture, default: &str) -> PathBuf {
        let bare = f.root.join("origin.git");
        sh(&f.root, &["init", "--bare", "-b", default, "origin.git"]);
        let dir = f.root.join("repo");
        sh(&f.root, &["init", "-b", default, "repo"]);
        std::fs::write(dir.join("a.txt"), "a\n").unwrap();
        sh(&dir, &["add", "-A"]);
        sh(&dir, &["commit", "-m", "init"]);
        sh(&dir, &["remote", "add", "origin", &s(&bare)]);
        sh(&dir, &["push", "-u", "origin", default]);
        sh(&dir, &["remote", "set-head", "origin", default]);
        dir
    }

    #[tokio::test]
    async fn detects_default_branch_without_a_remote() {
        let f = fixture();
        let dir = repo(&f).await;
        assert_eq!(f.git.detect_default_branch(&dir).await.unwrap(), "main");
    }

    #[tokio::test]
    async fn detects_default_branch_from_origin_head() {
        let f = fixture();
        let dir = repo_with_origin(&f, "develop").await;
        assert_eq!(f.git.detect_default_branch(&dir).await.unwrap(), "develop");
    }

    #[tokio::test]
    async fn falls_back_to_master_then_current_branch() {
        let f = fixture();
        let dir = f.root.join("m");
        sh(&f.root, &["init", "-b", "master", "m"]);
        std::fs::write(dir.join("a"), "a").unwrap();
        sh(&dir, &["add", "-A"]);
        sh(&dir, &["commit", "-m", "x"]);
        assert_eq!(f.git.detect_default_branch(&dir).await.unwrap(), "master");

        let other = f.root.join("o");
        sh(&f.root, &["init", "-b", "trunk", "o"]);
        std::fs::write(other.join("a"), "a").unwrap();
        sh(&other, &["add", "-A"]);
        sh(&other, &["commit", "-m", "x"]);
        assert_eq!(f.git.detect_default_branch(&other).await.unwrap(), "trunk");
    }

    #[tokio::test]
    async fn empty_repository_has_no_commits() {
        let f = fixture();
        let dir = f.root.join("empty");
        sh(&f.root, &["init", "-b", "main", "empty"]);
        assert!(!f.git.has_commits(&dir).await.unwrap());
        assert_eq!(f.git.detect_default_branch(&dir).await.unwrap(), "main");
    }

    #[tokio::test]
    async fn not_a_repo_is_typed() {
        let f = fixture();
        let err = f.git.toplevel(&f.root).await.unwrap_err();
        assert_eq!(err.kind, GitErrorKind::NotARepo);
    }

    #[tokio::test]
    async fn fetch_without_a_remote_is_typed() {
        let f = fixture();
        let dir = repo(&f).await;
        let err = f.git.fetch(&dir, "origin", None).await.unwrap_err();
        assert_eq!(err.kind, GitErrorKind::NoRemote);
    }

    #[tokio::test]
    async fn fetch_works_with_a_remote() {
        let f = fixture();
        let dir = repo_with_origin(&f, "main").await;
        f.git.fetch(&dir, "origin", Some("main")).await.unwrap();
        assert!(f.git.remote_url(&dir, "origin").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn branch_lifecycle() {
        let f = fixture();
        let dir = repo(&f).await;
        assert!(!f.git.branch_exists(&dir, "feat").await.unwrap());
        f.git.branch_create(&dir, "feat", "main").await.unwrap();
        assert!(f.git.branch_exists(&dir, "feat").await.unwrap());
        assert_eq!(
            f.git
                .branch_create(&dir, "feat", "main")
                .await
                .unwrap_err()
                .kind,
            GitErrorKind::BranchExists
        );
        f.git.branch_rename(&dir, "feat", "feature").await.unwrap();
        assert!(f.git.branch_exists(&dir, "feature").await.unwrap());
        f.git.branch_delete(&dir, "feature", false).await.unwrap();
        assert!(!f.git.branch_exists(&dir, "feature").await.unwrap());
        assert_eq!(
            f.git
                .branch_create(&dir, "x", "no-such-ref")
                .await
                .unwrap_err()
                .kind,
            GitErrorKind::BranchNotFound
        );
    }

    #[tokio::test]
    async fn worktree_lifecycle_and_checked_out_error() {
        let f = fixture();
        let dir = repo(&f).await;
        let wt = f.root.join("wt-oslo");

        f.git
            .worktree_add_new_branch(&dir, &wt, "me/abc12", "main")
            .await
            .unwrap();
        assert!(wt.join("README.md").exists());
        assert!(f.git.is_linked_worktree(&wt).await.unwrap());
        assert!(!f.git.is_linked_worktree(&dir).await.unwrap());
        assert_eq!(f.git.main_worktree(&wt).await.unwrap(), dir);

        let list = f.git.worktree_list(&dir).await.unwrap();
        assert_eq!(list.len(), 2);
        assert!(list.iter().any(|w| w.branch.as_deref() == Some("me/abc12")));

        // The same branch cannot be checked out twice.
        let other = f.root.join("wt-lima");
        let err = f
            .git
            .worktree_add_existing(&dir, &other, "me/abc12")
            .await
            .unwrap_err();
        assert_eq!(err.kind, GitErrorKind::BranchCheckedOut);
        assert_eq!(err.path.as_deref(), Some(s(&wt).as_str()));

        // Existing path.
        let err = f
            .git
            .worktree_add_new_branch(&dir, &wt, "me/zzz", "main")
            .await
            .unwrap_err();
        assert_eq!(err.kind, GitErrorKind::PathExists);

        f.git.worktree_remove(&dir, &wt, false).await.unwrap();
        assert!(!wt.exists());
        assert_eq!(f.git.worktree_list(&dir).await.unwrap().len(), 1);
        // Removing the worktree keeps the branch.
        assert!(f.git.branch_exists(&dir, "me/abc12").await.unwrap());
    }

    #[tokio::test]
    async fn dirty_worktree_blocks_removal_until_forced() {
        let f = fixture();
        let dir = repo(&f).await;
        let wt = f.root.join("wt");
        f.git
            .worktree_add_new_branch(&dir, &wt, "b", "main")
            .await
            .unwrap();
        std::fs::write(wt.join("scratch.txt"), "work in progress").unwrap();

        let status = f.git.status(&wt).await.unwrap();
        assert!(status.is_dirty);
        assert_eq!(status.untracked, 1);

        let err = f.git.worktree_remove(&dir, &wt, false).await.unwrap_err();
        assert_eq!(err.kind, GitErrorKind::DirtyTree);
        assert!(wt.exists());

        f.git.worktree_remove(&dir, &wt, true).await.unwrap();
        assert!(!wt.exists());
    }

    #[tokio::test]
    async fn prune_clears_deleted_worktrees() {
        let f = fixture();
        let dir = repo(&f).await;
        let wt = f.root.join("wt");
        f.git
            .worktree_add_new_branch(&dir, &wt, "b", "main")
            .await
            .unwrap();
        std::fs::remove_dir_all(&wt).unwrap();
        let list = f.git.worktree_list(&dir).await.unwrap();
        assert!(list.iter().any(|w| w.prunable));
        f.git.worktree_prune(&dir).await.unwrap();
        assert_eq!(f.git.worktree_list(&dir).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn status_diff_and_log() {
        let f = fixture();
        let dir = repo(&f).await;
        let base = f.git.rev_parse(&dir, "HEAD").await.unwrap();

        std::fs::write(dir.join("README.md"), "# test\nmore\n").unwrap();
        std::fs::write(dir.join("new.txt"), "x\n").unwrap();
        let status = f.git.status(&dir).await.unwrap();
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert_eq!(status.unstaged, 1);
        assert_eq!(status.untracked, 1);

        let stats = f.git.diff_numstat(&dir, "HEAD", None).await.unwrap();
        assert_eq!(stats.len(), 1);
        assert_eq!((stats[0].path.as_str(), stats[0].added), ("README.md", 1));
        let patch = f
            .git
            .diff_patch(&dir, "HEAD", None, Some("README.md"))
            .await
            .unwrap();
        assert!(patch.contains("+more"));

        sh(&dir, &["add", "-A"]);
        sh(&dir, &["commit", "-m", "second"]);
        let log = f.git.log(&dir, "HEAD", 10).await.unwrap();
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].subject, "second");
        assert_eq!(f.git.merge_base(&dir, "HEAD", &base).await.unwrap(), base);
        assert_eq!(
            f.git.ahead_behind(&dir, &base, "HEAD").await.unwrap(),
            (1, 0)
        );
        assert_eq!(
            f.git.ahead_behind(&dir, "HEAD", &base).await.unwrap(),
            (0, 1)
        );
        assert!(!f.git.status(&dir).await.unwrap().is_dirty);
    }

    #[tokio::test]
    async fn worktree_branches_do_not_track_the_base() {
        let f = fixture();
        let dir = repo_with_origin(&f, "main").await;
        let wt = f.root.join("wt");
        f.git
            .worktree_add_new_branch(&dir, &wt, "me/abc12", "origin/main")
            .await
            .unwrap();
        let status = f.git.status(&wt).await.unwrap();
        assert_eq!(status.branch.as_deref(), Some("me/abc12"));
        assert_eq!(status.upstream, None);
    }

    #[tokio::test]
    async fn ignored_entries_collapse_directories() {
        let f = fixture();
        let dir = repo(&f).await;
        std::fs::write(dir.join(".gitignore"), ".env\nnode_modules/\n*.log\n").unwrap();
        sh(&dir, &["add", "-A"]);
        sh(&dir, &["commit", "-m", "ignore"]);
        std::fs::write(dir.join(".env"), "A=1").unwrap();
        std::fs::write(dir.join("debug.log"), "x").unwrap();
        std::fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
        std::fs::write(dir.join("node_modules/pkg/index.js"), "x").unwrap();
        let mut entries = f.git.ignored_entries(&dir).await.unwrap();
        entries.sort();
        assert_eq!(entries, vec![".env", "debug.log", "node_modules/"]);
    }

    #[tokio::test]
    async fn misc_helpers() {
        let f = fixture();
        let dir = repo(&f).await;
        assert!(f
            .git
            .is_valid_branch_name(&dir, "me/feature-1")
            .await
            .unwrap());
        assert!(!f.git.is_valid_branch_name(&dir, "bad..name").await.unwrap());
        assert!(!f.git.is_valid_branch_name(&dir, "has space").await.unwrap());
        assert_eq!(f.git.config_get(&dir, "user.nothing").await.unwrap(), None);
        let exclude = f.git.info_exclude_path(&dir).await.unwrap();
        assert!(exclude.ends_with("info/exclude"));

        std::fs::write(dir.join("w.txt"), "w").unwrap();
        f.git.commit_all(&dir, "wip").await.unwrap();
        assert!(!f.git.status(&dir).await.unwrap().is_dirty);
        assert_eq!(f.git.log(&dir, "HEAD", 1).await.unwrap()[0].subject, "wip");
    }

    #[tokio::test]
    async fn quick_start_without_identity_is_typed() {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let path = std::env::var("PATH").unwrap_or_default();
        let env = ResolvedEnv::for_test(&[
            ("PATH", path.as_str()),
            ("HOME", &s(&root)),
            ("GIT_CONFIG_GLOBAL", "/dev/null"),
            ("GIT_CONFIG_SYSTEM", "/dev/null"),
            // Without this git guesses an identity from the hostname on macOS.
            ("GIT_CONFIG_COUNT", "1"),
            ("GIT_CONFIG_KEY_0", "user.useConfigOnly"),
            ("GIT_CONFIG_VALUE_0", "true"),
        ]);
        let git = Git::new(Arc::new(env));
        let err = git
            .init_with_commit(&root.join("p"), "# p\n")
            .await
            .unwrap_err();
        assert_eq!(err.kind, GitErrorKind::IdentityMissing);
    }
}
