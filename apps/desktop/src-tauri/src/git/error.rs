//! Typed git failures. The frontend switches on `kind` instead of parsing messages.

use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum GitErrorKind {
    /// The git executable was not found.
    GitMissing,
    NotARepo,
    /// The repository has no commits yet, so nothing can be branched from.
    NoCommits,
    BranchCheckedOut,
    BranchExists,
    BranchNotFound,
    /// Uncommitted changes block the operation.
    DirtyTree,
    NoRemote,
    AuthFailed,
    PathExists,
    /// `user.name` or `user.email` is not configured.
    IdentityMissing,
    Timeout,
    Other,
}

#[derive(Debug, Clone)]
pub struct GitError {
    pub kind: GitErrorKind,
    /// Short sentence safe to show to the user.
    pub message: String,
    /// Raw stderr, scrubbed of credentials and truncated.
    pub stderr: String,
    /// Path named by the error, when there is one (for example the worktree holding a branch).
    pub path: Option<String>,
}

impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for GitError {}

impl GitError {
    pub fn new(kind: GitErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            stderr: String::new(),
            path: None,
        }
    }

    pub fn details(&self) -> serde_json::Value {
        serde_json::json!({
            "kind": self.kind,
            "path": self.path,
            "stderr": self.stderr,
        })
    }
}

/// Removes credentials embedded in URLs and caps the length.
pub fn sanitize_stderr(stderr: &str) -> String {
    let mut out = String::with_capacity(stderr.len());
    let mut rest = stderr;
    while let Some(idx) = rest.find("://") {
        let (head, tail) = rest.split_at(idx + 3);
        out.push_str(head);
        let authority_end = tail
            .find(|c: char| c == '/' || c.is_whitespace())
            .unwrap_or(tail.len());
        match tail[..authority_end].rfind('@') {
            Some(at) => {
                out.push_str("[redacted]@");
                rest = &tail[at + 1..];
            }
            None => rest = tail,
        }
    }
    out.push_str(rest);
    let trimmed = out.trim();
    if trimmed.chars().count() > 600 {
        let cut: String = trimmed.chars().take(600).collect();
        format!("{cut}...")
    } else {
        trimmed.to_string()
    }
}

/// Extracts the first single-quoted path after `marker`, as in
/// `fatal: 'feat' is already checked out at '/path'`.
fn quoted_after(text: &str, marker: &str) -> Option<String> {
    let rest = &text[text.find(marker)? + marker.len()..];
    let start = rest.find('\'')? + 1;
    let end = rest[start..].find('\'')? + start;
    Some(rest[start..end].to_string())
}

