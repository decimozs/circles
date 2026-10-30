//! Getting a workspace's branch merged: pull request, checks, and reviews.

pub mod commits;
pub mod decide;
pub mod docs;
pub mod draft;
pub mod prompts;

use std::path::Path;
use std::sync::Arc;

use rusqlite::params;
use serde::Serialize;
use ts_rs::TS;

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::git::Git;
use crate::github::pr::{MergeMethod, PrState, PrStatus};
use crate::github::{Gh, GhError};
use crate::review::changes::base_ref;
use crate::workspace::Workspace;
use decide::{Blocker, Facts, NextAction};

/// What git says about the branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct GitState {
    pub branch: String,
    pub base: String,
    /// Commits the branch has that its base lacks, and the other way round.
    pub ahead: u32,
    pub behind: u32,
    /// Uncommitted changes, counted by kind.
    pub dirty: bool,
    pub staged: u32,
    pub unstaged: u32,
    pub untracked: u32,
    /// Files with unresolved conflicts in the working tree right now.
    pub conflicts: u32,
    /// Merging the base into the branch would conflict.
    pub conflicts_with_base: bool,
    /// The branch exists on the remote.
    pub pushed: bool,
}

/// A problem talking to GitHub, shown in place of the section it affects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct SectionError {
    /// `not_installed`, `not_authenticated`, `network`, `rate_limited`, `not_found`, `timeout`
    /// or `other`.
    pub kind: String,
    pub message: String,
    /// A command that fixes it, when there is one.
    pub fix: Option<String>,
}

