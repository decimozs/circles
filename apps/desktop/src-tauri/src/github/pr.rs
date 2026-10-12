//! Pull requests, checks, review threads and deployments through `gh`, with typed parsers.

use std::path::Path;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{bad_output, classify, Gh, GhError, GhErrorKind, DEFAULT_TIMEOUT};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PrState {
    Open,
    Closed,
    Merged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Mergeable {
    Clean,
    Conflicting,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct PrStatus {
    pub number: u32,
    pub url: String,
    pub title: String,
    pub state: PrState,
    pub is_draft: bool,
    pub mergeable: Mergeable,
    /// GitHub's `mergeStateStatus`, lower case: `clean`, `blocked`, `behind`, `dirty`, ...
    pub merge_state: String,
    /// `approved`, `changes_requested` or `review_required`, when the repository asks for reviews.
    pub review_decision: Option<String>,
    pub head_ref: String,
    pub base_ref: String,
}

pub const PR_STATUS_FIELDS: &str =
    "number,url,title,state,isDraft,mergeable,mergeStateStatus,reviewDecision,headRefName,baseRefName";

#[derive(Deserialize)]
struct RawStatus {
    number: u32,
    url: String,
    title: String,
    state: String,
    #[serde(rename = "isDraft", default)]
    is_draft: bool,
    #[serde(default)]
    mergeable: String,
    #[serde(rename = "mergeStateStatus", default)]
    merge_state_status: String,
    #[serde(rename = "reviewDecision", default)]
    review_decision: Option<String>,
    #[serde(rename = "headRefName", default)]
    head_ref_name: String,
    #[serde(rename = "baseRefName", default)]
    base_ref_name: String,
}

pub fn parse_pr_status(json: &str) -> Result<PrStatus, GhError> {
    let raw: RawStatus = serde_json::from_str(json).map_err(bad_output)?;
    Ok(PrStatus {
        number: raw.number,
        url: raw.url,
        title: raw.title,
        state: match raw.state.as_str() {
            "MERGED" => PrState::Merged,
            "CLOSED" => PrState::Closed,
            _ => PrState::Open,
        },
        is_draft: raw.is_draft,
        mergeable: match raw.mergeable.as_str() {
            "MERGEABLE" => Mergeable::Clean,
            "CONFLICTING" => Mergeable::Conflicting,
            _ => Mergeable::Unknown,
        },
        merge_state: raw.merge_state_status.to_lowercase(),
        review_decision: raw
            .review_decision
            .filter(|d| !d.is_empty())
            .map(|d| d.to_lowercase()),
        head_ref: raw.head_ref_name,
        base_ref: raw.base_ref_name,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CheckBucket {
    Pass,
    Fail,
    Pending,
    Skipping,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct Check {
    pub name: String,
    pub workflow: String,
    pub bucket: CheckBucket,
    /// The state GitHub reports, lower case, for example `success` or `in_progress`.
    pub state: String,
    pub description: String,
    pub link: String,
    /// The Actions run this check belongs to, when its link points at one.
    #[ts(type = "number | null")]
    pub run_id: Option<u64>,
}

#[derive(Deserialize)]
struct RawCheck {
    name: String,
    #[serde(default)]
    workflow: String,
    #[serde(default)]
    bucket: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    link: String,
}

/// The id of the Actions run in a check's link, such as `.../actions/runs/123/job/456`.
pub fn run_id_from_link(link: &str) -> Option<u64> {
    let rest = link.split("/actions/runs/").nth(1)?;
    rest.split(['/', '?', '#']).next()?.parse().ok()
}

pub fn parse_checks(json: &str) -> Result<Vec<Check>, GhError> {
    let raw: Vec<RawCheck> = serde_json::from_str(json).map_err(bad_output)?;
    Ok(raw
        .into_iter()
        .map(|c| Check {
            run_id: run_id_from_link(&c.link),
            bucket: match c.bucket.as_str() {
                "pass" => CheckBucket::Pass,
                "fail" => CheckBucket::Fail,
                "skipping" => CheckBucket::Skipping,
                "cancel" => CheckBucket::Cancel,
                _ => CheckBucket::Pending,
            },
            name: c.name,
            workflow: c.workflow,
            state: c.state.to_lowercase(),
            description: c.description,
            link: c.link,
        })
        .collect())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ThreadComment {
    pub author: String,
    pub body: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ReviewThread {
    /// GitHub's node id, needed to resolve the thread.
    pub id: String,
    pub path: String,
    pub line: Option<u32>,
    pub resolved: bool,
    pub outdated: bool,
    pub comments: Vec<ThreadComment>,
}

const THREADS_QUERY: &str = "query($owner: String!, $name: String!, $number: Int!) { repository(owner: $owner, name: $name) { pullRequest(number: $number) { reviewThreads(first: 100) { nodes { id isResolved isOutdated path line comments(first: 20) { nodes { body url author { login } } } } } } } }";

const RESOLVE_MUTATION: &str =
    "mutation($id: ID!) { resolveReviewThread(input: {threadId: $id}) { thread { isResolved } } }";
const UNRESOLVE_MUTATION: &str = "mutation($id: ID!) { unresolveReviewThread(input: {threadId: $id}) { thread { isResolved } } }";

pub fn parse_threads(json: &str) -> Result<Vec<ReviewThread>, GhError> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(bad_output)?;
    let nodes = value
        .pointer("/data/repository/pullRequest/reviewThreads/nodes")
        .and_then(|n| n.as_array())
        .ok_or_else(|| {
            GhError::new(
                GhErrorKind::Other,
                "unexpected gh output: no review threads",
                None,
            )
        })?;
    Ok(nodes
        .iter()
        .map(|node| ReviewThread {
            id: node["id"].as_str().unwrap_or_default().to_string(),
            path: node["path"].as_str().unwrap_or_default().to_string(),
            line: node["line"].as_u64().map(|l| l as u32),
            resolved: node["isResolved"].as_bool().unwrap_or(false),
            outdated: node["isOutdated"].as_bool().unwrap_or(false),
            comments: node
                .pointer("/comments/nodes")
                .and_then(|c| c.as_array())
                .map(|list| {
                    list.iter()
                        .map(|c| ThreadComment {
                            author: c
                                .pointer("/author/login")
                                .and_then(|a| a.as_str())
                                .unwrap_or("someone")
                                .to_string(),
                            body: c["body"].as_str().unwrap_or_default().to_string(),
                            url: c["url"].as_str().unwrap_or_default().to_string(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct Deployment {
    pub environment: String,
    /// `success`, `failure`, `in_progress`, `pending`, ... or `unknown` before any status.
    pub state: String,
    pub url: Option<String>,
    pub created_at: String,
}

pub fn parse_deployment_list(json: &str) -> Result<Vec<(u64, String, String)>, GhError> {
    let value: Vec<serde_json::Value> = serde_json::from_str(json).map_err(bad_output)?;
    Ok(value
        .iter()
        .filter_map(|d| {
            Some((
                d["id"].as_u64()?,
                d["environment"]
                    .as_str()
                    .unwrap_or("deployment")
                    .to_string(),
                d["created_at"].as_str().unwrap_or_default().to_string(),
            ))
        })
        .collect())
}

/// The newest status in `gh api .../deployments/<id>/statuses`.
pub fn parse_deployment_status(json: &str) -> Result<(String, Option<String>), GhError> {
    let value: Vec<serde_json::Value> = serde_json::from_str(json).map_err(bad_output)?;
    Ok(value
        .first()
        .map(|s| {
            let url = s["environment_url"]
                .as_str()
                .filter(|u| !u.is_empty())
                .or_else(|| s["log_url"].as_str().filter(|u| !u.is_empty()))
                .map(str::to_string);
            (s["state"].as_str().unwrap_or("unknown").to_string(), url)
        })
        .unwrap_or_else(|| ("unknown".to_string(), None)))
}

/// The URL `gh pr create` prints: its last line.
pub fn url_from_create_output(output: &str) -> Option<String> {
    output
        .lines()
        .rev()
        .map(str::trim)
        .find(|l| l.starts_with("https://"))
        .map(str::to_string)
}

/// The end of a long log, which is where the failure is.
pub fn tail_chars(text: &str, limit: usize) -> String {
    let count = text.chars().count();
    if count <= limit {
        return text.to_string();
    }
    let skipped = count - limit;
    let tail: String = text.chars().skip(skipped).collect();
    format!("… {skipped} characters earlier were left out …\n{tail}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeMethod {
    Squash,
    Merge,
    Rebase,
}

impl MergeMethod {
    pub fn parse(text: Option<&str>) -> Self {
        match text {
            Some("merge") => Self::Merge,
            Some("rebase") => Self::Rebase,
            _ => Self::Squash,
        }
    }

    fn flag(self) -> &'static str {
        match self {
            Self::Squash => "--squash",
            Self::Merge => "--merge",
            Self::Rebase => "--rebase",
        }
    }
}

impl Gh {
    /// The pull request of `branch`, or `None` when it has none.
    pub async fn pr_for_branch(
        &self,
        cwd: &Path,
        branch: &str,
    ) -> Result<Option<PrStatus>, GhError> {
        match self
            .run_in(
                Some(cwd),
                &["pr", "view", branch, "--json", PR_STATUS_FIELDS],
                DEFAULT_TIMEOUT,
            )
            .await
        {
            Ok(out) => parse_pr_status(&out).map(Some),
            Err(e) if e.message.to_lowercase().contains("no pull requests found") => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Checks of a pull request. `gh pr checks` exits non-zero while checks fail or wait, and still
    /// prints them, so the output counts whatever the exit code is.
    pub async fn pr_checks(&self, cwd: &Path, number: u32) -> Result<Vec<Check>, GhError> {
        let number = number.to_string();
        let (code, out, err) = self
            .run_raw(
                cwd,
                &[
                    "pr",
                    "checks",
                    &number,
                    "--json",
                    "name,workflow,bucket,state,description,link",
                ],
            )
            .await?;
        if out.trim_start().starts_with('[') {
            return parse_checks(&out);
        }
        if err.to_lowercase().contains("no checks reported") {
            return Ok(Vec::new());
        }
        if code == Some(0) {
            return parse_checks(&out);
        }
        Err(classify(&err))
    }

    /// `owner/name` of the repository in `cwd`.
    async fn repo_name(&self, cwd: &Path) -> Result<(String, String), GhError> {
        let out = self
            .run_in(
                Some(cwd),
                &["repo", "view", "--json", "owner,name"],
                DEFAULT_TIMEOUT,
            )
            .await?;
        let value: serde_json::Value = serde_json::from_str(&out).map_err(bad_output)?;
        let owner = value.pointer("/owner/login").and_then(|v| v.as_str());
        let name = value.get("name").and_then(|v| v.as_str());
        match (owner, name) {
            (Some(o), Some(n)) => Ok((o.to_string(), n.to_string())),
            _ => Err(GhError::new(
                GhErrorKind::Other,
                "unexpected gh output: no repository",
                None,
            )),
        }
    }

    pub async fn review_threads(
        &self,
        cwd: &Path,
        number: u32,
    ) -> Result<Vec<ReviewThread>, GhError> {
        let (owner, name) = self.repo_name(cwd).await?;
        let query = format!("query={THREADS_QUERY}");
        let owner = format!("owner={owner}");
        let name = format!("name={name}");
        let number = format!("number={number}");
        let out = self
            .run_in(
                Some(cwd),
                &[
                    "api", "graphql", "-f", &query, "-f", &owner, "-f", &name, "-F", &number,
                ],
                DEFAULT_TIMEOUT,
            )
            .await?;
        parse_threads(&out)
    }

    pub async fn set_thread_resolved(
        &self,
        cwd: &Path,
        thread_id: &str,
        resolved: bool,
    ) -> Result<(), GhError> {
        let query = format!(
            "query={}",
            if resolved {
                RESOLVE_MUTATION
            } else {
                UNRESOLVE_MUTATION
            }
        );
        let id = format!("id={thread_id}");
        self.run_in(
            Some(cwd),
            &["api", "graphql", "-f", &query, "-f", &id],
            DEFAULT_TIMEOUT,
        )
        .await
        .map(|_| ())
    }

    /// Recent deployments of a branch, each with its newest status.
    pub async fn deployments(&self, cwd: &Path, branch: &str) -> Result<Vec<Deployment>, GhError> {
        let path = format!("repos/{{owner}}/{{repo}}/deployments?ref={branch}&per_page=5");
        let out = self
            .run_in(Some(cwd), &["api", &path], DEFAULT_TIMEOUT)
            .await?;
        let mut deployments = Vec::new();
        for (id, environment, created_at) in parse_deployment_list(&out)? {
            let path = format!("repos/{{owner}}/{{repo}}/deployments/{id}/statuses?per_page=1");
            let statuses = self
                .run_in(Some(cwd), &["api", &path], DEFAULT_TIMEOUT)
                .await?;
            let (state, url) = parse_deployment_status(&statuses)?;
            deployments.push(Deployment {
                environment,
                state,
                url,
                created_at,
            });
        }
        Ok(deployments)
    }

    /// The log of the failed steps of an Actions run.
    pub async fn failed_log(&self, cwd: &Path, run_id: u64) -> Result<String, GhError> {
        let id = run_id.to_string();
        self.run_in(
            Some(cwd),
            &["run", "view", &id, "--log-failed"],
            std::time::Duration::from_secs(60),
        )
        .await
    }

    /// Creates a pull request for `head` and returns its URL.
    pub async fn pr_create(
        &self,
        cwd: &Path,
        title: &str,
        body: &str,
        base: &str,
        head: &str,
        draft: bool,
    ) -> Result<String, GhError> {
        let mut args = vec![
            "pr", "create", "--title", title, "--body", body, "--base", base, "--head", head,
        ];
        if draft {
            args.push("--draft");
        }
        let out = self
            .run_in(Some(cwd), &args, std::time::Duration::from_secs(60))
            .await?;
        url_from_create_output(&out).ok_or_else(|| {
            GhError::new(
                GhErrorKind::Other,
                "gh did not say where the pull request is",
                None,
            )
        })
    }

    pub async fn pr_merge(
        &self,
        cwd: &Path,
        number: u32,
        method: MergeMethod,
    ) -> Result<(), GhError> {
        let number = number.to_string();
        self.run_in(
            Some(cwd),
            &["pr", "merge", &number, method.flag()],
            std::time::Duration::from_secs(60),
        )
        .await
        .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pull_request_status_is_read() {
        let status = parse_pr_status(
            r#"{"number":42,"url":"https://github.com/o/r/pull/42","title":"Add login","state":"OPEN",
                "isDraft":false,"mergeable":"CONFLICTING","mergeStateStatus":"DIRTY",
                "reviewDecision":"CHANGES_REQUESTED","headRefName":"me/login","baseRefName":"main"}"#,
        )
        .unwrap();
        assert_eq!(status.number, 42);
        assert_eq!(status.state, PrState::Open);
        assert_eq!(status.mergeable, Mergeable::Conflicting);
        assert_eq!(status.merge_state, "dirty");
        assert_eq!(status.review_decision.as_deref(), Some("changes_requested"));
        assert_eq!(status.head_ref, "me/login");
    }

    #[test]
    fn merged_and_closed_states_and_a_missing_review_decision() {
        let merged = parse_pr_status(
            r#"{"number":1,"url":"u","title":"t","state":"MERGED","mergeable":"UNKNOWN","reviewDecision":""}"#,
        )
        .unwrap();
        assert_eq!(merged.state, PrState::Merged);
        assert_eq!(merged.mergeable, Mergeable::Unknown);
        assert_eq!(merged.review_decision, None);
        let closed =
            parse_pr_status(r#"{"number":1,"url":"u","title":"t","state":"CLOSED"}"#).unwrap();
        assert_eq!(closed.state, PrState::Closed);
        assert!(parse_pr_status("not json").is_err());
    }

    #[test]
    fn checks_are_read_with_their_run() {
        let checks = parse_checks(
            r#"[{"name":"build","workflow":"CI","bucket":"fail","state":"FAILURE","description":"x",
                 "link":"https://github.com/o/r/actions/runs/987/job/55"},
                {"name":"deploy","workflow":"","bucket":"pending","state":"IN_PROGRESS","link":""},
                {"name":"lint","bucket":"pass","state":"SUCCESS","link":"https://ci.example.com/1"},
                {"name":"docs","bucket":"skipping","state":"SKIPPED","link":""},
                {"name":"old","bucket":"cancel","state":"CANCELLED","link":""}]"#,
        )
        .unwrap();
        let buckets: Vec<_> = checks.iter().map(|c| c.bucket).collect();
        assert_eq!(
            buckets,
            [
                CheckBucket::Fail,
                CheckBucket::Pending,
                CheckBucket::Pass,
                CheckBucket::Skipping,
                CheckBucket::Cancel
            ]
        );
        assert_eq!(checks[0].run_id, Some(987));
        assert_eq!(checks[0].state, "failure");
        assert_eq!(checks[2].run_id, None);
        assert!(parse_checks("[]").unwrap().is_empty());
    }

    #[test]
    fn run_ids_come_from_action_links_only() {
        assert_eq!(
            run_id_from_link("https://github.com/o/r/actions/runs/123"),
            Some(123)
        );
        assert_eq!(
            run_id_from_link("https://github.com/o/r/actions/runs/123/job/9?pr=1"),
            Some(123)
        );
        assert_eq!(run_id_from_link("https://github.com/o/r/pull/3"), None);
        assert_eq!(
            run_id_from_link("https://github.com/o/r/actions/runs/abc"),
            None
        );
    }

    #[test]
    fn review_threads_are_read_with_their_comments() {
        let threads = parse_threads(
            r#"{"data":{"repository":{"pullRequest":{"reviewThreads":{"nodes":[
                {"id":"T1","isResolved":false,"isOutdated":false,"path":"src/a.ts","line":12,
                 "comments":{"nodes":[{"body":"Rename this","url":"https://x/1","author":{"login":"alice"}},
                                       {"body":"Agreed","url":"https://x/2","author":null}]}},
                {"id":"T2","isResolved":true,"isOutdated":true,"path":"b.rs","line":null,"comments":{"nodes":[]}}
            ]}}}}}"#,
        )
        .unwrap();
        assert_eq!(threads.len(), 2);
        assert_eq!(threads[0].line, Some(12));
        assert!(!threads[0].resolved);
        assert_eq!(threads[0].comments[0].author, "alice");
        assert_eq!(threads[0].comments[1].author, "someone");
        assert!(threads[1].resolved && threads[1].outdated);
        assert_eq!(threads[1].line, None);
        assert!(parse_threads(r#"{"data":null}"#).is_err());
    }

    #[test]
    fn deployments_and_their_newest_status() {
        let list = parse_deployment_list(
            r#"[{"id":7,"environment":"preview","created_at":"2026-01-01T00:00:00Z"},{"id":6}]"#,
        )
        .unwrap();
        assert_eq!(
            list[0],
            (7, "preview".into(), "2026-01-01T00:00:00Z".into())
        );
        assert_eq!(list[1].1, "deployment");
        let (state, url) = parse_deployment_status(
            r#"[{"state":"success","environment_url":"https://preview.example.com","log_url":"https://logs"}]"#,
        )
        .unwrap();
        assert_eq!(
            (state.as_str(), url.as_deref()),
            ("success", Some("https://preview.example.com"))
        );
        assert_eq!(
            parse_deployment_status("[]").unwrap(),
            ("unknown".to_string(), None)
        );
        let (_, url) = parse_deployment_status(
            r#"[{"state":"failure","environment_url":"","log_url":"https://logs"}]"#,
        )
        .unwrap();
        assert_eq!(url.as_deref(), Some("https://logs"));
    }

    #[test]
    fn create_output_and_log_helpers() {
        assert_eq!(
            url_from_create_output(
                "Creating pull request for x into main\n\nhttps://github.com/o/r/pull/9\n"
            )
            .as_deref(),
            Some("https://github.com/o/r/pull/9")
        );
        assert_eq!(url_from_create_output("nothing useful"), None);
        assert_eq!(tail_chars("short", 100), "short");
        let long = "a".repeat(50) + "END";
        let cut = tail_chars(&long, 10);
        assert!(cut.ends_with("aaaaaaaEND"));
        assert!(cut.starts_with("… 43 characters"));
        assert_eq!(MergeMethod::parse(None).flag(), "--squash");
        assert_eq!(MergeMethod::parse(Some("rebase")).flag(), "--rebase");
        assert_eq!(MergeMethod::parse(Some("merge")).flag(), "--merge");
        assert_eq!(MergeMethod::parse(Some("weird")).flag(), "--squash");
    }

    #[test]
    fn new_error_cases_are_classified() {
        assert_eq!(
            classify("API rate limit exceeded for user").kind,
            GhErrorKind::RateLimited
        );
        assert_eq!(
            classify("dial tcp: lookup api.github.com: no such host").kind,
            GhErrorKind::Network
        );
    }
}
