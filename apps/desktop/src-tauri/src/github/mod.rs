//! GitHub through the `gh` CLI. No OAuth and no tokens of our own: `gh` owns authentication.

use std::path::{Path, PathBuf};
pub mod browse;
pub mod browse_detail;
pub mod browse_files;
pub mod pr;

use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::process::Command;
use ts_rs::TS;

use crate::env::ResolvedEnv;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const CLONE_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum GhErrorKind {
    NotInstalled,
    NotAuthenticated,
    Network,
    RateLimited,
    NotFound,
    InvalidRepo,
    Timeout,
    Other,
}

#[derive(Debug, Clone)]
pub struct GhError {
    pub kind: GhErrorKind,
    pub message: String,
    /// The shell command that fixes the problem, when there is one.
    pub fix: Option<String>,
}

impl std::fmt::Display for GhError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for GhError {}

impl GhError {
    fn new(kind: GhErrorKind, message: impl Into<String>, fix: Option<&str>) -> Self {
        Self {
            kind,
            message: message.into(),
            fix: fix.map(str::to_string),
        }
    }

    pub fn details(&self) -> serde_json::Value {
        serde_json::json!({ "kind": self.kind, "fix": self.fix })
    }

    fn not_installed() -> Self {
        Self::new(
            GhErrorKind::NotInstalled,
            "The GitHub CLI (gh) is not installed. Install it with `brew install gh`.",
            Some("brew install gh"),
        )
    }

    fn not_authenticated() -> Self {
        Self::new(
            GhErrorKind::NotAuthenticated,
            "The GitHub CLI is not signed in. Run `gh auth login` in a terminal.",
            Some("gh auth login"),
        )
    }
}

/// Maps `gh` stderr to a typed error.
pub fn classify(stderr: &str) -> GhError {
    let lower = stderr.to_lowercase();
    let has = |n: &str| lower.contains(n);
    if has("gh auth login")
        || has("not logged in")
        || has("http 401")
        || has("requires authentication")
    {
        GhError::not_authenticated()
    } else if has("rate limit") || has("http 403") && has("secondary") {
        GhError::new(
            GhErrorKind::RateLimited,
            "GitHub is limiting requests right now. Try again in a few minutes.",
            None,
        )
    } else if has("could not resolve host")
        || has("error connecting")
        || has("dial tcp")
        || has("network is unreachable")
    {
        GhError::new(
            GhErrorKind::Network,
            "Could not reach GitHub. Check your connection.",
            None,
        )
    } else if has("no commits between") {
        GhError::new(
            GhErrorKind::Other,
            "GitHub sees no commits on this branch that the base branch lacks. Commit your changes and push them first.",
            None,
        )
    } else if has("could not resolve to a repository") || has("http 404") || has("not found") {
        GhError::new(
            GhErrorKind::NotFound,
            "That repository was not found, or you do not have access to it.",
            None,
        )
    } else {
        let first = stderr
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("gh failed");
        GhError::new(GhErrorKind::Other, first.trim().to_string(), None)
    }
}

#[derive(Debug, Clone, Serialize, TS, PartialEq, Eq)]
#[ts(export)]
pub struct GhRepo {
    pub name_with_owner: String,
    pub name: String,
    pub description: Option<String>,
    pub is_private: bool,
    pub url: String,
    pub owner: String,
    pub avatar_url: String,
}

/// The repositories to pick from: the signed in account's own, then those of each organization it
/// belongs to.
#[derive(Debug, Clone, Serialize, TS, PartialEq, Eq)]
#[ts(export)]
pub struct GhRepos {
    /// The account `gh` is signed in as.
    pub account: String,
    pub repos: Vec<GhRepo>,
    /// Organizations whose repositories could not be listed, such as ones that need SSO.
    pub failed_orgs: Vec<String>,
}

#[derive(Deserialize)]
struct RawRepo {
    #[serde(rename = "nameWithOwner")]
    name_with_owner: String,
    name: String,
    description: Option<String>,
    #[serde(rename = "isPrivate")]
    is_private: bool,
    url: String,
    owner: RawOwner,
}

#[derive(Deserialize)]
struct RawOwner {
    login: String,
}

const PR_FIELDS: &str = "number,title,headRefName,isCrossRepository,author,updatedAt,state";

/// One pull request, as much as a list of branches needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrBrief {
    pub number: u32,
    pub head_ref: String,
    pub state: pr::PrState,
    pub is_draft: bool,
}