impl From<GhError> for SectionError {
    fn from(e: GhError) -> Self {
        let kind = serde_json::to_value(e.kind)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_else(|| "other".into());
        Self {
            kind,
            message: e.message,
            fix: e.fix,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct CheckSummary {
    pub total: u32,
    pub passed: u32,
    pub failed: u32,
    pub pending: u32,
}

/// Everything the top bar and the Checks tab need to know to say what to do next.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Readiness {
    pub git: GitState,
    pub pr: Option<PrStatus>,
    pub checks: CheckSummary,
    pub unresolved_threads: u32,
    pub blockers: Vec<Blocker>,
    pub next_action: NextAction,
    /// Nothing holds the merge back.
    pub ready: bool,
    /// Set when GitHub could not be asked. The git part is still right.
    pub gh_error: Option<SectionError>,
}

#[derive(Clone)]
pub struct PrService {
    pub git: Git,
    pub gh: Gh,
    pub db: Arc<Db>,
}

impl PrService {
    pub async fn git_state(&self, ws: &Workspace, cwd: &Path) -> AppResult<GitState> {
        let status = self.git.status(cwd).await?;
        let base = base_ref(&self.git, cwd, &ws.base_branch).await;
        let (ahead, behind) = self
            .git
            .ahead_behind(cwd, &base, "HEAD")
            .await
            .unwrap_or((0, 0));
        let conflicts_with_base = if ahead > 0 || behind > 0 {
            self.git.merge_conflicts(cwd, &base).await.unwrap_or(false)
        } else {
            false
        };
        let pushed = self
            .git
            .remote_branch_exists(cwd, "origin", &ws.branch)
            .await
            .unwrap_or(false);
        Ok(GitState {
            branch: ws.branch.clone(),
            base: ws.base_branch.clone(),
            ahead,
            behind,
            dirty: status.is_dirty,
            staged: status.staged,
            unstaged: status.unstaged,
            untracked: status.untracked,
            conflicts: status.conflicts,
            conflicts_with_base,
            pushed,
        })
    }

    fn remember_pr(&self, ws: &Workspace, pr: &PrStatus) -> AppResult<()> {
        if ws.pr_number == Some(pr.number as i64) && ws.pr_url.as_deref() == Some(&pr.url) {
            return Ok(());
        }
        self.db.write(|c| {
            c.execute(
                "UPDATE workspaces SET pr_number = ?1, pr_url = ?2 WHERE id = ?3",
                params![pr.number as i64, pr.url, ws.id],
            )
        })?;
        Ok(())
    }

    /// Gathers the facts and decides. GitHub being unreachable is a state, not a failure: the
    /// answer then rests on git alone.
    pub async fn readiness(&self, ws: &Workspace, cwd: &Path) -> AppResult<Readiness> {
        let git = self.git_state(ws, cwd).await?;
        let mut facts = Facts {
            dirty: git.dirty,
            ahead: git.ahead,
            conflicts_with_base: git.conflicts_with_base,
            ..Facts::default()
        };
        let mut gh_error = None;

        match self.gh.pr_for_branch(cwd, &ws.branch).await {
            Ok(Some(pr)) => {
                self.remember_pr(ws, &pr)?;
                if pr.state == PrState::Open {
                    match self.gh.pr_checks(cwd, pr.number).await {
                        Ok(checks) => facts.checks = checks,
                        Err(e) => gh_error = Some(e.into()),
                    }
                    match self.gh.review_threads(cwd, pr.number).await {
                        Ok(threads) => {
                            facts.unresolved_threads =
                                threads.iter().filter(|t| !t.resolved).count() as u32;
                        }
                        Err(e) => gh_error = gh_error.or(Some(e.into())),
                    }
                }
                facts.pr = Some(pr);
            }
            Ok(None) => {}
            Err(e) => gh_error = Some(e.into()),
        }

        let checks = CheckSummary {
            total: facts.checks.len() as u32,
            passed: facts
                .checks
                .iter()
                .filter(|c| c.bucket == crate::github::pr::CheckBucket::Pass)
                .count() as u32,
            failed: decide::failing(&facts.checks),
            pending: decide::pending(&facts.checks),
        };
        let blockers = decide::blockers(&facts);
        // With GitHub out of reach there is no telling whether a pull request exists, so do not
        // suggest creating one.
        let next_action = if gh_error.is_some() && facts.pr.is_none() {
            NextAction::None
        } else {
            decide::next_action(&facts)
        };
        Ok(Readiness {
            git,
            checks,
            unresolved_threads: facts.unresolved_threads,
            next_action,
            ready: decide::ready_to_merge(&facts),
            blockers,
            pr: facts.pr,
            gh_error,
        })
    }

    /// Pushes the branch and opens a pull request for it.
    pub async fn create_pr(
        &self,
        ws: &Workspace,
        cwd: &Path,
        title: &str,
        body: &str,
        draft: bool,
    ) -> AppResult<PrStatus> {
        let title = title.trim();
        if title.is_empty() {
            return Err(AppError::Invalid("Give the pull request a title.".into()));
        }
        let body = prompts::without_attribution(body);

        // A pull request needs commits the base branch lacks. Check against the latest base, and
        // say what is missing before anything is pushed.
        let _ = self.git.fetch(cwd, "origin", Some(&ws.base_branch)).await;
        let base = base_ref(&self.git, cwd, &ws.base_branch).await;
        let (ahead, _) = self
            .git
            .ahead_behind(cwd, &base, "HEAD")
            .await
            .unwrap_or((1, 0));
        if ahead == 0 {
            let dirty = self
                .git
                .status(cwd)
                .await
                .map(|s| s.is_dirty)
                .unwrap_or(false);
            return Err(AppError::Invalid(if dirty {
                format!(
                    "Nothing is committed on this branch yet. Commit your changes, then create the pull request. It needs commits that {} does not have.",
                    ws.base_branch
                )
            } else {
                format!(
                    "This branch has no commits that {} does not already have.",
                    ws.base_branch
                )
            }));
        }
        self.git.push(cwd, "origin", &ws.branch).await?;
        self.gh
            .pr_create(cwd, title, &body, &ws.base_branch, &ws.branch, draft)
            .await?;
        let pr = self
            .gh
            .pr_for_branch(cwd, &ws.branch)
            .await?
            .ok_or_else(|| {
                AppError::Internal("the pull request was created but cannot be found".into())
            })?;
        self.remember_pr(ws, &pr)?;
        Ok(pr)
    }

    /// Merges the pull request. Blockers stop it unless the user chose to override them, and a
    /// draft cannot be merged at all.
    pub async fn merge(
        &self,
        ws: &Workspace,
        cwd: &Path,
        method: MergeMethod,
        override_blockers: bool,
    ) -> AppResult<PrStatus> {
        let readiness = self.readiness(ws, cwd).await?;
        let Some(pr) = readiness.pr.clone().filter(|p| p.state == PrState::Open) else {
            return Err(AppError::Invalid(
                "There is no open pull request to merge.".into(),
            ));
        };
        if let Some(hard) = readiness.blockers.iter().find(|b| !b.overridable) {
            return Err(AppError::Invalid(hard.message.clone()));
        }
        if !readiness.blockers.is_empty() && !override_blockers {
            let reasons: Vec<_> = readiness
                .blockers
                .iter()
                .map(|b| b.message.as_str())
                .collect();
            return Err(AppError::Invalid(format!(
                "Not ready to merge: {}",
                reasons.join(" ")
            )));
        }
        self.gh.pr_merge(cwd, pr.number, method).await?;
        let merged = self
            .gh
            .pr_for_branch(cwd, &ws.branch)
            .await?
            .unwrap_or(PrStatus {
                state: PrState::Merged,
                ..pr
            });
        Ok(merged)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::ResolvedEnv;
    use crate::git::testutil::{repo_with_commit, sh, test_git};
    use crate::workspace::state::WorkspaceState;
    use std::path::PathBuf;

    /// A `gh` that answers from files in `$FAKE_GH_DIR` and records what it was asked.
    const FAKE_GH: &str = r#"#!/bin/sh
echo "$@" >> "$FAKE_GH_DIR/calls.log"
case "$1 $2" in
  "pr view")
    if [ -f "$FAKE_GH_DIR/pr.json" ]; then cat "$FAKE_GH_DIR/pr.json"; else echo "no pull requests found for branch \"$3\"" >&2; exit 1; fi ;;
  "pr checks")
    if [ -f "$FAKE_GH_DIR/checks.json" ]; then cat "$FAKE_GH_DIR/checks.json"; exit 1; else echo "no checks reported on the 'x' branch" >&2; exit 1; fi ;;
  "repo view") echo '{"owner":{"login":"o"},"name":"r"}' ;;
  "api graphql")
    if [ -f "$FAKE_GH_DIR/offline" ]; then echo "dial tcp: lookup api.github.com: no such host" >&2; exit 1; fi
    if [ -f "$FAKE_GH_DIR/threads.json" ]; then cat "$FAKE_GH_DIR/threads.json"; else echo '{"data":{"repository":{"pullRequest":{"reviewThreads":{"nodes":[]}}}}}'; fi ;;
  "pr create")
    cp "$FAKE_GH_DIR/created.json" "$FAKE_GH_DIR/pr.json"
    echo "https://github.com/o/r/pull/5" ;;
  "pr merge")
    sed 's/"OPEN"/"MERGED"/' "$FAKE_GH_DIR/pr.json" > "$FAKE_GH_DIR/pr.tmp" && mv "$FAKE_GH_DIR/pr.tmp" "$FAKE_GH_DIR/pr.json" ;;
  *) echo "unexpected: $@" >&2; exit 2 ;;
