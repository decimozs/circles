//! Browsing the pull requests of the repositories you have added.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use serde::Deserialize;
use tauri::{AppHandle, Emitter, State};
use ts_rs::TS;

use crate::cache::{CacheStore, Entry};
use crate::error::AppError;
use crate::files::{looks_like_text, TEXT_LIMIT};
use crate::github::browse::{browse_all, BrowseState, PrList, PrRow, RepoRef, Whose};
use crate::github::browse_detail::PrDetail;
use crate::github::browse_files::PrFiles;
use crate::github::{Gh, GhError, GhErrorKind};
use crate::ipc::{now_ms, IpcResult};
use crate::review::changes::FilePair;
use crate::state::AppState;

/// Most pull requests asked for from one repository.
const PER_REPO: u32 = 50;
/// What was fetched less than this long ago is not fetched again just because it was asked for.
const FRESH_MS: i64 = 15_000;
/// A pull request that is not open changes rarely, so it is looked at again less often.
const SETTLED_MS: i64 = 10 * 60 * 1000;

/// What to do with the pull requests of several repositories, given what is kept for each.
#[derive(Debug, Default)]
struct ListPlan {
    /// Rows to show now, from what is kept.
    rows: Vec<PrRow>,
    /// Repositories with nothing kept: they must be asked, and the answer waited for.
    missing: Vec<RepoRef>,
    /// Repositories with something kept that is old enough to ask again in the background.
    stale: Vec<RepoRef>,
    /// When the oldest part of `rows` was fetched.
    oldest: Option<i64>,
}

fn plan_list(kept: Vec<(RepoRef, Option<Entry<Vec<PrRow>>>)>, now: i64) -> ListPlan {
    let mut plan = ListPlan::default();
    for (repo, entry) in kept {
        match entry {
            None => plan.missing.push(repo),
            Some(e) => {
                plan.oldest = Some(plan.oldest.map_or(e.fetched_at, |o| o.min(e.fetched_at)));
                if now - e.fetched_at > FRESH_MS {
                    plan.stale.push(repo);
                }
                plan.rows.extend(e.value);
            }
        }
    }
    plan
}

/// Whether a kept pull request is old enough to ask GitHub about again.
fn detail_is_stale(state: &str, fetched_at: i64, now: i64) -> bool {
    let limit = if state == "open" {
        FRESH_MS
    } else {
        SETTLED_MS
    };
    now - fetched_at > limit
}

fn list_key(project_id: &str, which: BrowseState) -> String {
    format!("pr-list:{project_id}:{}", which.flag())
}

fn detail_key(project_id: &str, number: u32) -> String {
    format!("pr-detail:{project_id}:{number}")
}

/// Keys being fetched in the background right now, so the same one is not fetched twice at once.
fn in_flight() -> &'static Mutex<HashSet<String>> {
    static SET: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    SET.get_or_init(Default::default)
}

struct Flight(String);

impl Flight {
    fn start(key: &str) -> Option<Self> {
        in_flight()
            .lock()
            .ok()?
            .insert(key.to_string())
            .then(|| Flight(key.to_string()))
    }
}

impl Drop for Flight {
    fn drop(&mut self) {
        if let Ok(mut set) = in_flight().lock() {
            set.remove(&self.0);
        }
    }
}

/// An answer that GitHub says is gone or not allowed must not stay readable from here.
fn is_gone(e: &GhError) -> bool {
    matches!(
        e.kind,
        GhErrorKind::NotFound | GhErrorKind::NotAuthenticated
    )
}

/// Keeps what GitHub said about each repository, and drops what it refused.
fn keep_lists(cache: &CacheStore, which: BrowseState, repos: &[RepoRef], list: &PrList) {
    for repo in repos {
        let key = list_key(&repo.project_id, which);
        if list
            .failures
            .iter()
            .any(|f| f.project_id == repo.project_id)
        {
            if list
                .failures
                .iter()
                .any(|f| f.project_id == repo.project_id && f.gone)
            {
                let _ = cache.delete(&key);
            }
            continue;
        }
        let rows: Vec<&PrRow> = list
            .rows
            .iter()
            .filter(|r| r.project_id == repo.project_id)
            .collect();
        let _ = cache.put(&key, &rows);
    }
}

