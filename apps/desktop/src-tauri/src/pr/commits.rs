//! Committing the uncommitted changes of a workspace as atomic commits. Claude only proposes how
//! to group the files. Circles runs git itself, so every commit is exactly what the plan says.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::prompts::{without_attribution, COMMITS_FENCE};
use crate::error::{AppError, AppResult};
use crate::git::Git;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PlannedCommit {
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct CommitInfo {
    pub sha: String,
    pub message: String,
    pub files: Vec<String>,
}

/// The commits in the last `circles-commits` block of a reply.
pub fn parse_plan(text: &str) -> Option<Vec<PlannedCommit>> {
    let mut found = None;
    for part in text.split(&format!("```{COMMITS_FENCE}")).skip(1) {
        let Some(body) = part.split("```").next() else {
            continue;
        };
        if let Ok(plan) = serde_json::from_str::<Vec<PlannedCommit>>(body.trim()) {
            found = Some(plan);
        }
    }
    found
}

fn first_line(message: &str) -> String {
    without_attribution(message)
}

/// Makes a plan cover every changed file exactly once. Files the plan does not know are dropped,
/// a file in two commits stays in the first, and files it left out go in a last commit.
pub fn normalize(plan: Vec<PlannedCommit>, all: &[String]) -> Vec<PlannedCommit> {
    let known: HashSet<&str> = all.iter().map(String::as_str).collect();
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for commit in plan {
        let files: Vec<String> = commit
            .files
            .into_iter()
            .filter(|f| known.contains(f.as_str()) && seen.insert(f.clone()))
            .collect();
        if files.is_empty() {
            continue;
        }
        let message = first_line(&commit.message);
        let message = if message.trim().is_empty() {
            format!("Update {}", files[0])
        } else {
            message
        };
        out.push(PlannedCommit { message, files });
    }
    let left: Vec<String> = all.iter().filter(|f| !seen.contains(*f)).cloned().collect();
    if !left.is_empty() {
        let message = if left.len() == 1 {
            format!("Update {}", left[0])
        } else {
            "Update remaining files".to_string()
        };
        out.push(PlannedCommit {
            message,
            files: left,
        });
    }
    out
}

/// What is uncommitted, as the planning step needs to see it.
#[derive(Debug, Clone, Default)]
pub struct Pending {
    /// Each changed path with a one letter status: `A`dded, `M`odified, `D`eleted, `R`enamed.
    pub files: Vec<(String, String)>,
    /// New path to old path, for renames.
    pub origins: HashMap<String, String>,
    /// The diff against HEAD, plus the start of every new file, which a diff does not show.
    pub diff: String,
}

/// How much of a new file is shown to the planner.
const PREVIEW_CHARS: usize = 1500;

pub async fn gather(git: &Git, cwd: &Path) -> AppResult<Pending> {
    let status = git.status(cwd).await?;
    let mut pending = Pending::default();
    let mut previews = String::new();
    for file in status
        .files
        .iter()
        .filter(|f| f.kind != crate::git::parse::FileKind::Ignored)
    {
        let letter = if file.kind == crate::git::parse::FileKind::Untracked {
            "A"
        } else if file.kind == crate::git::parse::FileKind::Renamed {
            "R"
        } else if file.index == "D" || file.worktree == "D" {
            "D"
        } else if file.index == "A" {
            "A"
        } else {
            "M"
        };
        pending.files.push((file.path.clone(), letter.to_string()));
        if let Some(old) = &file.orig_path {
            pending.origins.insert(file.path.clone(), old.clone());
        }
        if file.kind == crate::git::parse::FileKind::Untracked {
            if let Ok(bytes) = std::fs::read(cwd.join(&file.path)) {
                if crate::files::looks_like_text(&bytes) {
                    let text = String::from_utf8_lossy(&bytes);
                    let shown: String = text.chars().take(PREVIEW_CHARS).collect();
                    previews.push_str(&format!(
                        "\n--- new file {}\n{}\n",
                        file.path,
                        shown.trim_end()
                    ));
                }
            }
        }
    }
    pending.diff = git
        .diff_patch(cwd, "HEAD", None, None)
        .await
        .unwrap_or_default();
    pending.diff.push_str(&previews);
    pending.files.sort();
    Ok(pending)
}

/// Makes the commits one after another. A rename also stages the old path, so git records it as a
/// move. When a commit fails, the earlier ones stay and the error says how far it got.
pub async fn apply(
    git: &Git,
    cwd: &Path,
    plan: &[PlannedCommit],
    origins: &HashMap<String, String>,
) -> AppResult<Vec<CommitInfo>> {
    git.unstage_all(cwd).await?;
    let mut made = Vec::new();
    for (index, commit) in plan.iter().enumerate() {
        let mut paths = commit.files.clone();
        paths.extend(commit.files.iter().filter_map(|f| origins.get(f).cloned()));
        let attempt = async {
            git.stage_paths(cwd, &paths).await?;
            git.commit_staged(cwd, &commit.message).await
        }
        .await;
        if let Err(e) = attempt {
            // What was staged for the failed commit goes back to being plain changes.
            let _ = git.unstage_all(cwd).await;
            return Err(AppError::Invalid(format!(
                "Made {index} of {} commits, then git stopped: {e}",
                plan.len()
            )));
        }
        let sha = git.rev_parse(cwd, "HEAD").await.unwrap_or_default();
        made.push(CommitInfo {
            sha: sha.chars().take(7).collect(),
            message: commit.message.clone(),
            files: commit.files.clone(),
        });
    }
    Ok(made)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::testutil::{repo_with_commit, sh, test_git};

    fn commit(message: &str, files: &[&str]) -> PlannedCommit {
        PlannedCommit {
            message: message.into(),
            files: files.iter().map(|f| f.to_string()).collect(),
        }
    }

    fn all(files: &[&str]) -> Vec<String> {
        files.iter().map(|f| f.to_string()).collect()
    }

    #[test]
    fn a_plan_is_read_from_the_last_labelled_block() {
        let text = "Plan:\n```circles-commits\n[{\"message\":\"One\",\"files\":[\"a\"]}]\n```\n```circles-commits\n[{\"message\":\"Two\",\"files\":[\"a\",\"b\"]},{\"message\":\"Three\"}]\n```";
        let plan = parse_plan(text).unwrap();
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0], commit("Two", &["a", "b"]));
        assert!(plan[1].files.is_empty(), "a missing list reads as empty");
        assert!(parse_plan("nothing").is_none());
        assert!(parse_plan("```circles-commits\nnot json\n```").is_none());
    }

    #[test]
    fn a_plan_is_made_to_cover_every_file_once() {
        let plan = vec![
            commit("Add the feature", &["a.ts", "b.ts", "ghost.ts"]),
            commit("Tests", &["b.ts", "a.test.ts"]),
            commit("Nothing real", &["ghost.ts"]),
            commit("  ", &["c.ts"]),
        ];
        let result = normalize(
            plan,
            &all(&["a.ts", "b.ts", "a.test.ts", "c.ts", "left.ts", "left2.ts"]),
        );
        assert_eq!(
            result,
            vec![
                commit("Add the feature", &["a.ts", "b.ts"]),
                commit("Tests", &["a.test.ts"]),
                commit("Update c.ts", &["c.ts"]),
                commit("Update remaining files", &["left.ts", "left2.ts"]),
            ]
        );
    }

    #[test]
    fn a_single_leftover_is_named_and_credit_lines_are_removed() {
        let plan = vec![commit(
            "Add it\n\nCo-Authored-By: Claude <noreply@anthropic.com>",
            &["a.ts"],
        )];
        let result = normalize(plan, &all(&["a.ts", "b.ts"]));
        assert_eq!(result[0].message, "Add it");
        assert_eq!(result[1], commit("Update b.ts", &["b.ts"]));
        assert_eq!(
            normalize(vec![], &all(&["only.ts"])),
            vec![commit("Update only.ts", &["only.ts"])]
        );
        assert!(normalize(vec![], &[]).is_empty());
    }

    #[tokio::test]
    async fn pending_changes_are_listed_with_their_kind_and_a_look_at_new_files() {
        let r = repo().await;
        std::fs::write(r.dir.join("keep.txt"), "one\ntwo\nthree\nfour\nfive\n").unwrap();
        std::fs::write(r.dir.join("drop.txt"), "bye\n").unwrap();
        std::fs::write(
            r.dir.join("old.txt"),
            "some long enough content\nfor git to see a rename\nacross lines\n",
        )
        .unwrap();
        std::fs::write(r.dir.join(".gitignore"), "ignored.log\n").unwrap();
        sh(&r.dir, &["add", "."]);
        sh(&r.dir, &["commit", "-q", "-m", "setup"]);
        std::fs::write(r.dir.join("keep.txt"), "one\nTWO\nthree\nfour\nfive\n").unwrap();
        std::fs::remove_file(r.dir.join("drop.txt")).unwrap();
        sh(&r.dir, &["mv", "old.txt", "moved.txt"]);
        std::fs::write(r.dir.join("fresh.ts"), "export const fresh = 1;\n").unwrap();
        std::fs::write(r.dir.join("ignored.log"), "noise").unwrap();
        std::fs::write(r.dir.join("blob.bin"), [0u8, 1, 2]).unwrap();

        let pending = gather(&r.git, &r.dir).await.unwrap();
        let kinds: HashMap<_, _> = pending.files.iter().cloned().collect();
        assert_eq!(kinds["keep.txt"], "M");
        assert_eq!(kinds["drop.txt"], "D");
        assert_eq!(kinds["moved.txt"], "R");
        assert_eq!(kinds["fresh.ts"], "A");
        assert!(
            !kinds.contains_key("ignored.log"),
            "ignored files are not changes"
        );
        assert_eq!(pending.origins["moved.txt"], "old.txt");
        assert!(pending.diff.contains("+TWO"));
        assert!(pending
            .diff
            .contains("--- new file fresh.ts\nexport const fresh = 1;"));
        assert!(
            !pending.diff.contains("blob.bin\n\0"),
            "binary files are not shown"
        );
        assert!(pending.files.windows(2).all(|w| w[0] <= w[1]), "sorted");
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

    fn files_of(dir: &Path, rev: &str) -> Vec<String> {
        let out = std::process::Command::new("git")
            .args(["show", "--name-only", "--format=", rev])
            .current_dir(dir)
            .output()
            .unwrap();
        let mut files: Vec<String> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::to_string)
            .collect();
        files.sort();
        files
    }

    fn subjects(dir: &Path) -> Vec<String> {
        let out = std::process::Command::new("git")
            .args(["log", "--format=%s", "-n", "5"])
            .current_dir(dir)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn clean(dir: &Path) -> bool {
        let out = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(dir)
            .output()
            .unwrap();
        out.stdout.is_empty()
    }

    #[tokio::test]
    async fn each_planned_commit_has_exactly_its_files_and_nothing_is_left_over() {
        let r = repo().await;
        std::fs::write(r.dir.join("gone.txt"), "bye\n").unwrap();
        sh(&r.dir, &["add", "."]);
        sh(&r.dir, &["commit", "-q", "-m", "add gone"]);
        std::fs::write(r.dir.join("feature.ts"), "feature\n").unwrap();
        std::fs::write(r.dir.join("feature.test.ts"), "test\n").unwrap();
        std::fs::write(r.dir.join("docs.md"), "docs\n").unwrap();
        std::fs::remove_file(r.dir.join("gone.txt")).unwrap();
        // Something staged by hand that belongs to the docs commit.
        sh(&r.dir, &["add", "docs.md"]);

        let plan = normalize(
            vec![
                commit("Add the feature", &["feature.ts", "feature.test.ts"]),
                commit("Document it", &["docs.md"]),
            ],
            &all(&["feature.ts", "feature.test.ts", "docs.md", "gone.txt"]),
        );
        let made = apply(&r.git, &r.dir, &plan, &HashMap::new()).await.unwrap();
        assert_eq!(made.len(), 3);
        assert!(made.iter().all(|c| c.sha.len() == 7));

        assert_eq!(
            subjects(&r.dir)[..3],
            [
                "Update remaining files"
                    .to_string()
                    .replace("remaining files", "gone.txt"),
                "Document it".into(),
                "Add the feature".into()
            ]
        );
        assert_eq!(
            files_of(&r.dir, "HEAD~2"),
            ["feature.test.ts", "feature.ts"]
        );
        assert_eq!(files_of(&r.dir, "HEAD~1"), ["docs.md"]);
        assert_eq!(files_of(&r.dir, "HEAD"), ["gone.txt"]);
        assert!(clean(&r.dir), "everything was committed");
    }

    #[tokio::test]
    async fn a_rename_is_committed_as_a_move() {
        let r = repo().await;
        std::fs::write(
            r.dir.join("old.txt"),
            "some long enough content\nfor git to see a rename\nacross lines\n",
        )
        .unwrap();
        sh(&r.dir, &["add", "."]);
        sh(&r.dir, &["commit", "-q", "-m", "add old"]);
        sh(&r.dir, &["mv", "old.txt", "new.txt"]);
        let origins = HashMap::from([("new.txt".to_string(), "old.txt".to_string())]);
        let plan = vec![commit("Rename old to new", &["new.txt"])];
        apply(&r.git, &r.dir, &plan, &origins).await.unwrap();
        assert!(clean(&r.dir));
        let out = std::process::Command::new("git")
            .args(["show", "--name-status", "--format=", "-M", "HEAD"])
            .current_dir(&r.dir)
            .output()
            .unwrap();
        assert!(
            String::from_utf8_lossy(&out.stdout).starts_with('R'),
            "{}",
            String::from_utf8_lossy(&out.stdout)
        );
    }

    #[tokio::test]
    async fn when_git_refuses_a_commit_the_earlier_ones_stay_and_the_error_says_how_far_it_got() {
        let r = repo().await;
        let hook = r.dir.join(".git/hooks/commit-msg");
        std::fs::create_dir_all(hook.parent().unwrap()).unwrap();
        std::fs::write(
            &hook,
            "#!/bin/sh\ngrep -q FAIL \"$1\" && { echo 'message rejected' >&2; exit 1; }\nexit 0\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::fs::write(r.dir.join("a.txt"), "a\n").unwrap();
        std::fs::write(r.dir.join("b.txt"), "b\n").unwrap();
        let plan = vec![
            commit("Add a", &["a.txt"]),
            commit("FAIL this one", &["b.txt"]),
        ];
        let err = apply(&r.git, &r.dir, &plan, &HashMap::new())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Made 1 of 2 commits"), "{err}");
        assert_eq!(subjects(&r.dir)[0], "Add a");
        let out = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&r.dir)
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            "?? b.txt",
            "the rest is left as plain changes"
        );
    }

    /// Uses the real Claude Code and a little API credit:
    /// `cargo test --lib -- --ignored --nocapture real_claude_plans`
    #[tokio::test]
    #[ignore = "uses the real Claude Code and a little API credit"]
    async fn real_claude_plans_atomic_commits_that_apply_cleanly() {
        let r = repo().await;
        std::fs::write(
            r.dir.join("greeting.py"),
            "def greet(name):\n    return f\"Hello, {name}!\"\n",
        )
        .unwrap();
        std::fs::write(r.dir.join("test_greeting.py"), "from greeting import greet\n\ndef test_greet():\n    assert greet(\"Ann\") == \"Hello, Ann!\"\n").unwrap();
        std::fs::write(
            r.dir.join("CHANGELOG.md"),
            "# Changelog\n\n- Added a greeting function.\n",
        )
        .unwrap();
        let pending = gather(&r.git, &r.dir).await.unwrap();
        let prompt = crate::pr::prompts::commit_plan(
            &pending.files,
            &pending.diff,
            &crate::settings::Instructions::default(),
        );
        let env = crate::env::resolve().await;
        let answer = crate::pr::draft::run_once(&env, &r.dir, &prompt, "haiku")
            .await
            .unwrap();
        println!("{answer}");
        let plan = parse_plan(&answer).expect("a plan in the asked form");
        let paths: Vec<String> = pending.files.iter().map(|(p, _)| p.clone()).collect();
        let plan = normalize(plan, &paths);
        println!("{plan:?}");
        let made = apply(&r.git, &r.dir, &plan, &pending.origins)
            .await
            .unwrap();
        assert!(!made.is_empty());
        assert!(clean(&r.dir), "everything was committed");
        assert!(subjects(&r.dir)
            .iter()
            .all(|s| !s.to_lowercase().contains("claude")));
    }
}
