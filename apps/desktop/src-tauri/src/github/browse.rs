//! Pull requests of a repository as a list to look through, for any repository folder, not only
//! the branch a workspace is on.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use ts_rs::TS;

use super::{avatar_url, bad_output, Gh, GhError, DEFAULT_TIMEOUT};

/// How many repositories are asked at once.
const AT_ONCE: usize = 4;

const LIST_FIELDS: &str = "number,title,author,url,createdAt,updatedAt,additions,deletions,changedFiles,isDraft,state,labels,reviewDecision,reviewRequests,statusCheckRollup,headRefName,baseRefName";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct PrLabel {
    pub name: String,
    /// Hex without `#`, as GitHub gives it.
    pub color: String,
}

/// How the checks of a pull request stand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct PrChecks {
    /// `passing`, `failing`, `pending` or `none`.
    pub state: String,
    pub passed: u32,
    pub total: u32,
}

/// One pull request in the list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct PrRow {
    pub project_id: String,
    pub project_name: String,
    /// `owner/name`.
    pub repo: String,
    pub number: u32,
    pub title: String,
    pub url: String,
    pub author: String,
    pub author_avatar: String,
    pub head_ref: String,
    pub base_ref: String,
    /// `open`, `merged` or `closed`.
    pub state: String,
    pub is_draft: bool,
    pub additions: u32,
    pub deletions: u32,
    pub changed_files: u32,
    pub created_at: String,
    pub updated_at: String,
    pub labels: Vec<PrLabel>,
    /// `approved`, `changes_requested`, `review_required` or `none`.
    pub review: String,
    pub requested_reviewers: Vec<String>,
    pub checks: PrChecks,
    /// The signed-in account opened it.
    pub by_me: bool,
    /// The signed-in account is asked to review it.
    pub review_requested: bool,
}

/// Which pull requests to list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum BrowseState {
    Open,
    Merged,
    Closed,
    All,
}

impl BrowseState {
    fn flag(self) -> &'static str {
        match self {
            BrowseState::Open => "open",
            BrowseState::Merged => "merged",
            BrowseState::Closed => "closed",
            BrowseState::All => "all",
        }
    }
}

/// A list that may come as `null`, which `gh` can do for a pull request with nothing to list.
fn list<'de, D, T>(d: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(d)?.unwrap_or_default())
}

#[derive(Deserialize)]
struct RawPerson {
    #[serde(default)]
    login: String,
    #[serde(default)]
    slug: String,
}

#[derive(Deserialize)]
struct RawLabel {
    name: String,
    #[serde(default)]
    color: String,
}

#[derive(Deserialize)]
struct RawRow {
    number: u32,
    #[serde(default)]
    title: String,
    author: Option<RawPerson>,
    #[serde(default)]
    url: String,
    #[serde(rename = "createdAt", default)]
    created_at: String,
    #[serde(rename = "updatedAt", default)]
    updated_at: String,
    #[serde(default)]
    additions: u32,
    #[serde(default)]
    deletions: u32,
    #[serde(rename = "changedFiles", default)]
    changed_files: u32,
    #[serde(rename = "isDraft", default)]
    is_draft: bool,
    #[serde(default)]
    state: String,
    #[serde(default, deserialize_with = "list")]
    labels: Vec<RawLabel>,
    #[serde(rename = "reviewDecision", default)]
    review_decision: String,
    #[serde(rename = "reviewRequests", default, deserialize_with = "list")]
    review_requests: Vec<RawPerson>,
    #[serde(rename = "statusCheckRollup", default, deserialize_with = "list")]
    rollup: Vec<serde_json::Value>,
    #[serde(rename = "headRefName", default)]
    head_ref: String,
    #[serde(rename = "baseRefName", default)]
    base_ref: String,
}

/// Whose pull requests these are, so a row knows its project and whether it concerns the reader.
pub struct Whose<'a> {
    pub project_id: &'a str,
    pub project_name: &'a str,
    /// The signed-in account. Empty when it is not known.
    pub me: &'a str,
}

/// `owner/name` from `https://github.com/owner/name/pull/12`.
pub fn repo_from_url(url: &str) -> String {
    let rest = url
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(url)
        .split_once('/')
        .map(|(_, path)| path)
        .unwrap_or("");
    let mut parts = rest.split('/');
    match (parts.next(), parts.next()) {
        (Some(owner), Some(name)) if !owner.is_empty() && !name.is_empty() => {
            format!("{owner}/{name}")
        }
        _ => String::new(),
    }
}