/// Asks GitHub again about repositories that have something kept, in the background, and tells the
/// window when the answer is in.
fn refresh_lists_later(
    app: AppHandle,
    cache: CacheStore,
    gh: Gh,
    repos: Vec<RepoRef>,
    which: BrowseState,
) {
    let flights: Vec<(RepoRef, Flight)> = repos
        .into_iter()
        .filter_map(|r| Flight::start(&list_key(&r.project_id, which)).map(|f| (r, f)))
        .collect();
    if flights.is_empty() {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let repos: Vec<RepoRef> = flights.iter().map(|(r, _)| r.clone()).collect();
        let list = browse_all(&gh, repos.clone(), which, PER_REPO).await;
        keep_lists(&cache, which, &repos, &list);
        drop(flights);
        let _ = app.emit("cache:changed", "pr-list");
    });
}

/// The pull requests of one project, or of every project with a GitHub remote when `project_id`
/// is not given. What was kept is returned at once and asked again in the background when it is
/// old; only a repository with nothing kept, or `refresh`, waits for GitHub.
#[tauri::command]
pub async fn pr_browse_list(
    app: AppHandle,
    state: State<'_, AppState>,
    project_id: Option<String>,
    which: BrowseState,
    refresh: Option<bool>,
) -> IpcResult<PrList> {
    let projects = state.project_service().await.list()?;
    let repos: Vec<RepoRef> = projects
        .into_iter()
        .filter(|p| project_id.as_deref().is_none_or(|id| id == p.id))
        .filter(|p| {
            p.path_exists
                && p.remote_url
                    .as_deref()
                    .is_some_and(|u| u.contains("github.com"))
        })
        .map(|p| RepoRef {
            project_id: p.id,
            project_name: p.name,
            path: p.path.into(),
        })
        .collect();
    let gh = state.pr_service().await.gh.clone();
    let cache = CacheStore::new(state.db.clone());
    let force = refresh.unwrap_or(false);
    let now = now_ms();

    let kept = repos
        .iter()
        .map(|r| {
            let entry = if force {
                None
            } else {
                cache.get::<Vec<PrRow>>(&list_key(&r.project_id, which))?
            };
            Ok((r.clone(), entry))
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    let mut plan = plan_list(kept, now);

    let live = browse_all(&gh, plan.missing.clone(), which, PER_REPO).await;
    keep_lists(&cache, which, &plan.missing, &live);
    let refreshing = !plan.stale.is_empty();
    refresh_lists_later(app, cache, gh, std::mem::take(&mut plan.stale), which);

    let mut rows = plan.rows;
    rows.extend(live.rows);
    rows.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then(a.repo.cmp(&b.repo))
            .then(b.number.cmp(&a.number))
    });
    Ok(PrList {
        rows,
        failures: live.failures,
        fetched_at: Some(plan.oldest.unwrap_or(now)),
        refreshing,
    })
}

/// The folder and name of a project that has a repository on disk.
async fn project_folder(state: &AppState, project_id: &str) -> Result<(PathBuf, String), AppError> {
    let project = state.project_service().await.get(project_id)?;
    if !project.path_exists {
        return Err(AppError::NotFound(format!(
            "The folder of {} is not on this Mac any more.",
            project.name
        )));
    }
    Ok((project.path.into(), project.name))
}

/// Asks GitHub about one pull request again, keeps the answer and tells the window.
fn refresh_detail_later(
    app: AppHandle,
    cache: CacheStore,
    gh: Gh,
    folder: PathBuf,
    project_id: String,
    name: String,
    number: u32,
) {
    let key = detail_key(&project_id, number);
    let Some(flight) = Flight::start(&key) else {
        return;
    };
    tauri::async_runtime::spawn(async move {
        let me = gh.current_user_in(&folder).await.unwrap_or_default();
        let whose = Whose {
            project_id: &project_id,
            project_name: &name,
            me: &me,
        };
        match gh.browse_pr(&folder, &whose, number).await {
            Ok(detail) => {
                let _ = cache.put(&key, &detail);
            }
            Err(e) if is_gone(&e) => {
                let _ = cache.delete(&key);
            }
            Err(_) => {}
        }
        drop(flight);
        let _ = app.emit("cache:changed", "pr-detail");
    });
}

