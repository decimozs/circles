//! One pull request in full: its description, reviews, comments, commits and checks.

use std::path::Path;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::browse::{to_row, PrRow, RawRow, Whose, LIST_FIELDS};
use super::{avatar_url, bad_output, Gh, GhError, DEFAULT_TIMEOUT};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PrReview {
    pub author: String,
    pub author_avatar: String,
    /// `approved`, `changes_requested`, `commented`, `dismissed` or `pending`.
    pub state: String,
    pub body: String,
    pub submitted_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PrComment {
    pub author: String,
    pub author_avatar: String,
    pub body: String,
    pub created_at: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PrCommit {
    pub sha: String,
    pub headline: String,
    pub body: String,
    pub author: String,
    pub author_avatar: String,
    pub authored_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PrCheckRun {
    pub name: String,
    /// The workflow it belongs to, when it has one.
    pub workflow: String,
    /// `passing`, `failing` or `pending`.
    pub state: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PrDetail {
    pub row: PrRow,
    /// The description, as Markdown.
    pub body: String,
    /// `mergeable`, `conflicting` or `unknown`.
    pub mergeable: String,
    /// GitHub's word for whether it can be merged now, lower case: `clean`, `blocked`, `behind`,
    /// `dirty`, `unstable`, `draft`, `has_hooks` or `unknown`.
    pub merge_state: String,
    pub head_sha: String,
    pub merged_by: Option<String>,
    pub merged_at: Option<String>,
    pub reviews: Vec<PrReview>,
    pub comments: Vec<PrComment>,
    pub commits: Vec<PrCommit>,
    pub check_runs: Vec<PrCheckRun>,
    /// When this was fetched from GitHub, in milliseconds.
    #[serde(default)]
    #[ts(type = "number | null")]
    pub fetched_at: Option<i64>,
    /// It is being fetched again in the background.
    #[serde(default)]
    pub refreshing: bool,
}

#[derive(Deserialize)]
struct Person {
    #[serde(default)]
    login: String,
    #[serde(default)]
    name: String,
}

fn name_of(p: Option<Person>) -> String {
    p.map(|p| if p.login.is_empty() { p.name } else { p.login })
        .unwrap_or_default()
}

fn list<'de, D, T>(d: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(d)?.unwrap_or_default())
}

#[derive(Deserialize)]
struct RawReview {
    author: Option<Person>,
    #[serde(default)]
    state: String,
    #[serde(default)]
    body: String,
    #[serde(rename = "submittedAt", default)]
    submitted_at: String,
}

#[derive(Deserialize)]
struct RawComment {
    author: Option<Person>,
    #[serde(default)]
    body: String,
    #[serde(rename = "createdAt", default)]
    created_at: String,
    #[serde(default)]
    url: String,
}

#[derive(Deserialize)]
struct RawCommit {
    oid: String,
    #[serde(rename = "messageHeadline", default)]
    headline: String,
    #[serde(rename = "messageBody", default)]
    body: String,
    #[serde(default, deserialize_with = "list")]
    authors: Vec<Person>,
    #[serde(rename = "authoredDate", default)]
    authored_at: String,
}

#[derive(Deserialize)]
struct RawExtra {
    #[serde(default)]
    body: String,
    #[serde(default)]
    mergeable: String,
    #[serde(rename = "mergeStateStatus", default)]
    merge_state: String,
    #[serde(rename = "headRefOid", default)]
    head_sha: String,
    #[serde(rename = "mergedBy")]
    merged_by: Option<Person>,
    #[serde(rename = "mergedAt")]
    merged_at: Option<String>,
    #[serde(default, deserialize_with = "list")]
    reviews: Vec<RawReview>,
    #[serde(default, deserialize_with = "list")]
    comments: Vec<RawComment>,
    #[serde(default, deserialize_with = "list")]
    commits: Vec<RawCommit>,
    #[serde(rename = "statusCheckRollup", default, deserialize_with = "list")]
    rollup: Vec<serde_json::Value>,
}

fn check_run(value: &serde_json::Value) -> Option<PrCheckRun> {
    let name = value["name"]
        .as_str()
        .or_else(|| value["context"].as_str())?;
    let (conclusion, status, state) = (
        value["conclusion"].as_str().unwrap_or(""),
        value["status"].as_str().unwrap_or(""),
        value["state"].as_str().unwrap_or(""),
    );
    let outcome = match (conclusion, status, state) {
        ("SUCCESS" | "NEUTRAL" | "SKIPPED", _, _) | (_, _, "SUCCESS") => "passing",
        ("FAILURE" | "TIMED_OUT" | "STARTUP_FAILURE" | "CANCELLED" | "ACTION_REQUIRED", _, _)
        | (_, _, "FAILURE" | "ERROR") => "failing",
        _ => "pending",
    };
    Some(PrCheckRun {
        name: name.into(),
        workflow: value["workflowName"].as_str().unwrap_or("").into(),
        state: outcome.into(),
        url: value["detailsUrl"]
            .as_str()
            .or_else(|| value["targetUrl"].as_str())
            .unwrap_or("")
            .into(),
    })
}

/// Parses `gh pr view --json ...` into a full pull request.
pub fn parse_detail(json: &str, whose: &Whose) -> Result<PrDetail, GhError> {
    let row: RawRow = serde_json::from_str(json).map_err(bad_output)?;
    let extra: RawExtra = serde_json::from_str(json).map_err(bad_output)?;
    let person = |p: Option<Person>| {
        let name = name_of(p);
        (avatar_url(&name), name)
    };
    Ok(PrDetail {
        row: to_row(row, whose),
        body: extra.body,
        mergeable: match extra.mergeable.as_str() {
            "MERGEABLE" => "mergeable",
            "CONFLICTING" => "conflicting",
            _ => "unknown",
        }
        .into(),
        merge_state: if extra.merge_state.is_empty() {
            "unknown".into()
        } else {
            extra.merge_state.to_lowercase()
        },
        head_sha: extra.head_sha,
        merged_by: extra
            .merged_by
            .map(|p| name_of(Some(p)))
            .filter(|n| !n.is_empty()),
        merged_at: extra.merged_at.filter(|t| !t.is_empty()),
        reviews: extra
            .reviews
            .into_iter()
            .map(|r| {
                let (author_avatar, author) = person(r.author);
                PrReview {
                    author,
                    author_avatar,
                    state: r.state.to_lowercase(),
                    body: r.body,
                    submitted_at: r.submitted_at,
                }
            })
            .collect(),
        comments: extra
            .comments
            .into_iter()
            .map(|c| {
                let (author_avatar, author) = person(c.author);
                PrComment {
                    author,
                    author_avatar,
                    body: c.body,
                    created_at: c.created_at,
                    url: c.url,
                }
            })
            .collect(),
        commits: extra
            .commits
            .into_iter()
            .map(|c| {
                let (author_avatar, author) = person(c.authors.into_iter().next());
                PrCommit {
                    sha: c.oid,
                    headline: c.headline,
                    body: c.body,
                    author,
                    author_avatar,
                    authored_at: c.authored_at,
                }
            })
            .collect(),
        check_runs: extra.rollup.iter().filter_map(check_run).collect(),
        fetched_at: None,
        refreshing: false,
    })
}

impl Gh {
    /// One pull request of the repository in `repo`, open or not.
    pub async fn browse_pr(
        &self,
        repo: &Path,
        whose: &Whose<'_>,
        number: u32,
    ) -> Result<PrDetail, GhError> {
        let number = number.to_string();
        let fields = format!(
            "{LIST_FIELDS},body,mergeable,mergeStateStatus,headRefOid,mergedBy,mergedAt,reviews,comments,commits"
        );
        let out = self
            .run_in(
                Some(repo),
                &["pr", "view", &number, "--json", &fields],
                DEFAULT_TIMEOUT,
            )
            .await?;
        parse_detail(&out, whose)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DETAIL: &str = r###"{
      "number": 27, "title": "fix: continue a chat", "author": {"login": "decimozs"},
      "url": "https://github.com/decimozs/circles/pull/27",
      "createdAt": "2026-10-30T04:00:00Z", "updatedAt": "2026-10-30T07:06:49Z",
      "additions": 10, "deletions": 2, "changedFiles": 3, "isDraft": false, "state": "MERGED",
      "labels": [], "reviewDecision": "", "reviewRequests": [],
      "headRefName": "fix/x", "baseRefName": "main",
      "body": "## Description\nA fix.",
      "mergeable": "UNKNOWN", "mergeStateStatus": "CLEAN", "headRefOid": "a1dc99e",
      "mergedBy": {"login": "decimozs", "name": "Marlon"}, "mergedAt": "2026-10-30T07:06:49Z",
      "reviews": [{"author": {"login": "ben"}, "state": "CHANGES_REQUESTED", "body": "Please split", "submittedAt": "2026-10-30T05:00:00Z"}],
      "comments": [{"author": {"login": "ana"}, "body": "Looks good", "createdAt": "2026-10-30T06:00:00Z", "url": "https://github.com/decimozs/circles/pull/27#issuecomment-1"}],
      "commits": [
        {"oid": "9ed64bb", "messageHeadline": "docs: first", "messageBody": "why", "authoredDate": "2026-10-30T04:34:01Z",
         "authors": [{"login": "decimozs", "name": "decimozs"}]},
        {"oid": "a1dc99e", "messageHeadline": "fix: second", "messageBody": "", "authoredDate": "2026-10-30T04:50:00Z", "authors": []}
      ],
      "statusCheckRollup": [
        {"__typename": "CheckRun", "name": "changes", "workflowName": "CI", "status": "COMPLETED", "conclusion": "SUCCESS", "detailsUrl": "https://github.com/x/actions/runs/1"},
        {"__typename": "CheckRun", "name": "tests", "workflowName": "CI", "status": "IN_PROGRESS", "conclusion": "", "detailsUrl": "https://github.com/x/actions/runs/2"},
        {"__typename": "StatusContext", "context": "ci/legacy", "state": "FAILURE", "targetUrl": "https://ci.example/1"}
      ]
    }"###;

    fn whose() -> Whose<'static> {
        Whose {
            project_id: "p",
            project_name: "circles",
            me: "decimozs",
        }
    }

    #[test]
    fn a_pull_request_in_full_has_its_text_reviews_comments_commits_and_checks() {
        let d = parse_detail(DETAIL, &whose()).unwrap();
        assert_eq!(d.row.number, 27);
        assert!(d.row.by_me);
        assert_eq!(d.body, "## Description\nA fix.");
        assert_eq!(
            (d.mergeable.as_str(), d.merge_state.as_str()),
            ("unknown", "clean")
        );
        assert_eq!(d.head_sha, "a1dc99e");
        assert_eq!(d.merged_by.as_deref(), Some("decimozs"));
        assert_eq!(d.merged_at.as_deref(), Some("2026-10-30T07:06:49Z"));
        assert_eq!(d.reviews[0].state, "changes_requested");
        assert_eq!(d.reviews[0].author, "ben");
        assert_eq!(d.comments[0].body, "Looks good");
        assert_eq!(d.commits.len(), 2);
        assert_eq!(
            (d.commits[0].sha.as_str(), d.commits[0].author.as_str()),
            ("9ed64bb", "decimozs")
        );
        // A commit with no author listed still shows.
        assert_eq!(d.commits[1].author, "");
        let states: Vec<_> = d
            .check_runs
            .iter()
            .map(|c| (c.name.as_str(), c.state.as_str()))
            .collect();
        assert_eq!(
            states,
            [
                ("changes", "passing"),
                ("tests", "pending"),
                ("ci/legacy", "failing")
            ]
        );
        assert_eq!(d.check_runs[0].workflow, "CI");
        assert_eq!(d.check_runs[2].url, "https://ci.example/1");
    }

    #[test]
    fn a_bare_pull_request_still_parses() {
        let d = parse_detail(
            r#"{"number": 1, "mergedBy": null, "mergedAt": null, "comments": null, "reviews": null, "commits": null}"#,
            &whose(),
        )
        .unwrap();
        assert!(
            d.body.is_empty()
                && d.reviews.is_empty()
                && d.comments.is_empty()
                && d.commits.is_empty()
        );
        assert_eq!(
            (d.mergeable.as_str(), d.merge_state.as_str()),
            ("unknown", "unknown")
        );
        assert_eq!((d.merged_by, d.merged_at), (None, None));
        assert!(parse_detail("not json", &whose()).is_err());
    }

    /// Uses the real GitHub CLI on pull request 27 of this repository:
    /// `cargo test --lib -- --ignored --nocapture real_gh_reads_a_pull`
    #[tokio::test]
    #[ignore = "uses the real GitHub CLI and the network"]
    async fn real_gh_reads_a_pull_request_in_full() {
        let gh = Gh::new(std::sync::Arc::new(crate::env::resolve().await));
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
        let me = gh.current_user_in(&folder).await.unwrap_or_default();
        let whose = Whose {
            project_id: "p",
            project_name: "circles",
            me: &me,
        };
        let d = gh.browse_pr(&folder, &whose, 27).await.unwrap();
        println!(
            "{} [{}] by {} merged by {:?}; {} commits, {} reviews, {} comments, {} checks; merge state {}; body {} chars",
            d.row.title, d.row.state, d.row.author, d.merged_by, d.commits.len(), d.reviews.len(),
            d.comments.len(), d.check_runs.len(), d.merge_state, d.body.len()
        );
        assert_eq!(d.row.number, 27);
        assert!(!d.commits.is_empty() && !d.body.is_empty());
        // What GitHub really said survives being kept and read back.
        let tmp = tempfile::tempdir().unwrap();
        let db = std::sync::Arc::new(crate::db::Db::open(&tmp.path().join("t.db")).unwrap());
        let cache = crate::cache::CacheStore::new(db);
        cache.put("pr-detail:p:27", &d).unwrap();
        let back = cache.get::<PrDetail>("pr-detail:p:27").unwrap().unwrap();
        assert_eq!(back.value, d);
        println!("kept {} bytes", cache.info().unwrap().1);
    }
}