/// Maps git's stderr to a typed error. Matches on stable phrases from git's own messages.
pub fn classify(stderr: &str) -> GitError {
    let clean = sanitize_stderr(stderr);
    let lower = clean.to_lowercase();
    let has = |needle: &str| lower.contains(needle);

    let (kind, message, path) = if has("is already checked out at")
        || has("is already used by worktree at")
        || has("refusing to fetch into branch")
    {
        let path = quoted_after(&clean, "checked out at")
            .or_else(|| quoted_after(&clean, "used by worktree at"));
        (
            GitErrorKind::BranchCheckedOut,
            "That branch is already checked out in another worktree.".to_string(),
            path,
        )
    } else if has("not a git repository") {
        (
            GitErrorKind::NotARepo,
            "This folder is not a git repository.".to_string(),
            None,
        )
    } else if has("author identity unknown")
        || has("unable to auto-detect email")
        || has("empty ident name")
        || has("please tell me who you are")
    {
        (
            GitErrorKind::IdentityMissing,
            "Git needs your name and email. Run `git config --global user.name \"Your Name\"` and `git config --global user.email you@example.com`.".to_string(),
            None,
        )
    } else if has("authentication failed")
        || has("could not read username")
        || has("could not read password")
        || has("permission denied (publickey")
        || has("terminal prompts disabled")
        || has("host key verification failed")
        || has("repository not found")
    {
        (
            GitErrorKind::AuthFailed,
            "Git could not authenticate with the remote. Check your credentials or SSH key."
                .to_string(),
            None,
        )
    } else if has("no such remote")
        || has("does not appear to be a git repository")
        || has("no configured push destination")
        || has("could not read from remote repository")
    {
        (
            GitErrorKind::NoRemote,
            "This repository has no usable remote.".to_string(),
            None,
        )
    } else if has("a branch named") && has("already exists") {
        (
            GitErrorKind::BranchExists,
            "A branch with that name already exists.".to_string(),
            None,
        )
    } else if has("contains modified or untracked files")
        || has("would be overwritten")
        || has("please commit your changes or stash")
        || has("you have unstaged changes")
        || has("your index contains uncommitted changes")
    {
        (
            GitErrorKind::DirtyTree,
            "There are uncommitted changes. Commit or stash them first.".to_string(),
            None,
        )
    } else if has("invalid reference")
        || has("not a valid object name")
        || has("unknown revision")
        || has("not found.") && has("branch")
        || has("couldn't find remote ref")
        || has("bad revision")
    {
        (
            GitErrorKind::BranchNotFound,
            "That branch or revision does not exist.".to_string(),
            None,
        )
    } else if has("already exists") || has("missing but already registered worktree") {
        let path = quoted_after(&clean, "fatal:");
        (
            GitErrorKind::PathExists,
            "That path already exists.".to_string(),
            path,
        )
    } else if has("does not have any commits yet") || has("ambiguous argument 'head'") {
        (
            GitErrorKind::NoCommits,
            "The repository has no commits yet.".to_string(),
            None,
        )
    } else {
        let first = clean.lines().next().unwrap_or("git failed").trim();
        (
            GitErrorKind::Other,
            first.trim_start_matches("fatal: ").to_string(),
            None,
        )
    };

    GitError {
        kind,
        message,
        stderr: clean,
        path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_checked_out_names_the_worktree() {
        let e = classify(
            "fatal: 'feature' is already checked out at '/Users/x/circles/workspaces/p/oslo'",
        );
        assert_eq!(e.kind, GitErrorKind::BranchCheckedOut);
        assert_eq!(
            e.path.as_deref(),
            Some("/Users/x/circles/workspaces/p/oslo")
        );
        let e = classify("fatal: 'feature' is already used by worktree at '/tmp/w'");
        assert_eq!(e.kind, GitErrorKind::BranchCheckedOut);
        assert_eq!(e.path.as_deref(), Some("/tmp/w"));
    }

    #[test]
    fn classifies_common_failures() {
        let cases = [
            ("fatal: not a git repository (or any of the parent directories): .git", GitErrorKind::NotARepo),
            ("fatal: a branch named 'x' already exists", GitErrorKind::BranchExists),
            ("fatal: 'origin' does not appear to be a git repository", GitErrorKind::NoRemote),
            ("fatal: No such remote: 'origin'", GitErrorKind::NoRemote),
            ("fatal: Authentication failed for 'https://github.com/a/b.git/'", GitErrorKind::AuthFailed),
            ("fatal: could not read Username for 'https://github.com': terminal prompts disabled", GitErrorKind::AuthFailed),
            ("git@github.com: Permission denied (publickey).", GitErrorKind::AuthFailed),
            ("fatal: '/p' contains modified or untracked files, use --force to delete it", GitErrorKind::DirtyTree),
            ("error: Your local changes to the following files would be overwritten by checkout", GitErrorKind::DirtyTree),
            ("fatal: invalid reference: nope", GitErrorKind::BranchNotFound),
            ("fatal: not a valid object name: 'nope'", GitErrorKind::BranchNotFound),
            ("fatal: '/tmp/w' already exists", GitErrorKind::PathExists),
            ("Author identity unknown\n\n*** Please tell me who you are.", GitErrorKind::IdentityMissing),
            ("fatal: something strange", GitErrorKind::Other),
        ];
        for (stderr, expected) in cases {
            assert_eq!(classify(stderr).kind, expected, "for: {stderr}");
        }
    }

    #[test]
    fn other_errors_keep_the_first_line() {
        let e = classify("fatal: something strange\nmore detail");
        assert_eq!(e.message, "something strange");
    }

    #[test]
    fn credentials_in_urls_are_removed() {
        let s = sanitize_stderr(
            "fatal: unable to access 'https://user:ghp_secret@github.com/a/b.git/': 403",
        );
        assert!(!s.contains("ghp_secret"));
        assert!(s.contains("[redacted]@github.com"));
        let plain = sanitize_stderr("fatal: unable to access 'https://github.com/a/b.git/'");
        assert!(plain.contains("https://github.com/a/b.git/"));
    }

    #[test]
    fn long_stderr_is_truncated() {
        let s = sanitize_stderr(&"x".repeat(2000));
        assert!(s.chars().count() <= 603);
    }
}