/// One pull request in full. What was kept is returned at once, and asked again in the background
/// when it is old (an open one sooner than one that is closed); `refresh` waits for GitHub.
#[tauri::command]
pub async fn pr_browse_detail(
    app: AppHandle,
    state: State<'_, AppState>,
    project_id: String,
    number: u32,
    refresh: Option<bool>,
) -> IpcResult<PrDetail> {
    let (folder, name) = project_folder(&state, &project_id).await?;
    let gh = state.pr_service().await.gh.clone();
    let cache = CacheStore::new(state.db.clone());
    let key = detail_key(&project_id, number);
    if !refresh.unwrap_or(false) {
        if let Some(kept) = cache.get::<PrDetail>(&key)? {
            let stale = detail_is_stale(&kept.value.row.state, kept.fetched_at, now_ms());
            if stale {
                refresh_detail_later(app, cache, gh, folder, project_id, name, number);
            }
            let mut detail = kept.value;
            detail.fetched_at = Some(kept.fetched_at);
            detail.refreshing = stale;
            return Ok(detail);
        }
    }
    let me = gh.current_user_in(&folder).await.unwrap_or_default();
    let whose = Whose {
        project_id: &project_id,
        project_name: &name,
        me: &me,
    };
    let mut detail = match gh.browse_pr(&folder, &whose, number).await {
        Ok(detail) => detail,
        Err(e) => {
            if is_gone(&e) {
                let _ = cache.delete(&key);
            }
            return Err(e.into());
        }
    };
    detail.fetched_at = Some(cache.put(&key, &detail)?);
    Ok(detail)
}

/// The files a pull request changes, and the commits to read them at.
#[tauri::command]
pub async fn pr_browse_files(
    state: State<'_, AppState>,
    project_id: String,
    repo: String,
    number: u32,
) -> IpcResult<PrFiles> {
    let (folder, _) = project_folder(&state, &project_id).await?;
    let gh = state.pr_service().await.gh.clone();
    Ok(gh.browse_pr_files(&folder, &repo, number).await?)
}

/// Which file of which pull request to read, and at which commits.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct PrFileRequest {
    pub project_id: String,
    /// `owner/name`.
    pub repo: String,
    pub base_sha: String,
    pub head_sha: String,
    pub path: String,
    pub previous_path: Option<String>,
    /// `added`, `removed` and the like, as `pr_browse_files` reported it.
    pub status: String,
}

/// Both sides of one file of a pull request. The project's own git is asked first, since it is
/// quicker and costs nothing when it already has the commits; GitHub is asked for what it lacks.
#[tauri::command]
pub async fn pr_browse_file(
    state: State<'_, AppState>,
    request: PrFileRequest,
) -> IpcResult<FilePair> {
    let PrFileRequest {
        project_id,
        repo,
        base_sha,
        head_sha,
        path,
        previous_path,
        status,
    } = request;
    let (folder, _) = project_folder(&state, &project_id).await?;
    let service = state.pr_service().await;
    let (gh, git) = (service.gh.clone(), service.git.clone());
    let read = |sha: String, at: String| {
        let (gh, git, folder, repo) = (gh.clone(), git.clone(), folder.clone(), repo.clone());
        async move {
            if let Ok(Some(text)) = git.show_blob(&folder, &sha, &at).await {
                return Ok::<_, AppError>(Some(text));
            }
            Ok(gh.browse_file_text(&folder, &repo, &sha, &at).await?)
        }
    };
    let original = if status == "added" {
        None
    } else {
        read(base_sha, previous_path.unwrap_or_else(|| path.clone())).await?
    };
    let modified = if status == "removed" {
        None
    } else {
        read(head_sha, path.clone()).await?
    };
    let language = crate::files::language_for(&path).to_string();
    let too_large = [&original, &modified]
        .iter()
        .any(|t| t.as_deref().is_some_and(|t| t.len() as u64 > TEXT_LIMIT));
    let binary = [&original, &modified]
        .iter()
        .any(|t| t.as_deref().is_some_and(|t| !looks_like_text(t.as_bytes())));
    if binary || too_large {
        return Ok(FilePair {
            path,
            original: None,
            modified: None,
            language,
            binary,
            too_large,
        });
    }
    Ok(FilePair {
        path,
        original,
        modified,
        language,
        binary: false,
        too_large: false,
    })
}

#[cfg(test)]
mod cache_tests {
    use super::*;
    use crate::github::browse::{BrowseFailure, PrChecks};
    use std::sync::Arc;

    fn repo(id: &str) -> RepoRef {
        RepoRef {
            project_id: id.into(),
            project_name: id.into(),
            path: PathBuf::from("/x"),
        }
    }

    fn row(project: &str, number: u32) -> PrRow {
        PrRow {
            project_id: project.into(),
            project_name: project.into(),
            repo: format!("o/{project}"),
            number,
            title: "t".into(),
            url: String::new(),
            author: String::new(),
            author_avatar: String::new(),
            head_ref: String::new(),
            base_ref: String::new(),
            state: "open".into(),
            is_draft: false,
            additions: 0,
            deletions: 0,
            changed_files: 0,
            created_at: String::new(),
            updated_at: String::new(),
            labels: vec![],
            review: "none".into(),
            requested_reviewers: vec![],
            checks: PrChecks {
                state: "none".into(),
                passed: 0,
                total: 0,
            },
            by_me: false,
            review_requested: false,
        }
    }