/// Counts the checks of a pull request. A check run has a `status` and a `conclusion`; a commit
/// status has a `state`. Skipped and neutral runs count as passed.
pub fn summarize_checks(rollup: &[serde_json::Value]) -> PrChecks {
    let (mut passed, mut failed, mut pending) = (0u32, 0u32, 0u32);
    for check in rollup {
        let conclusion = check["conclusion"].as_str().unwrap_or("");
        let status = check["status"].as_str().unwrap_or("");
        let state = check["state"].as_str().unwrap_or("");
        match (conclusion, status, state) {
            ("SUCCESS" | "NEUTRAL" | "SKIPPED", _, _) | (_, _, "SUCCESS") => passed += 1,
            (
                "FAILURE" | "TIMED_OUT" | "STARTUP_FAILURE" | "CANCELLED" | "ACTION_REQUIRED",
                _,
                _,
            )
            | (_, _, "FAILURE" | "ERROR") => failed += 1,
            _ => pending += 1,
        }
    }
    let total = passed + failed + pending;
    let state = if total == 0 {
        "none"
    } else if failed > 0 {
        "failing"
    } else if pending > 0 {
        "pending"
    } else {
        "passing"
    };
    PrChecks {
        state: state.into(),
        passed,
        total,
    }
}

/// Parses `gh pr list --json ...` for the repository of `whose`.
pub fn parse_rows(json: &str, whose: &Whose) -> Result<Vec<PrRow>, GhError> {
    let raw: Vec<RawRow> = serde_json::from_str(json).map_err(bad_output)?;
    Ok(raw
        .into_iter()
        .map(|r| {
            let author = r.author.map(|a| a.login).unwrap_or_default();
            let requested: Vec<String> = r
                .review_requests
                .into_iter()
                .map(|p| if p.login.is_empty() { p.slug } else { p.login })
                .filter(|name| !name.is_empty())
                .collect();
            let me = !whose.me.is_empty();
            PrRow {
                project_id: whose.project_id.into(),
                project_name: whose.project_name.into(),
                repo: repo_from_url(&r.url),
                number: r.number,
                title: r.title,
                url: r.url,
                author_avatar: avatar_url(&author),
                by_me: me && author.eq_ignore_ascii_case(whose.me),
                review_requested: me && requested.iter().any(|n| n.eq_ignore_ascii_case(whose.me)),
                author,
                head_ref: r.head_ref,
                base_ref: r.base_ref,
                state: r.state.to_lowercase(),
                is_draft: r.is_draft,
                additions: r.additions,
                deletions: r.deletions,
                changed_files: r.changed_files,
                created_at: r.created_at,
                updated_at: r.updated_at,
                labels: r
                    .labels
                    .into_iter()
                    .map(|l| PrLabel {
                        name: l.name,
                        color: l.color,
                    })
                    .collect(),
                review: match r.review_decision.as_str() {
                    "APPROVED" => "approved",
                    "CHANGES_REQUESTED" => "changes_requested",
                    "REVIEW_REQUIRED" => "review_required",
                    _ => "none",
                }
                .into(),
                requested_reviewers: requested,
                checks: summarize_checks(&r.rollup),
            }
        })
        .collect())
}

impl Gh {
    /// The pull requests of the repository in `repo`, newest activity first.
    pub async fn browse_prs(
        &self,
        repo: &Path,
        whose: &Whose<'_>,
        state: BrowseState,
        limit: u32,
    ) -> Result<Vec<PrRow>, GhError> {
        let limit = limit.to_string();
        let out = self
            .run_in(
                Some(repo),
                &[
                    "pr",
                    "list",
                    "--state",
                    state.flag(),
                    "--limit",
                    &limit,
                    "--json",
                    LIST_FIELDS,
                ],
                DEFAULT_TIMEOUT,
            )
            .await?;
        parse_rows(&out, whose)
    }
}

/// A repository to list, with the project it belongs to.
#[derive(Debug, Clone)]
pub struct RepoRef {
    pub project_id: String,
    pub project_name: String,
    pub path: PathBuf,
}

/// A repository that could not be listed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct BrowseFailure {
    pub project_id: String,
    pub project_name: String,
    pub message: String,
    /// The command that fixes it, when there is one, such as `gh auth login`.
    pub fix: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct PrList {
    /// Most recently updated first.
    pub rows: Vec<PrRow>,
    pub failures: Vec<BrowseFailure>,
}