pub fn parse_pr_briefs(json: &str) -> Result<Vec<PrBrief>, GhError> {
    #[derive(serde::Deserialize)]
    struct Raw {
        number: u32,
        #[serde(rename = "headRefName", default)]
        head_ref_name: String,
        #[serde(default)]
        state: String,
        #[serde(rename = "isDraft", default)]
        is_draft: bool,
    }
    let raw: Vec<Raw> = serde_json::from_str(json).map_err(bad_output)?;
    Ok(raw
        .into_iter()
        .map(|r| PrBrief {
            number: r.number,
            head_ref: r.head_ref_name,
            state: match r.state.as_str() {
                "MERGED" => pr::PrState::Merged,
                "CLOSED" => pr::PrState::Closed,
                _ => pr::PrState::Open,
            },
            is_draft: r.is_draft,
        })
        .collect())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrInfo {
    pub number: u32,
    pub title: String,
    pub head_ref: String,
    pub author: String,
    /// The branch lives in a fork, so it is not available as `origin/<head_ref>`.
    pub is_cross_repository: bool,
    pub updated_at: String,
}

/// An open pull request with checks that fail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailingPr {
    pub number: u32,
    pub title: String,
    pub head_ref: String,
    /// Names of the checks that fail, sorted.
    pub failing: Vec<String>,
}

#[derive(Deserialize)]
struct RawRollupPr {
    number: u32,
    title: String,
    #[serde(rename = "headRefName")]
    head_ref_name: String,
    #[serde(rename = "statusCheckRollup", default)]
    rollup: Vec<serde_json::Value>,
}

/// Parses `gh pr list --json number,title,headRefName,statusCheckRollup`, keeping only the pull
/// requests with a check that failed. A check run reports a `conclusion`, a status reports a
/// `state`.
pub fn parse_failing_prs(json: &str) -> Result<Vec<FailingPr>, GhError> {
    let raw: Vec<RawRollupPr> = serde_json::from_str(json).map_err(bad_output)?;
    Ok(raw
        .into_iter()
        .filter_map(|pr| {
            let mut failing: Vec<String> = pr
                .rollup
                .iter()
                .filter(|c| {
                    matches!(
                        c["conclusion"].as_str(),
                        Some("FAILURE" | "TIMED_OUT" | "STARTUP_FAILURE")
                    ) || matches!(c["state"].as_str(), Some("FAILURE" | "ERROR"))
                })
                .filter_map(|c| c["name"].as_str().or_else(|| c["context"].as_str()))
                .map(str::to_string)
                .collect();
            failing.sort();
            failing.dedup();
            (!failing.is_empty()).then_some(FailingPr {
                number: pr.number,
                title: pr.title,
                head_ref: pr.head_ref_name,
                failing,
            })
        })
        .collect())
}

#[derive(Deserialize)]
struct RawPr {
    number: u32,
    title: String,
    #[serde(rename = "headRefName")]
    head_ref_name: String,
    #[serde(rename = "isCrossRepository", default)]
    is_cross_repository: bool,
    author: Option<RawOwner>,
    #[serde(rename = "updatedAt", default)]
    updated_at: String,
}

impl From<RawPr> for PrInfo {
    fn from(r: RawPr) -> Self {
        Self {
            number: r.number,
            title: r.title,
            head_ref: r.head_ref_name,
            author: r.author.map(|a| a.login).unwrap_or_default(),
            is_cross_repository: r.is_cross_repository,
            updated_at: r.updated_at,
        }
    }
}

fn bad_output(e: serde_json::Error) -> GhError {
    GhError::new(
        GhErrorKind::Other,
        format!("unexpected gh output: {e}"),
        None,
    )
}

pub fn parse_pr_list(json: &str) -> Result<Vec<PrInfo>, GhError> {
    let raw: Vec<RawPr> = serde_json::from_str(json).map_err(bad_output)?;
    Ok(raw.into_iter().map(PrInfo::from).collect())
}

pub fn parse_pr(json: &str) -> Result<PrInfo, GhError> {
    let raw: RawPr = serde_json::from_str(json).map_err(bad_output)?;
    Ok(raw.into())
}

pub fn avatar_url(owner: &str) -> String {
    format!("https://github.com/{owner}.png?size=64")
}

/// Parses `gh repo list --json nameWithOwner,name,description,isPrivate,url,owner`.
pub fn parse_repo_list(json: &str) -> Result<Vec<GhRepo>, GhError> {
    let raw: Vec<RawRepo> = serde_json::from_str(json).map_err(|e| {
        GhError::new(
            GhErrorKind::Other,
            format!("unexpected gh output: {e}"),
            None,
        )
    })?;
    Ok(raw
        .into_iter()
        .map(|r| GhRepo {
            avatar_url: avatar_url(&r.owner.login),
            owner: r.owner.login,
            name_with_owner: r.name_with_owner,
            name: r.name,
            description: r.description.filter(|d| !d.is_empty()),
            is_private: r.is_private,
            url: r.url,
        })
        .collect())
}

/// The login and id from `gh api user --jq '[.login,.id]|@tsv'`.
pub fn parse_account(out: &str) -> Option<(String, u64)> {
    let (login, id) = out.trim().split_once('\t')?;
    let id = id.trim().parse().ok()?;
    (!login.is_empty()).then(|| (login.to_string(), id))
}

/// The address GitHub credits to an account without showing a real one.
pub fn noreply_email(login: &str, id: u64) -> String {
    format!("{id}+{login}@users.noreply.github.com")
}

