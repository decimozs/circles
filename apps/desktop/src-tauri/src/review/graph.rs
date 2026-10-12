//! The history of a workspace's branch next to its base, for the graph in the right pane.

use std::path::Path;

use serde::Serialize;
use ts_rs::TS;

use crate::error::AppResult;
use crate::git::parse::GraphCommit;
use crate::git::Git;
use crate::review::changes::base_ref;

/// How many commits the graph shows at most.
pub const LIMIT: u32 = 80;

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct GitGraph {
    /// Newest first, children before parents.
    pub commits: Vec<GraphCommit>,
    /// The branch checked out in the workspace, `None` when detached.
    pub branch: Option<String>,
    /// The base the branch is compared with, as git names it (`origin/main` or `main`).
    pub base: String,
    /// Commits the branch has that its base lacks.
    pub ahead: u32,
    /// Commits the base has that the branch lacks.
    pub behind: u32,
    /// More history exists than is shown.
    pub truncated: bool,
}

pub async fn graph(git: &Git, cwd: &Path, base_branch: &str) -> AppResult<GitGraph> {
    let branch = git.current_branch(cwd).await?;
    let base = base_ref(git, cwd, base_branch).await;

    let mut revs = vec!["HEAD".to_string()];
    if git.rev_parse(cwd, &base).await.is_ok() {
        revs.push(base.clone());
    }
    if let Some(branch) = &branch {
        let remote = format!("origin/{branch}");
        if git.rev_parse(cwd, &remote).await.is_ok() {
            revs.push(remote);
        }
    }
    let fork = git.merge_base(cwd, "HEAD", &base).await.ok();
    let mut commits = git.graph(cwd, &revs, fork.as_deref(), LIMIT + 1).await?;
    let truncated = commits.len() > LIMIT as usize;
    commits.truncate(LIMIT as usize);
    let (ahead, behind) = git.ahead_behind(cwd, &base, "HEAD").await.unwrap_or((0, 0));
    Ok(GitGraph {
        commits,
        branch,
        base,
        ahead,
        behind,
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::parse::parse_graph;
    use crate::git::testutil::{repo_with_commit, sh, test_git};

    #[test]
    fn decorations_become_branches_tags_and_remotes() {
        let text = "aaa\x1fbbb ccc\x1fAda\x1f100\x1fMerge it\x1fHEAD -> feature, origin/feature, tag: v1, main, origin/HEAD\x1e\nbbb\x1f\x1fBob\x1f50\x1fRoot\x1f\x1e";
        let commits = parse_graph(text);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].parents, ["bbb", "ccc"]);
        let kinds: Vec<_> = commits[0]
            .refs
            .iter()
            .map(|r| (r.name.as_str(), r.kind.as_str()))
            .collect();
        assert_eq!(
            kinds,
            [
                ("feature", "head"),
                ("origin/feature", "remote"),
                ("v1", "tag"),
                ("main", "branch")
            ]
        );
        assert!(commits[1].parents.is_empty());
        assert_eq!(commits[1].subject, "Root");
    }

    #[tokio::test]
    async fn a_branch_is_drawn_against_its_base_with_ahead_and_behind() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let git = test_git(&home);
        let dir = tmp.path().join("repo");
        std::fs::create_dir_all(&dir).unwrap();
        repo_with_commit(&git, &dir).await;
        let base = git.current_branch(&dir).await.unwrap().unwrap();

        sh(&dir, &["checkout", "-q", "-b", "feature"]);
        for n in 1..=2 {
            std::fs::write(dir.join(format!("f{n}.txt")), "x\n").unwrap();
            sh(&dir, &["add", "."]);
            sh(&dir, &["commit", "-q", "-m", &format!("feature {n}")]);
        }
        sh(&dir, &["checkout", "-q", &base]);
        std::fs::write(dir.join("m.txt"), "x\n").unwrap();
        sh(&dir, &["add", "."]);
        sh(&dir, &["commit", "-q", "-m", "on base"]);
        sh(&dir, &["checkout", "-q", "feature"]);

        let g = graph(&git, &dir, &base).await.unwrap();
        assert_eq!(g.branch.as_deref(), Some("feature"));
        assert_eq!((g.ahead, g.behind), (2, 1));
        assert!(!g.truncated);
        let subjects: Vec<_> = g.commits.iter().map(|c| c.subject.as_str()).collect();
        assert!(subjects.contains(&"feature 2") && subjects.contains(&"on base"));
        assert_eq!(
            subjects.len(),
            4,
            "two on the branch, one on base, the shared root"
        );
        // The branch's own tip carries the checked out marker. Which of the two tips comes first
        // is not defined when their commits fall in the same second, so it is looked up.
        let tip = g.commits.iter().find(|c| c.subject == "feature 2").unwrap();
        assert!(tip
            .refs
            .iter()
            .any(|r| r.kind == "head" && r.name == "feature"));
        // Children come before the commits they grew from.
        let pos = |sha: &str| g.commits.iter().position(|c| c.sha == sha).unwrap();
        for c in &g.commits {
            for p in &c.parents {
                if g.commits.iter().any(|x| &x.sha == p) {
                    assert!(pos(&c.sha) < pos(p));
                }
            }
        }
    }
}