/// The pull requests of several repositories, a few at a time. One that fails is reported and the
/// others still show.
pub async fn browse_all(gh: &Gh, repos: Vec<RepoRef>, state: BrowseState, limit: u32) -> PrList {
    let gate = Arc::new(Semaphore::new(AT_ONCE));
    let mut tasks = JoinSet::new();
    for repo in repos {
        let (gh, gate) = (gh.clone(), gate.clone());
        tasks.spawn(async move {
            let _slot = gate.acquire_owned().await;
            // Without the account name nothing can be said to be "mine", but the list is still good.
            let me = gh.current_user_in(&repo.path).await.unwrap_or_default();
            let whose = Whose {
                project_id: &repo.project_id,
                project_name: &repo.project_name,
                me: &me,
            };
            let result = gh.browse_prs(&repo.path, &whose, state, limit).await;
            (repo, result)
        });
    }
    let mut list = PrList::default();
    while let Some(done) = tasks.join_next().await {
        let Ok((repo, result)) = done else { continue };
        match result {
            Ok(rows) => list.rows.extend(rows),
            Err(e) => list.failures.push(BrowseFailure {
                project_id: repo.project_id,
                project_name: repo.project_name,
                message: e.message,
                fix: e.fix,
            }),
        }
    }
    list.rows.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then(a.repo.cmp(&b.repo))
            .then(b.number.cmp(&a.number))
    });
    list.failures
        .sort_by(|a, b| a.project_name.cmp(&b.project_name));
    list
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::ResolvedEnv;

    const ONE: &str = r#"[{
      "number": 879, "title": "STH-1766 : Bump nine dependencies",
      "author": {"login": "Alfa-Roselle", "is_bot": false},
      "url": "https://github.com/Anervea-AI/sethi_enterprise_backend/pull/879",
      "createdAt": "2026-10-20T10:00:00Z", "updatedAt": "2026-10-22T10:00:00Z",
      "additions": 137, "deletions": 10, "changedFiles": 2, "isDraft": false, "state": "OPEN",
      "labels": [{"name": "deps", "color": "0e8a16"}],
      "reviewDecision": "REVIEW_REQUIRED",
      "reviewRequests": [{"__typename": "User", "login": "decimozs"}, {"__typename": "Team", "slug": "backend"}],
      "statusCheckRollup": [
        {"name": "build", "status": "COMPLETED", "conclusion": "SUCCESS"},
        {"name": "lint", "status": "COMPLETED", "conclusion": "SKIPPED"},
        {"context": "ci/legacy", "state": "SUCCESS"}
      ],
      "headRefName": "fix/STH-1766", "baseRefName": "develop"
    }]"#;

    fn whose<'a>(me: &'a str) -> Whose<'a> {
        Whose {
            project_id: "p1",
            project_name: "backend",
            me,
        }
    }

    #[test]
    fn a_row_has_everything_the_list_shows() {
        let rows = parse_rows(ONE, &whose("decimozs")).unwrap();
        let r = &rows[0];
        assert_eq!(
            (r.number, r.repo.as_str()),
            (879, "Anervea-AI/sethi_enterprise_backend")
        );
        assert_eq!(r.author, "Alfa-Roselle");
        assert_eq!(
            r.author_avatar,
            "https://github.com/Alfa-Roselle.png?size=64"
        );
        assert_eq!((r.additions, r.deletions, r.changed_files), (137, 10, 2));
        assert_eq!(r.state, "open");
        assert_eq!(r.review, "review_required");
        assert_eq!(
            (r.head_ref.as_str(), r.base_ref.as_str()),
            ("fix/STH-1766", "develop")
        );
        assert_eq!(
            r.labels,
            [PrLabel {
                name: "deps".into(),
                color: "0e8a16".into()
            }]
        );
        assert_eq!(r.requested_reviewers, ["decimozs", "backend"]);
        assert_eq!(
            r.checks,
            PrChecks {
                state: "passing".into(),
                passed: 3,
                total: 3
            }
        );
        assert_eq!(
            (r.project_id.as_str(), r.project_name.as_str()),
            ("p1", "backend")
        );
    }

    #[test]
    fn it_knows_which_pull_requests_are_mine_or_wait_for_my_review() {
        let asked = parse_rows(ONE, &whose("DecimoZS")).unwrap();
        assert!(asked[0].review_requested && !asked[0].by_me);
        let mine = parse_rows(ONE, &whose("alfa-roselle")).unwrap();
        assert!(mine[0].by_me && !mine[0].review_requested);
        // With no account name, nothing is claimed.
        let nobody = parse_rows(ONE, &whose("")).unwrap();
        assert!(!nobody[0].by_me && !nobody[0].review_requested);
    }

    #[test]
    fn missing_fields_and_odd_values_do_not_break_a_row() {
        let rows = parse_rows(
            r#"[{"number": 5, "author": null, "state": "MERGED", "isDraft": true, "reviewDecision": "",
                 "labels": null, "reviewRequests": null, "statusCheckRollup": null}]"#,
            &whose("x"),
        )
        .unwrap();
        let r = &rows[0];
        assert_eq!(
            (r.author.as_str(), r.state.as_str(), r.review.as_str()),
            ("", "merged", "none")
        );
        assert!(r.is_draft && r.title.is_empty() && r.repo.is_empty());
        assert_eq!(r.checks.state, "none");
        assert!(parse_rows("not json", &whose("x")).is_err());
    }

    #[test]
    fn checks_are_failing_pending_or_passing_in_that_order_of_importance() {
        let check = |json: &str| {
            summarize_checks(&serde_json::from_str::<Vec<serde_json::Value>>(json).unwrap())
        };
        assert_eq!(check("[]").state, "none");
        assert_eq!(
            check(
                r#"[{"conclusion":"SUCCESS"},{"conclusion":"FAILURE"},{"status":"IN_PROGRESS","conclusion":""}]"#
            ),
            PrChecks {
                state: "failing".into(),
                passed: 1,
                total: 3
            }
        );
        assert_eq!(
            check(r#"[{"conclusion":"SUCCESS"},{"status":"QUEUED","conclusion":""},{"state":"PENDING"}]"#).state,
            "pending"
        );
        assert_eq!(check(r#"[{"state":"ERROR"}]"#).state, "failing");
        assert_eq!(check(r#"[{"conclusion":"CANCELLED"}]"#).state, "failing");
    }

    #[test]
    fn the_repository_comes_from_the_address() {
        assert_eq!(repo_from_url("https://github.com/o/r/pull/3"), "o/r");
        assert_eq!(repo_from_url("https://ghe.example.com/o/r/pull/3"), "o/r");
        assert_eq!(repo_from_url("nonsense"), "");
        assert_eq!(repo_from_url(""), "");
    }

    /// A `gh` that answers per repository folder: `good` lists one pull request, `bad` fails.
    fn fake_gh(dir: &Path) -> Gh {
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let script = bin.join("gh");
        let json = ONE.replace('\n', " ");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\ncase \"$1\" in\n  api) echo decimozs ;;\n  pr) case \"$PWD\" in\n    */bad) echo 'HTTP 404: Not Found' >&2; exit 1 ;;\n    *) echo '{json}' ;;\n  esac ;;\nesac\n"
            ),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let path = format!("{}:/usr/bin:/bin", bin.display());
        // A token in the environment means no account lookup through git.
        Gh::new(Arc::new(ResolvedEnv::for_test(&[
            ("PATH", &path),
            ("GH_TOKEN", "x"),
        ])))
    }

    #[tokio::test]
    async fn one_repository_failing_does_not_hide_the_others() {
        let tmp = tempfile::tempdir().unwrap();
        let gh = fake_gh(tmp.path());
        let repo = |name: &str| {
            let path = tmp.path().join(name);
            std::fs::create_dir_all(&path).unwrap();
            RepoRef {
                project_id: name.into(),
                project_name: name.into(),
                path,
            }
        };
        let list = browse_all(
            &gh,
            vec![repo("good"), repo("bad"), repo("other")],
            BrowseState::Open,
            50,
        )
        .await;
        assert_eq!(list.rows.len(), 2);
        assert!(list.rows.iter().all(|r| r.review_requested));
        assert_eq!(list.failures.len(), 1);
        assert_eq!(list.failures[0].project_name, "bad");
        assert!(
            list.failures[0]
                .message
                .to_lowercase()
                .contains("not found"),
            "{:?}",
            list.failures[0]
        );
    }

    #[tokio::test]
    async fn a_missing_gh_is_reported_for_every_repository() {
        let tmp = tempfile::tempdir().unwrap();
        let gh = Gh::new(Arc::new(ResolvedEnv::for_test(&[("PATH", "/nonexistent")])));
        let list = browse_all(
            &gh,
            vec![RepoRef {
                project_id: "a".into(),
                project_name: "a".into(),
                path: tmp.path().into(),
            }],
            BrowseState::All,
            50,
        )
        .await;
        assert!(list.rows.is_empty());
        assert_eq!(list.failures[0].fix.as_deref(), Some("brew install gh"));
    }

    /// Uses the real GitHub CLI on this repository:
    /// `cargo test --lib -- --ignored --nocapture real_gh_lists`
    #[tokio::test]
    #[ignore = "uses the real GitHub CLI and the network"]
    async fn real_gh_lists_the_pull_requests_of_this_repository() {
        let env = Arc::new(crate::env::resolve().await);
        let gh = Gh::new(env);
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
        let list = browse_all(
            &gh,
            vec![RepoRef {
                project_id: "p".into(),
                project_name: "circles".into(),
                path: folder,
            }],
            BrowseState::All,
            5,
        )
        .await;
        for r in &list.rows {
            println!(
                "#{} {} [{}] {} +{} -{} checks {:?} by_me {} review {}",
                r.number,
                r.title,
                r.state,
                r.repo,
                r.additions,
                r.deletions,
                r.checks,
                r.by_me,
                r.review
            );
        }
        println!("failures {:?}", list.failures);
        assert!(!list.rows.is_empty());
    }
}