    fn entry(rows: Vec<PrRow>, fetched_at: i64) -> Option<Entry<Vec<PrRow>>> {
        Some(Entry {
            value: rows,
            fetched_at,
        })
    }

    #[test]
    fn what_is_kept_shows_at_once_and_only_what_is_missing_is_waited_for() {
        let now = 1_000_000;
        let plan = plan_list(
            vec![
                (repo("fresh"), entry(vec![row("fresh", 1)], now - 5_000)),
                (repo("old"), entry(vec![row("old", 2)], now - 60_000)),
                (repo("new"), None),
            ],
            now,
        );
        assert_eq!(plan.rows.len(), 2);
        assert_eq!(
            plan.missing
                .iter()
                .map(|r| r.project_id.as_str())
                .collect::<Vec<_>>(),
            ["new"]
        );
        // Only the one older than fifteen seconds is asked about again.
        assert_eq!(
            plan.stale
                .iter()
                .map(|r| r.project_id.as_str())
                .collect::<Vec<_>>(),
            ["old"]
        );
        // The age shown is that of the oldest part.
        assert_eq!(plan.oldest, Some(now - 60_000));
    }

    #[test]
    fn nothing_kept_means_everything_is_asked_and_nothing_is_old() {
        let plan = plan_list(vec![(repo("a"), None), (repo("b"), None)], 5);
        assert_eq!(
            (
                plan.rows.len(),
                plan.missing.len(),
                plan.stale.len(),
                plan.oldest
            ),
            (0, 2, 0, None)
        );
    }

    #[test]
    fn an_open_pull_request_is_asked_about_sooner_than_one_that_is_over() {
        let now = 10_000_000;
        assert!(!detail_is_stale("open", now - 10_000, now));
        assert!(detail_is_stale("open", now - 20_000, now));
        assert!(!detail_is_stale("merged", now - 5 * 60_000, now));
        assert!(detail_is_stale("merged", now - 11 * 60_000, now));
        assert!(!detail_is_stale("closed", now - 60_000, now));
    }

    fn store() -> (tempfile::TempDir, CacheStore) {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(crate::db::Db::open(&tmp.path().join("t.db")).unwrap());
        (tmp, CacheStore::new(db))
    }

    #[test]
    fn answers_are_kept_per_repository_and_a_repository_that_is_gone_is_forgotten() {
        let (_tmp, cache) = store();
        let repos = vec![repo("a"), repo("b"), repo("c")];
        let list = PrList {
            rows: vec![row("a", 1), row("a", 2), row("b", 3)],
            failures: vec![],
            fetched_at: None,
            refreshing: false,
        };
        keep_lists(&cache, BrowseState::Open, &repos, &list);
        let kept = |id: &str| {
            cache
                .get::<Vec<PrRow>>(&list_key(id, BrowseState::Open))
                .unwrap()
        };
        assert_eq!(kept("a").unwrap().value.len(), 2);
        assert_eq!(kept("b").unwrap().value.len(), 1);
        // A repository with none open is kept as empty, so it is not waited for next time.
        assert_eq!(kept("c").unwrap().value.len(), 0);
        // Another state is another entry.
        assert!(cache
            .get::<Vec<PrRow>>(&list_key("a", BrowseState::All))
            .unwrap()
            .is_none());

        // A failure keeps what was there; one that says "gone" forgets it.
        let failing = |gone| PrList {
            rows: vec![],
            failures: vec![BrowseFailure {
                project_id: "a".into(),
                project_name: "a".into(),
                message: "x".into(),
                fix: None,
                gone,
            }],
            fetched_at: None,
            refreshing: false,
        };
        keep_lists(&cache, BrowseState::Open, &repos[..1], &failing(false));
        assert!(kept("a").is_some());
        keep_lists(&cache, BrowseState::Open, &repos[..1], &failing(true));
        assert!(kept("a").is_none());
    }

    #[test]
    fn the_same_key_is_not_fetched_twice_at_once() {
        let first = Flight::start("pr-list:x:open").expect("free");
        assert!(Flight::start("pr-list:x:open").is_none());
        assert!(Flight::start("pr-list:y:open").is_some());
        drop(first);
        assert!(Flight::start("pr-list:x:open").is_some());
    }
}