/// Normalises `owner/name`, `https://github.com/owner/name(.git)` and `git@github.com:owner/name.git`
/// into `(owner, name)`. Returns `None` for anything that is not a GitHub repository reference.
pub fn parse_repo_ref(input: &str) -> Option<(String, String)> {
    let trimmed = input.trim().trim_end_matches('/');
    let path = if let Some(rest) = trimmed.strip_prefix("git@github.com:") {
        rest
    } else if let Some(rest) = trimmed
        .strip_prefix("https://github.com/")
        .or_else(|| trimmed.strip_prefix("http://github.com/"))
        .or_else(|| trimmed.strip_prefix("ssh://git@github.com/"))
    {
        rest
    } else if trimmed.contains("://") || trimmed.contains('@') {
        return None;
    } else {
        trimmed
    };
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, name) = path.split_once('/')?;
    let valid = |s: &str| {
        !s.is_empty()
            && s != "."
            && s != ".."
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    (valid(owner) && valid(name)).then(|| (owner.to_string(), name.to_string()))
}

/// Owner of a GitHub remote URL, used to pick a project avatar.
pub fn owner_from_remote(url: &str) -> Option<String> {
    parse_repo_ref(url).map(|(owner, _)| owner)
}

/// How long a token found for an account is reused.
const TOKEN_TTL: Duration = Duration::from_secs(60);

type TokenCache =
    std::sync::Mutex<std::collections::HashMap<String, (std::time::Instant, Option<String>)>>;

fn token_cache() -> &'static TokenCache {
    static CACHE: std::sync::OnceLock<TokenCache> = std::sync::OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// The token of the signed in `gh` account that owns the repository in `cwd`.
///
/// `gh` can hold several accounts (a work and a personal one) and uses whichever is active, which
/// is often not the one that owns the repository. Asking for the owner's own token and handing it
/// to `gh` (and to `git push`) as `GH_TOKEN` makes each repository use its own account. `None`
/// means: change nothing. That is the case when the owner is not one of the accounts, the remote
/// is not on GitHub, or a token is already set in the environment on purpose.
pub async fn account_token_for(env: &ResolvedEnv, cwd: &Path) -> Option<String> {
    if env.get("GH_TOKEN").is_some() || env.get("GITHUB_TOKEN").is_some() {
        return None;
    }
    let git = env.which("git")?;
    let mut remote = Command::new(git);
    env.apply(&mut remote);
    remote
        .args(["remote", "get-url", "origin"])
        .current_dir(cwd)
        .stdin(Stdio::null())
        .kill_on_drop(true);
    let out = tokio::time::timeout(Duration::from_secs(10), remote.output())
        .await
        .ok()?
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let url = String::from_utf8_lossy(&out.stdout);
    let url = url.trim();
    if !url.contains("github.com") {
        return None;
    }
    let (owner, name) = parse_repo_ref(url)?;

    let key = format!("{owner}/{name}").to_lowercase();
    if let Some((at, token)) = token_cache().lock().ok()?.get(&key) {
        if at.elapsed() < TOKEN_TTL {
            return token.clone();
        }
    }
    let token = pick_account(env, &owner, &name).await;
    if let Ok(mut cache) = token_cache().lock() {
        cache.insert(key, (std::time::Instant::now(), token.clone()));
    }
    token
}

/// The accounts `gh` is signed in to, the active one first.
async fn account_logins(env: &ResolvedEnv) -> Vec<String> {
    signed_in_accounts(env)
        .await
        .into_iter()
        .map(|(login, _)| login)
        .collect()
}