esac
"#;

    const OPEN_PR: &str = r#"{"number":5,"url":"https://github.com/o/r/pull/5","title":"Add login","state":"OPEN","isDraft":false,"mergeable":"MERGEABLE","mergeStateStatus":"CLEAN","reviewDecision":"","headRefName":"feature","baseRefName":"main"}"#;

    struct Rig {
        _tmp: tempfile::TempDir,
        dir: PathBuf,
        gh_dir: PathBuf,
        service: PrService,
        ws: Workspace,
    }

    impl Rig {
        fn calls(&self) -> String {
            std::fs::read_to_string(self.gh_dir.join("calls.log")).unwrap_or_default()
        }

        fn put(&self, name: &str, text: &str) {
            std::fs::write(self.gh_dir.join(name), text).unwrap();
        }
    }

    async fn rig() -> Rig {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let git = test_git(&home);
        let dir = tmp.path().join("repo");
        std::fs::create_dir_all(&dir).unwrap();
        repo_with_commit(&git, &dir).await;
        let origin = tmp.path().join("origin.git");
        std::process::Command::new("git")
            .args(["init", "-q", "--bare"])
            .arg(&origin)
            .status()
            .unwrap();
        sh(&dir, &["remote", "add", "origin", origin.to_str().unwrap()]);
        sh(&dir, &["push", "-q", "origin", "main"]);
        sh(&dir, &["fetch", "-q", "origin"]);
        sh(&dir, &["checkout", "-q", "-b", "feature"]);
        std::fs::write(dir.join("login.txt"), "login\n").unwrap();
        sh(&dir, &["add", "."]);
        sh(&dir, &["commit", "-q", "-m", "Add login"]);

        let gh_dir = tmp.path().join("gh");
        std::fs::create_dir_all(&gh_dir).unwrap();
        let bin = tmp.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let script = bin.join("gh");
        std::fs::write(&script, FAKE_GH).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let path = format!("{}:/usr/bin:/bin", bin.display());
        let gh_env = ResolvedEnv::for_test(&[
            ("PATH", path.as_str()),
            ("FAKE_GH_DIR", gh_dir.to_str().unwrap()),
        ]);

        let db = Arc::new(Db::open(&tmp.path().join("t.db")).unwrap());
        db.write(|c| {
            c.execute_batch(
                "INSERT INTO projects VALUES ('p','n','/p','main',NULL,1,1);
                 INSERT INTO workspaces (id,project_id,city,branch,base_branch,path,state,created_at,updated_at)
                   VALUES ('w1','p','Oslo','feature','main','/w1','ready',1,1);",
            )
        })
        .unwrap();
        let ws = Workspace {
            id: "w1".into(),
            project_id: "p".into(),
            city: "Oslo".into(),
            title: None,
            branch: "feature".into(),
            base_branch: "main".into(),
            path: dir.display().to_string(),
            state: WorkspaceState::Ready,
            error: None,
            pinned: false,
            section_id: None,
            pr_number: None,
            pr_url: None,
            created_at: 1,
            updated_at: 1,
            last_opened_at: None,
            archived_at: None,
            pending_prompt: None,
            pending_model: None,
            pending_effort: None,
        };
        let service = PrService {
            git,
            gh: Gh::new(Arc::new(gh_env)),
            db,
        };
        Rig {
            _tmp: tmp,
            dir,
            gh_dir,
            service,
            ws,
        }
    }

    #[tokio::test]
    async fn a_branch_with_commits_and_no_pull_request_should_create_one() {
        let r = rig().await;
        let readiness = r.service.readiness(&r.ws, &r.dir).await.unwrap();
        assert_eq!(readiness.git.ahead, 1);
        assert!(!readiness.git.pushed);
        assert!(!readiness.git.dirty);
        assert!(readiness.pr.is_none());
        assert_eq!(readiness.next_action, NextAction::CreatePr);
        assert!(!readiness.ready);
        assert!(readiness.gh_error.is_none());
        assert_eq!(readiness.checks, CheckSummary::default());
    }

    #[tokio::test]
    async fn uncommitted_work_shows_in_the_git_state() {
        let r = rig().await;
        std::fs::write(r.dir.join("loose.txt"), "x").unwrap();
        let state = r.service.git_state(&r.ws, &r.dir).await.unwrap();
        assert!(state.dirty);
        assert_eq!(state.untracked, 1);
    }

    #[tokio::test]
    async fn creating_a_pull_request_pushes_the_branch_and_remembers_the_pull_request() {
        let r = rig().await;
        r.put("created.json", OPEN_PR);
        let pr = r
            .service
            .create_pr(&r.ws, &r.dir, "  Add login ", "Body text", false)
            .await
            .unwrap();
        assert_eq!(pr.number, 5);
        assert!(r
            .service
            .git
            .remote_branch_exists(&r.dir, "origin", "feature")
            .await
            .unwrap());
        let calls = r.calls();
        assert!(
            calls.contains(
                "pr create --title Add login --body Body text --base main --head feature"
            ),
            "{calls}"
        );
        let stored: (Option<i64>, Option<String>) = r
            .service
            .db
            .read(|c| {
                c.query_row(
                    "SELECT pr_number, pr_url FROM workspaces WHERE id = 'w1'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
            })
            .unwrap();
        assert_eq!(
            stored,
            (Some(5), Some("https://github.com/o/r/pull/5".into()))
        );
        let readiness = r.service.readiness(&r.ws, &r.dir).await.unwrap();
        assert!(readiness.git.pushed);
        assert_eq!(readiness.pr.unwrap().number, 5);
    }

    #[tokio::test]
    async fn credit_lines_never_reach_the_pull_request() {
        let r = rig().await;
        r.put("created.json", OPEN_PR);
        r.service
            .create_pr(
                &r.ws,
                &r.dir,
                "Add login",
                "Adds a form.\n\n🤖 Generated with [Claude Code](https://claude.com/claude-code)",
                false,
            )
            .await
            .unwrap();
        let calls = r.calls();
        assert!(calls.contains("--body Adds a form."), "{calls}");
        assert!(!calls.contains("Claude Code"), "{calls}");
    }

    #[tokio::test]
    async fn a_branch_without_commits_of_its_own_is_refused_before_anything_is_pushed() {
        let r = rig().await;
        sh(&r.dir, &["checkout", "-q", "-b", "empty", "main"]);
        let ws = Workspace {
            branch: "empty".into(),
            ..r.ws.clone()
        };

        let err = r
            .service
            .create_pr(&ws, &r.dir, "Title", "", false)
            .await
            .unwrap_err();
        assert_eq!(err.code(), "invalid");
        assert!(
            err.to_string()
                .contains("no commits that main does not already have"),
            "{err}"
        );

        // Uncommitted work is told apart: the fix is to commit it.
        std::fs::write(r.dir.join("loose.txt"), "x").unwrap();
        let err = r
            .service
            .create_pr(&ws, &r.dir, "Title", "", false)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Commit your changes"), "{err}");

        assert!(!r
            .service
            .git
            .remote_branch_exists(&r.dir, "origin", "empty")
            .await
            .unwrap());
        assert!(!r.calls().contains("pr create"), "gh was never asked");
    }

    #[tokio::test]
    async fn a_pull_request_needs_a_title() {
        let r = rig().await;
        let err = r
            .service
            .create_pr(&r.ws, &r.dir, "  ", "", false)
            .await
            .unwrap_err();
        assert_eq!(err.code(), "invalid");
        assert!(r.calls().is_empty(), "nothing was pushed or sent");
    }

    #[tokio::test]
    async fn a_failing_check_is_what_to_fix_next() {
        let r = rig().await;
        r.put("pr.json", OPEN_PR);
        r.put(
            "checks.json",
            r#"[{"name":"build","workflow":"CI","bucket":"fail","state":"FAILURE","link":"https://github.com/o/r/actions/runs/9/job/1"},
                {"name":"lint","bucket":"pass","state":"SUCCESS","link":""}]"#,
        );
        let readiness = r.service.readiness(&r.ws, &r.dir).await.unwrap();
        assert_eq!(readiness.next_action, NextAction::FixChecks);
        assert_eq!(
            readiness.checks,
            CheckSummary {
                total: 2,
                passed: 1,
                failed: 1,
                pending: 0
            }
        );
        assert_eq!(
            readiness.blockers[0].kind,
            decide::BlockerKind::ChecksFailing
        );
        assert!(!readiness.ready);
    }

    #[tokio::test]
    async fn unresolved_review_threads_are_counted() {
        let r = rig().await;
        r.put("pr.json", OPEN_PR);
        r.put(
            "threads.json",
            r#"{"data":{"repository":{"pullRequest":{"reviewThreads":{"nodes":[
                {"id":"T1","isResolved":false,"isOutdated":false,"path":"a","line":1,"comments":{"nodes":[]}},
                {"id":"T2","isResolved":true,"isOutdated":false,"path":"b","line":2,"comments":{"nodes":[]}}]}}}}}"#,
        );
        let readiness = r.service.readiness(&r.ws, &r.dir).await.unwrap();
        assert_eq!(readiness.unresolved_threads, 1);
        assert_eq!(readiness.next_action, NextAction::RespondToReview);
    }

    #[tokio::test]
    async fn a_clean_pull_request_is_ready_and_merging_asks_gh_to_squash_it() {
        let r = rig().await;
        r.put("pr.json", OPEN_PR);
        r.put(
            "checks.json",
            r#"[{"name":"build","bucket":"pass","state":"SUCCESS","link":""}]"#,
        );
        let readiness = r.service.readiness(&r.ws, &r.dir).await.unwrap();
        assert!(readiness.ready);
        assert_eq!(readiness.next_action, NextAction::Merge);

        let merged = r
            .service
            .merge(&r.ws, &r.dir, MergeMethod::Squash, false)
            .await
            .unwrap();
        assert_eq!(merged.state, PrState::Merged);
        assert!(r.calls().contains("pr merge 5 --squash"));
        let after = r.service.readiness(&r.ws, &r.dir).await.unwrap();
        assert_eq!(after.next_action, NextAction::Archive);
    }

    #[tokio::test]
    async fn an_override_merges_despite_failing_checks() {
        let r = rig().await;
        r.put("pr.json", OPEN_PR);
        r.put(
            "checks.json",
            r#"[{"name":"build","bucket":"fail","state":"FAILURE","link":""}]"#,
        );
        assert!(r
            .service
            .merge(&r.ws, &r.dir, MergeMethod::Squash, false)
            .await
            .is_err());
        r.service
            .merge(&r.ws, &r.dir, MergeMethod::Squash, true)
            .await
            .unwrap();
        assert!(r.calls().contains("pr merge 5 --squash"));
    }

    #[tokio::test]
    async fn a_draft_cannot_be_merged_even_with_an_override() {
        let r = rig().await;
        r.put(
            "pr.json",
            &OPEN_PR.replace(r#""isDraft":false"#, r#""isDraft":true"#),
        );
        let err = r
            .service
            .merge(&r.ws, &r.dir, MergeMethod::Squash, true)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("draft"));
        assert!(!r.calls().contains("pr merge"));
    }

    #[tokio::test]
    async fn merging_without_a_pull_request_is_refused() {
        let r = rig().await;
        let err = r
            .service
            .merge(&r.ws, &r.dir, MergeMethod::Squash, true)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("no open pull request"));
    }

    #[tokio::test]
    async fn github_being_unreachable_is_a_state_and_hides_the_create_suggestion() {
        let r = rig().await;
        r.put("pr.json", OPEN_PR);
        r.put("offline", "");
        let readiness = r.service.readiness(&r.ws, &r.dir).await.unwrap();
        let error = readiness.gh_error.expect("the problem is reported");
        assert_eq!(error.kind, "network");
        assert!(
            readiness.pr.is_some(),
            "what was found before the failure stays"
        );
        assert_eq!(readiness.git.ahead, 1, "git still answers");

        // Without a pull request on record, and no way to ask, nothing is suggested.
        let r2 = rig().await;
        let missing_gh = PrService {
            gh: Gh::new(Arc::new(ResolvedEnv::for_test(&[("PATH", "/nonexistent")]))),
            ..r2.service.clone()
        };
        let readiness = missing_gh.readiness(&r2.ws, &r2.dir).await.unwrap();
        assert_eq!(readiness.gh_error.unwrap().kind, "not_installed");
        assert_eq!(readiness.next_action, NextAction::None);
        assert!(readiness.git.ahead == 1);
    }
}