/// The accounts `gh` is signed in to on github.com, each with whether it is the active one, the
/// active one first.
pub async fn signed_in_accounts(env: &ResolvedEnv) -> Vec<(String, bool)> {
    let Some(gh) = env.which("gh") else {
        return Vec::new();
    };
    let mut cmd = Command::new(gh);
    env.apply(&mut cmd);
    cmd.args([
        "auth",
        "status",
        "--hostname",
        "github.com",
        "--json",
        "hosts",
    ])
    .env("GH_PROMPT_DISABLED", "1")
    .env("NO_COLOR", "1")
    .stdin(Stdio::null())
    .kill_on_drop(true);
    let Ok(Ok(out)) = tokio::time::timeout(Duration::from_secs(10), cmd.output()).await else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else {
        return Vec::new();
    };
    let mut accounts: Vec<(bool, String)> = json["hosts"]["github.com"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter(|a| a["state"] == "success")
                .filter_map(|a| {
                    Some((
                        a["active"].as_bool().unwrap_or(false),
                        a["login"].as_str()?.to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    accounts.sort_by_key(|(active, _)| !active);
    accounts
        .into_iter()
        .map(|(active, login)| (login, active))
        .collect()
}

/// Makes `login` the active `gh` account. It must be one of the accounts `gh` is signed in to.
pub async fn switch_account(env: &ResolvedEnv, login: &str) -> Result<(), GhError> {
    let known = signed_in_accounts(env).await;
    if !known.iter().any(|(l, _)| l == login) {
        return Err(GhError::new(
            GhErrorKind::Other,
            format!("{login} is not signed in to the GitHub CLI."),
            None,
        ));
    }
    let gh = env.which("gh").ok_or_else(GhError::not_installed)?;
    let mut cmd = Command::new(gh);
    env.apply(&mut cmd);
    cmd.args([
        "auth",
        "switch",
        "--hostname",
        "github.com",
        "--user",
        login,
    ])
    .env("GH_PROMPT_DISABLED", "1")
    .env("NO_COLOR", "1")
    .stdin(Stdio::null())
    .kill_on_drop(true);
    match tokio::time::timeout(Duration::from_secs(10), cmd.output()).await {
        Ok(Ok(out)) if out.status.success() => Ok(()),
        Ok(Ok(out)) => Err(classify(&String::from_utf8_lossy(&out.stderr))),
        _ => Err(GhError::new(
            GhErrorKind::Other,
            "The GitHub CLI did not answer.",
            None,
        )),
    }
}

/// The account for a repository: the one named like its owner, or else the first that can see it.
/// That second case is a work organisation's repository and a work login.
async fn pick_account(env: &ResolvedEnv, owner: &str, name: &str) -> Option<String> {
    let logins = account_logins(env).await;
    if let Some(login) = logins.iter().find(|l| l.eq_ignore_ascii_case(owner)) {
        return account_token(env, login).await;
    }
    for login in &logins {
        let Some(token) = account_token(env, login).await else {
            continue;
        };
        if can_see(env, &token, owner, name).await {
            return Some(token);
        }
    }
    None
}

async fn can_see(env: &ResolvedEnv, token: &str, owner: &str, name: &str) -> bool {
    let Some(gh) = env.which("gh") else {
        return false;
    };
    let mut cmd = Command::new(gh);
    env.apply(&mut cmd);
    cmd.args(["api", &format!("repos/{owner}/{name}"), "--silent"])
        .env("GH_TOKEN", token)
        .env("GH_PROMPT_DISABLED", "1")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .kill_on_drop(true);
    matches!(
        tokio::time::timeout(Duration::from_secs(15), cmd.output()).await,
        Ok(Ok(out)) if out.status.success()
    )
}

async fn account_token(env: &ResolvedEnv, user: &str) -> Option<String> {
    let gh = env.which("gh")?;
    let mut cmd = Command::new(gh);
    env.apply(&mut cmd);
    cmd.args(["auth", "token", "--hostname", "github.com", "--user", user])
        .env("GH_PROMPT_DISABLED", "1")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .kill_on_drop(true);
    let out = tokio::time::timeout(Duration::from_secs(10), cmd.output())
        .await
        .ok()?
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let token = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!token.is_empty()).then_some(token)
}

#[derive(Clone)]
pub struct Gh {
    env: Arc<ResolvedEnv>,
}

impl Gh {
    pub fn new(env: Arc<ResolvedEnv>) -> Self {
        Self { env }
    }

    async fn run(&self, args: &[&str], timeout: Duration) -> Result<String, GhError> {
        self.run_in(None, args, timeout).await
    }

    /// Runs `gh` in `cwd`, so it infers the repository from that folder's remote.
    async fn run_in(
        &self,
        cwd: Option<&Path>,
        args: &[&str],
        timeout: Duration,
    ) -> Result<String, GhError> {
        let bin: PathBuf = self.env.which("gh").ok_or_else(GhError::not_installed)?;
        let mut cmd = Command::new(bin);
        if let Some(cwd) = cwd {
            cmd.current_dir(cwd);
        }
        self.env.apply(&mut cmd);
        if let Some(token) = match cwd {
            Some(cwd) => account_token_for(&self.env, cwd).await,
            None => None,
        } {
            cmd.env("GH_TOKEN", token);
        }
        cmd.env("GH_PROMPT_DISABLED", "1")
            .env("NO_COLOR", "1")
            .env("GH_NO_UPDATE_NOTIFIER", "1")
            .args(args)
            .stdin(Stdio::null())
            .kill_on_drop(true);

        let output = tokio::time::timeout(timeout, cmd.output())
            .await
            .map_err(|_| {
                GhError::new(
                    GhErrorKind::Timeout,
                    format!("gh {} timed out", args.first().unwrap_or(&"")),
                    None,
                )
            })?
            .map_err(|e| GhError::new(GhErrorKind::Other, format!("cannot run gh: {e}"), None))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Err(classify(&String::from_utf8_lossy(&output.stderr)))
        }
    }

    /// Runs `gh` and returns its exit code with the output, so a caller can use the output of a
    /// command that exits non-zero on purpose.
    async fn run_raw(
        &self,
        cwd: &Path,
        args: &[&str],
    ) -> Result<(Option<i32>, String, String), GhError> {
        let bin: PathBuf = self.env.which("gh").ok_or_else(GhError::not_installed)?;
        let mut cmd = Command::new(bin);
        cmd.current_dir(cwd);
        self.env.apply(&mut cmd);
        if let Some(token) = account_token_for(&self.env, cwd).await {
            cmd.env("GH_TOKEN", token);
        }
        cmd.env("GH_PROMPT_DISABLED", "1")
            .env("NO_COLOR", "1")
            .env("GH_NO_UPDATE_NOTIFIER", "1")
            .args(args)
            .stdin(Stdio::null())
            .kill_on_drop(true);
        let output = tokio::time::timeout(DEFAULT_TIMEOUT, cmd.output())
            .await
            .map_err(|_| {
                GhError::new(
                    GhErrorKind::Timeout,
                    format!("gh {} timed out", args.first().unwrap_or(&"")),
                    None,
                )
            })?
            .map_err(|e| GhError::new(GhErrorKind::Other, format!("cannot run gh: {e}"), None))?;
        Ok((
            output.status.code(),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ))
    }

    /// Login of the signed in user. Fails with `NotInstalled` or `NotAuthenticated`.
    pub async fn current_user(&self) -> Result<String, GhError> {
        let out = self
            .run(&["api", "user", "--jq", ".login"], DEFAULT_TIMEOUT)
            .await?;
        Ok(out.trim().to_string())
    }

    /// Login of the account that `gh` uses for the repository in `repo`, which can differ from
    /// the active account when several are signed in.
    pub async fn current_user_in(&self, repo: &Path) -> Result<String, GhError> {
        let out = self
            .run_in(
                Some(repo),
                &["api", "user", "--jq", ".login"],
                DEFAULT_TIMEOUT,
            )
            .await?;
        Ok(out.trim().to_string())
    }

    /// Login and numeric id of the account that `gh` uses for the repository in `repo`.
    pub async fn account_in(&self, repo: &Path) -> Result<(String, u64), GhError> {
        let out = self
            .run_in(
                Some(repo),
                &["api", "user", "--jq", r#"[.login,.id]|@tsv"#],
                DEFAULT_TIMEOUT,
            )
            .await?;
        parse_account(&out).ok_or_else(|| {
            GhError::new(
                GhErrorKind::Other,
                "gh returned no account".to_string(),
                None,
            )
        })
    }

    /// Repositories of `owner`, a user or an organization, or of the signed in account.
    pub async fn list_repos(
        &self,
        owner: Option<&str>,
        limit: u32,
    ) -> Result<Vec<GhRepo>, GhError> {
        let limit = limit.to_string();
        let mut args = vec!["repo", "list"];
        args.extend(owner);
        args.extend([
            "--limit",
            &limit,
            "--json",
            "nameWithOwner,name,description,isPrivate,url,owner",
        ]);
        let out = self.run(&args, DEFAULT_TIMEOUT).await?;
        parse_repo_list(&out)
    }

    /// Logins of the organizations the signed in account belongs to.
    pub async fn list_orgs(&self) -> Result<Vec<String>, GhError> {
        let out = self
            .run(
                &["api", "user/orgs", "--paginate", "--jq", ".[].login"],
                DEFAULT_TIMEOUT,
            )
            .await?;
        Ok(out
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect())
    }

    /// Open pull requests of the repository in `repo`.
    pub async fn pr_list(&self, repo: &Path, limit: u32) -> Result<Vec<PrInfo>, GhError> {
        let limit = limit.to_string();
        let out = self
            .run_in(
                Some(repo),
                &[
                    "pr", "list", "--state", "open", "--limit", &limit, "--json", PR_FIELDS,
                ],
                DEFAULT_TIMEOUT,
            )
            .await?;
        parse_pr_list(&out)
    }

    /// The state of the most recent pull requests of the repository in `repo`, open or not, so
    /// many branches can be told apart with one call.
    pub async fn pr_states(&self, repo: &Path, limit: u32) -> Result<Vec<PrBrief>, GhError> {
        let limit = limit.to_string();
        let out = self
            .run_in(
                Some(repo),
                &[
                    "pr",
                    "list",
                    "--state",
                    "all",
                    "--limit",
                    &limit,
                    "--json",
                    "number,headRefName,state,isDraft",
                ],
                DEFAULT_TIMEOUT,
            )
            .await?;
        parse_pr_briefs(&out)
    }

    /// Open pull requests of the repository in `repo` that have failing checks.
    pub async fn failing_prs(&self, repo: &Path) -> Result<Vec<FailingPr>, GhError> {
        let out = self
            .run_in(
                Some(repo),
                &[
                    "pr",
                    "list",
                    "--state",
                    "open",
                    "--limit",
                    "50",
                    "--json",
                    "number,title,headRefName,statusCheckRollup",
                ],
                DEFAULT_TIMEOUT,
            )
            .await?;
        parse_failing_prs(&out)
    }

    /// One pull request by number, open or not.
    pub async fn pr_view(&self, repo: &Path, number: u32) -> Result<PrInfo, GhError> {
        let number = number.to_string();
        let out = self
            .run_in(
                Some(repo),
                &["pr", "view", &number, "--json", PR_FIELDS],
                DEFAULT_TIMEOUT,
            )
            .await?;
        parse_pr(&out)
    }

    pub async fn clone_repo(&self, owner: &str, name: &str, dest: &Path) -> Result<(), GhError> {
        let full = format!("{owner}/{name}");
        let dest = dest.to_string_lossy().into_owned();
        self.run(&["repo", "clone", &full, &dest], CLONE_TIMEOUT)
            .await
            .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_an_account_and_makes_its_noreply_address() {
        assert_eq!(
            parse_account("decimozs\t12345\n"),
            Some(("decimozs".to_string(), 12345))
        );
        assert_eq!(parse_account("nothing"), None);
        assert_eq!(parse_account("\t5"), None);
        assert_eq!(
            noreply_email("decimozs", 12345),
            "12345+decimozs@users.noreply.github.com"
        );
    }

    #[test]
    fn briefs_carry_the_state_of_each_pull_request() {
        let json = r#"[
          {"number": 3, "headRefName": "feat/a", "state": "OPEN", "isDraft": true},
          {"number": 2, "headRefName": "feat/b", "state": "MERGED", "isDraft": false},
          {"number": 1, "headRefName": "feat/c", "state": "CLOSED"}
        ]"#;
        let briefs = parse_pr_briefs(json).unwrap();
        assert_eq!(briefs.len(), 3);
        assert!(briefs[0].is_draft && briefs[0].state == pr::PrState::Open);
        assert_eq!(briefs[1].state, pr::PrState::Merged);
        assert_eq!(briefs[2].state, pr::PrState::Closed);
        assert!(parse_pr_briefs("not json").is_err());
    }

    #[test]
    fn only_pull_requests_with_a_failing_check_are_kept() {
        let json = r#"[
          {"number": 1, "title": "Green", "headRefName": "a", "statusCheckRollup": [
            {"name": "build", "conclusion": "SUCCESS"}, {"name": "lint", "conclusion": "SKIPPED"}]},
          {"number": 2, "title": "Red", "headRefName": "b", "statusCheckRollup": [
            {"name": "test", "conclusion": "FAILURE"}, {"name": "build", "conclusion": "SUCCESS"},
            {"context": "ci/legacy", "state": "ERROR"}, {"name": "test", "conclusion": "FAILURE"}]},
          {"number": 3, "title": "Waiting", "headRefName": "c", "statusCheckRollup": [
            {"name": "test", "conclusion": ""}, {"context": "ci", "state": "PENDING"}]},
          {"number": 4, "title": "No checks", "headRefName": "d", "statusCheckRollup": []},
          {"number": 5, "title": "Missing", "headRefName": "e"}
        ]"#;
        let failing = parse_failing_prs(json).unwrap();
        assert_eq!(failing.len(), 1);
        assert_eq!(failing[0].number, 2);
        assert_eq!(failing[0].failing, ["ci/legacy", "test"]);
        assert!(parse_failing_prs("nope").is_err());
    }

    #[test]
    fn parses_repo_references() {
        let ok = Some(("octo".to_string(), "hello.world".to_string()));
        assert_eq!(parse_repo_ref("octo/hello.world"), ok);
        assert_eq!(parse_repo_ref("https://github.com/octo/hello.world"), ok);
        assert_eq!(
            parse_repo_ref("https://github.com/octo/hello.world.git"),
            ok
        );
        assert_eq!(parse_repo_ref("https://github.com/octo/hello.world/"), ok);
        assert_eq!(parse_repo_ref("git@github.com:octo/hello.world.git"), ok);
        assert_eq!(parse_repo_ref("  octo/hello.world  "), ok);
    }

    #[test]
    fn rejects_bad_references() {
        for bad in [
            "",
            "octo",
            "octo/",
            "/name",
            "a/b/c",
            "../evil",
            "octo/..",
            "https://gitlab.com/octo/x",
            "evil@host:octo/x",
            "octo/na me",
            "octo/na;rm",
        ] {
            assert_eq!(parse_repo_ref(bad), None, "should reject {bad:?}");
        }
    }

    #[test]
    fn owner_from_remote_urls() {
        assert_eq!(
            owner_from_remote("git@github.com:me/x.git").as_deref(),
            Some("me")
        );
        assert_eq!(
            owner_from_remote("https://github.com/me/x").as_deref(),
            Some("me")
        );
        assert_eq!(owner_from_remote("/local/bare.git"), None);
    }

    #[test]
    fn classifies_gh_failures() {
        assert_eq!(
            classify("To get started with GitHub CLI, please run:  gh auth login").kind,
            GhErrorKind::NotAuthenticated
        );
        assert_eq!(
            classify("gh: Not Logged in (HTTP 401)").kind,
            GhErrorKind::NotAuthenticated
        );
        assert_eq!(
            classify("error connecting to api.github.com").kind,
            GhErrorKind::Network
        );
        assert_eq!(
            classify("GraphQL: Could not resolve to a Repository with the name 'a/b'.").kind,
            GhErrorKind::NotFound
        );
        assert_eq!(classify("something odd").kind, GhErrorKind::Other);
        assert_eq!(
            GhError::not_authenticated().fix.as_deref(),
            Some("gh auth login")
        );
    }

    #[test]
    fn parses_repo_list_json() {
        let json = r#"[
          {"nameWithOwner":"me/roster","name":"roster","description":"","isPrivate":true,"url":"https://github.com/me/roster","owner":{"id":"x","login":"me"}},
          {"nameWithOwner":"me/circles","name":"circles","description":"Agents","isPrivate":false,"url":"https://github.com/me/circles","owner":{"login":"me"}}
        ]"#;
        let repos = parse_repo_list(json).unwrap();
        assert_eq!(repos.len(), 2);
        assert_eq!(repos[0].description, None);
        assert!(repos[0].is_private);
        assert_eq!(repos[1].description.as_deref(), Some("Agents"));
        assert_eq!(repos[1].avatar_url, "https://github.com/me.png?size=64");
        assert!(parse_repo_list("not json").is_err());
    }

    /// Talks to the real GitHub CLI with the developer's login. Run with
    /// `cargo test --lib -- --ignored real_gh`.
    #[tokio::test]
    #[ignore = "needs a signed in gh and network access"]
    async fn real_gh_lists_repos() {
        let env = Arc::new(crate::env::resolve().await);
        let gh = Gh::new(env);
        let login = gh.current_user().await.unwrap();
        assert!(!login.is_empty());
        let repos = gh.list_repos(None, 5).await.unwrap();
        assert!(repos.len() <= 5);
        for repo in &repos {
            assert!(repo.name_with_owner.contains('/'));
            assert!(repo.url.starts_with("https://github.com/"));
        }
        println!("signed in as {login}, {} repos", repos.len());
    }

    #[test]
    fn parses_pull_requests() {
        let list = r#"[
          {"number":12,"title":"Fix login","headRefName":"me/fix-login","isCrossRepository":false,"author":{"login":"me"},"updatedAt":"2026-10-01T10:00:00Z","state":"OPEN"},
          {"number":9,"title":"From a fork","headRefName":"patch-1","isCrossRepository":true,"author":null,"updatedAt":"2026-09-01T10:00:00Z","state":"OPEN"}
        ]"#;
        let prs = parse_pr_list(list).unwrap();
        assert_eq!(prs.len(), 2);
        assert_eq!(
            (
                prs[0].number,
                prs[0].head_ref.as_str(),
                prs[0].author.as_str()
            ),
            (12, "me/fix-login", "me")
        );
        assert!(!prs[0].is_cross_repository);
        assert!(prs[1].is_cross_repository);
        assert_eq!(prs[1].author, "");

        let one = parse_pr(r#"{"number":3,"title":"t","headRefName":"b"}"#).unwrap();
        assert_eq!((one.number, one.head_ref.as_str()), (3, "b"));
        assert!(parse_pr("nope").is_err());
    }

    #[tokio::test]
    async fn missing_gh_is_typed() {
        let env = ResolvedEnv::for_test(&[("PATH", "/nonexistent")]);
        let err = Gh::new(Arc::new(env)).current_user().await.unwrap_err();
        assert_eq!(err.kind, GhErrorKind::NotInstalled);
        assert_eq!(err.fix.as_deref(), Some("brew install gh"));
    }

    #[tokio::test]
    async fn accounts_are_listed_with_the_active_one_first_and_one_can_be_switched_to() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(tmp.path().join("active"), "work").unwrap();
        let script = bin.join("gh");
        std::fs::write(
            &script,
            r#"#!/bin/sh
active=$(cat "$STATE/active")
case "$1 $2" in
  "auth status")
    w=false; p=false
    [ "$active" = work ] && w=true
    [ "$active" = me ] && p=true
    echo "{\"hosts\":{\"github.com\":[{\"state\":\"success\",\"active\":$w,\"login\":\"work\"},{\"state\":\"success\",\"active\":$p,\"login\":\"me\"},{\"state\":\"error\",\"active\":false,\"login\":\"old\"}]}}" ;;
  "auth switch")
    while [ $# -gt 0 ]; do [ "$1" = "--user" ] && echo "$2" > "$STATE/active"; shift; done ;;
  *) echo "unexpected: $@" >&2; exit 2 ;;
esac
"#,
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let path = format!("{}:/usr/bin:/bin", bin.display());
        let env = ResolvedEnv::for_test(&[
            ("PATH", path.as_str()),
            ("STATE", tmp.path().to_str().unwrap()),
        ]);

        let listed = signed_in_accounts(&env).await;
        // The active one first, and an account that is not working is left out.
        assert_eq!(
            listed,
            vec![("work".to_string(), true), ("me".to_string(), false)]
        );
        switch_account(&env, "me").await.unwrap();
        assert_eq!(
            signed_in_accounts(&env).await,
            vec![("me".to_string(), true), ("work".to_string(), false)]
        );
        // An account that is not signed in cannot be switched to.
        assert!(switch_account(&env, "stranger").await.is_err());
        assert!(switch_account(&env, "old").await.is_err());
    }

    /// A repository whose `origin` is `url`, and a fake `gh` signed in to `accounts` (the first is
    /// active) where only `sees` can open the repository.
    fn account_fixture(
        url: &str,
        accounts: &[&str],
        sees: &str,
    ) -> (tempfile::TempDir, PathBuf, String) {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        for args in [vec!["init", "-q"], vec!["remote", "add", "origin", url]] {
            assert!(std::process::Command::new("git")
                .args(args)
                .current_dir(&repo)
                .status()
                .unwrap()
                .success());
        }
        let hosts: Vec<String> = accounts
            .iter()
            .enumerate()
            .map(|(i, a)| {
                format!(
                    r#"{{"state":"success","active":{},"host":"github.com","login":"{a}"}}"#,
                    i == 0
                )
            })
            .collect();
        let json = format!(r#"{{"hosts":{{"github.com":[{}]}}}}"#, hosts.join(","));
        let known = accounts.join(" ");
        let bin = tmp.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let script = bin.join("gh");
        std::fs::write(
            &script,
            format!(
                r#"#!/bin/sh
case "$1 $2" in
  "auth status") echo '{json}'; exit 0;;
  "auth token")
    while [ $# -gt 0 ]; do [ "$1" = "--user" ] && user=$2; shift; done
    for a in {known}; do [ "$a" = "$user" ] && echo "token-of-$user" && exit 0; done
    echo "no such account" >&2; exit 1;;
esac
[ "$1 $2" = "api user" ] && echo "${{GH_TOKEN#token-of-}}" && exit 0
[ "$1" = "api" ] && [ "$GH_TOKEN" = "token-of-{sees}" ] && exit 0
exit 1
"#
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let path = format!(
            "{}:{}",
            bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        (tmp, repo, path)
    }

    fn test_env(path: &str, extra: &[(&str, &str)]) -> ResolvedEnv {
        let mut vars = vec![("PATH", path), ("HOME", "/nonexistent")];
        vars.extend_from_slice(extra);
        ResolvedEnv::for_test(&vars)
    }

    #[tokio::test]
    async fn a_repository_uses_the_account_named_like_its_owner() {
        let (_tmp, repo, path) = account_fixture(
            "https://github.com/acct-owner-a/thing.git",
            &["work-a", "acct-owner-a"],
            "nobody",
        );
        assert_eq!(
            account_token_for(&test_env(&path, &[]), &repo)
                .await
                .as_deref(),
            Some("token-of-acct-owner-a")
        );
    }

    #[tokio::test]
    async fn an_organisation_repository_uses_the_account_that_can_see_it() {
        // The owner is an organisation, so no account carries its name. The work login can see
        // the repository and the active personal one cannot.
        let (_tmp, repo, path) = account_fixture(
            "git@github.com:acct-org-b/thing.git",
            &["personal-b", "work-b"],
            "work-b",
        );
        assert_eq!(
            account_token_for(&test_env(&path, &[]), &repo)
                .await
                .as_deref(),
            Some("token-of-work-b")
        );
    }

    #[tokio::test]
    async fn the_login_of_a_repository_is_the_account_that_works_for_it() {
        let (_tmp, repo, path) = account_fixture(
            "https://github.com/acct-org-f/thing.git",
            &["personal-f", "work-f"],
            "work-f",
        );
        let gh = Gh::new(Arc::new(test_env(&path, &[])));
        assert_eq!(gh.current_user_in(&repo).await.unwrap(), "work-f");
    }

    #[tokio::test]
    async fn nothing_changes_when_no_account_can_see_the_repository() {
        let (_tmp, repo, path) = account_fixture(
            "https://github.com/acct-org-c/thing",
            &["personal-c", "work-c"],
            "nobody",
        );
        assert_eq!(account_token_for(&test_env(&path, &[]), &repo).await, None);
    }

    #[tokio::test]
    async fn a_token_set_on_purpose_and_other_hosts_are_left_alone() {
        let (_tmp, repo, path) = account_fixture(
            "https://github.com/acct-owner-d/thing",
            &["acct-owner-d"],
            "acct-owner-d",
        );
        let env = test_env(&path, &[("GH_TOKEN", "mine")]);
        assert_eq!(
            account_token_for(&env, &repo).await,
            None,
            "explicit token wins"
        );

        let (_tmp2, repo2, path2) = account_fixture(
            "https://gitlab.com/acct-owner-e/thing",
            &["acct-owner-e"],
            "acct-owner-e",
        );
        assert_eq!(
            account_token_for(&test_env(&path2, &[]), &repo2).await,
            None
        );
    }
}
