//! Projects: the repositories Circles manages. Adding a project only records it. Nothing in
//! the repository changes, and removing a project never deletes the repository.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusqlite::{params, Row};
use serde::Serialize;
use ts_rs::TS;

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::git::{Git, GitError, GitErrorKind};
use crate::github::{self, Gh, GhRepos};
use crate::ipc::{new_id, now_ms};
use crate::paths::AppPaths;

/// The most repositories listed for one owner. A repository outside it is cloned by its name.
const PER_OWNER: u32 = 100;

#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[ts(export)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: String,
    /// The repository folder exists. False after it was moved or deleted outside Circles.
    pub path_exists: bool,
    pub default_branch: String,
    pub remote_url: Option<String>,
    /// GitHub owner avatar when `origin` is a GitHub remote. The UI falls back to a letter.
    pub avatar_url: Option<String>,
    /// Workspaces that are not archived.
    pub workspace_count: u32,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub updated_at: i64,
}

const SELECT: &str =
    "SELECT p.id, p.name, p.path, p.default_branch, p.remote_url, p.created_at, p.updated_at,
        (SELECT COUNT(*) FROM workspaces w WHERE w.project_id = p.id AND w.state != 'archived')
     FROM projects p";

fn from_row(row: &Row<'_>) -> rusqlite::Result<Project> {
    let remote_url: Option<String> = row.get(4)?;
    let avatar_url = remote_url
        .as_deref()
        .and_then(github::owner_from_remote)
        .map(|owner| github::avatar_url(&owner));
    let path: String = row.get(2)?;
    Ok(Project {
        id: row.get(0)?,
        name: row.get(1)?,
        path_exists: Path::new(&path).is_dir(),
        path,
        default_branch: row.get(3)?,
        remote_url,
        avatar_url,
        workspace_count: row.get(7)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

#[derive(Clone)]
pub struct ProjectService {
    pub db: Arc<Db>,
    pub git: Git,
    pub gh: Gh,
    pub paths: AppPaths,
}

fn invalid(message: impl Into<String>) -> AppError {
    AppError::Invalid(message.into())
}

/// Folder name for a new repository. Keeps to characters that are safe in paths and URLs.
fn validate_repo_name(name: &str) -> AppResult<&str> {
    let name = name.trim();
    let ok = !name.is_empty()
        && name.len() <= 100
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if ok {
        Ok(name)
    } else {
        Err(invalid(
            "Use letters, numbers, dashes, underscores and dots for the project name (up to 100 characters, not starting with a dot).",
        ))
    }
}

impl ProjectService {
    pub fn list(&self) -> AppResult<Vec<Project>> {
        self.db.read(|c| {
            let mut stmt = c.prepare(&format!(
                "{SELECT} WHERE p.id != 'composers' ORDER BY LOWER(p.name), p.created_at"
            ))?;
            let rows = stmt.query_map([], from_row)?;
            rows.collect()
        })
    }

    pub fn get(&self, id: &str) -> AppResult<Project> {
        self.db
            .read(|c| c.query_row(&format!("{SELECT} WHERE p.id = ?1"), params![id], from_row))
            .map_err(|e| match e {
                AppError::Db(rusqlite::Error::QueryReturnedNoRows) => {
                    AppError::NotFound(format!("project {id}"))
                }
                other => other,
            })
    }

    fn find_by_path(&self, path: &str) -> AppResult<Option<Project>> {
        match self.db.read(|c| {
            c.query_row(
                &format!("{SELECT} WHERE p.path = ?1"),
                params![path],
                from_row,
            )
        }) {
            Ok(p) => Ok(Some(p)),
            Err(AppError::Db(rusqlite::Error::QueryReturnedNoRows)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Adds a repository that already exists on disk.
    pub async fn add_local(&self, path: &str) -> AppResult<Project> {
        let input = PathBuf::from(path);
        if !input.is_absolute() {
            return Err(invalid("Choose a folder using its full path."));
        }
        if !input.is_dir() {
            return Err(invalid(format!("{} is not a folder.", input.display())));
        }
        let input = std::fs::canonicalize(&input)?;
        self.register(&input).await
    }

    /// Validates `dir` and records it. `dir` may be anywhere inside the repository.
    async fn register(&self, dir: &Path) -> AppResult<Project> {
        let top = std::fs::canonicalize(self.git.toplevel(dir).await?)?;

        let workspaces_dir = std::fs::canonicalize(&self.paths.workspaces_dir)
            .unwrap_or_else(|_| self.paths.workspaces_dir.clone());
        if top.starts_with(&workspaces_dir) || self.git.is_linked_worktree(&top).await? {
            let main = self.git.main_worktree(&top).await.ok();
            return Err(invalid(match main {
                Some(main) => format!(
                    "This folder is a git worktree. Add the main repository instead: {}",
                    main.display()
                ),
                None => {
                    "This folder is a git worktree. Add the main repository instead.".to_string()
                }
            }));
        }

        let top_str = top.to_string_lossy().into_owned();
        if let Some(existing) = self.find_by_path(&top_str)? {
            return Err(AppError::AlreadyExists(format!(
                "{} is already added as \"{}\".",
                top_str, existing.name
            )));
        }

        if !self.git.has_commits(&top).await? {
            return Err(GitError::new(
                GitErrorKind::NoCommits,
                "This repository has no commits yet. Make an initial commit, then add it again.",
            )
            .into());
        }

        let default_branch = self.git.detect_default_branch(&top).await?;
        let remote_url = self.git.remote_url(&top, "origin").await?;
        let name = top
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "project".to_string());

        let id = new_id();
        let now = now_ms();
        self.db.write(|c| {
            c.execute(
                "INSERT INTO projects (id, name, path, default_branch, remote_url, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                params![id, name, top_str, default_branch, remote_url, now],
            )
        })?;
        tracing::info!(project = %name, base = %default_branch, "project added");
        self.get(&id)
    }

    /// Repositories of the signed in GitHub account and of the organizations it belongs to. An
    /// organization that cannot be listed is named, not failed on.
    pub async fn github_repos(&self) -> AppResult<GhRepos> {
        let account = self.gh.current_user().await?;
        let mut repos = self.gh.list_repos(None, PER_OWNER).await?;
        // Organizations are a bonus: when they cannot be listed the account's own still show.
        let orgs = self.gh.list_orgs().await.unwrap_or_default();
        let mut tasks = tokio::task::JoinSet::new();
        for org in orgs {
            let gh = self.gh.clone();
            tasks.spawn(async move {
                let listed = gh.list_repos(Some(&org), PER_OWNER).await;
                (org, listed)
            });
        }
        let mut by_org = Vec::new();
        let mut failed_orgs = Vec::new();
        while let Some(done) = tasks.join_next().await {
            let Ok((org, listed)) = done else { continue };
            match listed {
                Ok(list) => by_org.push((org, list)),
                Err(e) => {
                    tracing::warn!(%org, error = %e, "cannot list an organization's repositories");
                    failed_orgs.push(org);
                }
            }
        }
        by_org.sort_by_key(|(org, _)| org.to_lowercase());
        failed_orgs.sort();
        repos.extend(by_org.into_iter().flat_map(|(_, list)| list));
        Ok(GhRepos {
            account,
            repos,
            failed_orgs,
        })
    }

    /// Clones a GitHub repository into `~/circles/repos/` and adds it.
    pub async fn add_github(&self, reference: &str) -> AppResult<Project> {
        let (owner, name) = github::parse_repo_ref(reference)
            .ok_or_else(|| invalid("Enter a repository as owner/name or paste its GitHub URL."))?;

        // Fail early with a clear message when gh is missing or signed out.
        self.gh.current_user().await?;

        if let Some(existing) = self.find_by_remote(&owner, &name)? {
            return Err(AppError::AlreadyExists(format!(
                "{owner}/{name} is already added as \"{}\".",
                existing.name
            )));
        }

        std::fs::create_dir_all(&self.paths.repos_dir)?;
        let (dest, leftover) = self.clone_target(&owner, &name).await;
        if leftover {
            // A folder from before the project was removed: removing a project keeps its clone,
            // so adding it again picks the same folder up instead of cloning next to it.
            return self.register(&dest).await;
        }

        self.gh.clone_repo(&owner, &name, &dest).await?;
        match self.register(&dest).await {
            Ok(project) => Ok(project),
            Err(e) => {
                // We created this directory a moment ago, so it is safe to clean up.
                let _ = std::fs::remove_dir_all(&dest);
                Err(e)
            }
        }
    }

    /// Where to clone `owner/name`, and whether that folder already holds this repository from an
    /// earlier add. The first free `name`, `name-2`, ... otherwise.
    async fn clone_target(&self, owner: &str, name: &str) -> (PathBuf, bool) {
        let mut dest = self.paths.repos_dir.join(name);
        let mut n = 2;
        while dest.exists() {
            if self.is_leftover_clone(&dest, owner, name).await {
                return (dest, true);
            }
            dest = self.paths.repos_dir.join(format!("{name}-{n}"));
            n += 1;
        }
        (dest, false)
    }

    /// A clone of `owner/name` that no project uses.
    async fn is_leftover_clone(&self, dir: &Path, owner: &str, name: &str) -> bool {
        let Ok(top) = std::fs::canonicalize(dir) else {
            return false;
        };
        if !matches!(self.find_by_path(&top.to_string_lossy()), Ok(None)) {
            return false;
        }
        let Ok(Some(url)) = self.git.remote_url(&top, "origin").await else {
            return false;
        };
        github::parse_repo_ref(&url)
            .is_some_and(|(o, n)| o.eq_ignore_ascii_case(owner) && n.eq_ignore_ascii_case(name))
    }

    fn find_by_remote(&self, owner: &str, name: &str) -> AppResult<Option<Project>> {
        Ok(self.list()?.into_iter().find(|p| {
            p.remote_url
                .as_deref()
                .and_then(github::parse_repo_ref)
                .is_some_and(|(o, n)| o.eq_ignore_ascii_case(owner) && n.eq_ignore_ascii_case(name))
        }))
    }

    /// Creates a new repository in `~/circles/repos/<name>` with an initial commit.
    pub async fn quick_start(&self, name: &str) -> AppResult<Project> {
        let name = validate_repo_name(name)?;
        let dest = self.paths.repos_dir.join(name);
        if dest.exists() {
            return Err(AppError::AlreadyExists(format!(
                "{} already exists.",
                dest.display()
            )));
        }
        self.git
            .init_with_commit(&dest, &format!("# {name}\n"))
            .await
            .inspect_err(|_| {
                let _ = std::fs::remove_dir_all(&dest);
            })?;
        match self.register(&dest).await {
            Ok(project) => Ok(project),
            Err(e) => {
                let _ = std::fs::remove_dir_all(&dest);
                Err(e)
            }
        }
    }

    /// Removes the project from Circles. The repository on disk is never touched.
    pub fn remove(&self, id: &str) -> AppResult<()> {
        self.remove_with(id, false)
    }

    /// Removes the project even when it has workspaces. The caller deals with them first, either
    /// deleting them or letting them go, since their records go with the project.
    pub fn remove_with_workspaces(&self, id: &str) -> AppResult<()> {
        self.remove_with(id, true)
    }

    fn remove_with(&self, id: &str, with_workspaces: bool) -> AppResult<()> {
        let project = self.get(id)?;
        if !with_workspaces && project.workspace_count > 0 {
            return Err(invalid(format!(
                "\"{}\" still has {} workspace(s). Archive them first.",
                project.name, project.workspace_count
            )));
        }
        self.db
            .write(|c| c.execute("DELETE FROM projects WHERE id = ?1", params![id]))?;
        tracing::info!(project = %project.name, "project removed");
        Ok(())
    }

    pub async fn set_base_branch(&self, id: &str, branch: &str) -> AppResult<Project> {
        let branch = branch.trim();
        if branch.is_empty() {
            return Err(invalid("Enter a branch name."));
        }
        crate::git::not_an_option(&[branch])?;
        let project = self.get(id)?;
        if !self
            .git
            .branch_resolvable(Path::new(&project.path), branch)
            .await?
        {
            return Err(GitError::new(
                GitErrorKind::BranchNotFound,
                format!("The branch \"{branch}\" does not exist locally or on origin."),
            )
            .into());
        }
        self.db.write(|c| {
            c.execute(
                "UPDATE projects SET default_branch = ?1, updated_at = ?2 WHERE id = ?3",
                params![branch, now_ms(), id],
            )
        })?;
        self.get(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::testutil::{repo_with_commit, sh, test_git};

    struct Fixture {
        _tmp: tempfile::TempDir,
        root: PathBuf,
        svc: ProjectService,
    }

    fn fixture() -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let paths = AppPaths::from_roots(&root.join("data"), &root.join("home"));
        paths.ensure().unwrap();
        let db = Arc::new(Db::open(&paths.db_file).unwrap());
        let git = test_git(&root);
        let gh = Gh::new(Arc::new(crate::env::ResolvedEnv::for_test(&[(
            "PATH",
            "/nonexistent",
        )])));
        Fixture {
            _tmp: tmp,
            root,
            svc: ProjectService { db, git, gh, paths },
        }
    }

    async fn make_repo(f: &Fixture, name: &str) -> PathBuf {
        let dir = f.root.join(name);
        repo_with_commit(&f.svc.git, &dir).await;
        dir
    }

    #[tokio::test]
    async fn adding_a_removed_repository_again_reuses_its_folder() {
        let f = fixture();
        std::fs::create_dir_all(&f.svc.paths.repos_dir).unwrap();
        let dir = f.svc.paths.repos_dir.join("test-repo");
        repo_with_commit(&f.svc.git, &dir).await;
        let remote = |dir: &Path, url: &str| {
            assert!(std::process::Command::new("git")
                .args(["remote", "add", "origin", url])
                .current_dir(dir)
                .status()
                .unwrap()
                .success());
        };
        remote(&dir, "https://github.com/decimozs/test-repo.git");

        // Nothing uses the folder, so it is the place for the repository.
        let (dest, leftover) = f.svc.clone_target("decimozs", "test-repo").await;
        assert_eq!((dest, leftover), (dir.clone(), true));

        // A project that uses it holds it, so the next clone goes beside it.
        let project = f.svc.add_local(&dir.to_string_lossy()).await.unwrap();
        let (dest, leftover) = f.svc.clone_target("decimozs", "test-repo").await;
        assert_eq!(
            (dest, leftover),
            (f.svc.paths.repos_dir.join("test-repo-2"), false)
        );

        // Removing the project frees it again.
        f.svc.remove(&project.id).unwrap();
        assert!(f.svc.clone_target("decimozs", "test-repo").await.1);

        // A folder of another repository is never taken over.
        let other = f.svc.paths.repos_dir.join("test-repo");
        std::process::Command::new("git")
            .args([
                "remote",
                "set-url",
                "origin",
                "https://github.com/someone/else.git",
            ])
            .current_dir(&other)
            .status()
            .unwrap();
        let (dest, leftover) = f.svc.clone_target("decimozs", "test-repo").await;
        assert_eq!(
            (dest, leftover),
            (f.svc.paths.repos_dir.join("test-repo-2"), false)
        );
    }

    #[tokio::test]
    async fn github_repos_list_the_account_and_then_each_organization() {
        const FAKE_GH: &str = r#"#!/bin/sh
repo() { echo "[{\"nameWithOwner\":\"$1/$2\",\"name\":\"$2\",\"description\":\"\",\"isPrivate\":true,\"url\":\"https://github.com/$1/$2\",\"owner\":{\"login\":\"$1\"}}]"; }
case "$1 $2" in
  "api user") echo me ;;
  "api user/orgs") printf 'zeta\nacme\nlocked\n' ;;
  "repo list")
    case "$3" in
      --limit) repo me mine ;;
      zeta) repo zeta last ;;
      acme) repo acme first ;;
      *) echo "SAML enforcement" >&2; exit 1 ;;
    esac ;;
  *) echo "unexpected: $@" >&2; exit 2 ;;
esac
"#;
        let mut f = fixture();
        let bin = f.root.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let script = bin.join("gh");
        std::fs::write(&script, FAKE_GH).unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let path = format!("{}:/usr/bin:/bin", bin.display());
        f.svc.gh = Gh::new(Arc::new(crate::env::ResolvedEnv::for_test(&[(
            "PATH",
            path.as_str(),
        )])));

        let listed = f.svc.github_repos().await.unwrap();
        assert_eq!(listed.account, "me");
        let names: Vec<_> = listed
            .repos
            .iter()
            .map(|r| r.name_with_owner.as_str())
            .collect();
        // The account's own first, then the organizations by name.
        assert_eq!(names, ["me/mine", "acme/first", "zeta/last"]);
        // An organization that cannot be listed is named and the rest still show.
        assert_eq!(listed.failed_orgs, ["locked"]);
    }

    fn git_kind(e: &AppError) -> Option<GitErrorKind> {
        match e {
            AppError::Git(g) => Some(g.kind),
            _ => None,
        }
    }

    #[tokio::test]
    async fn adds_a_local_repository() {
        let f = fixture();
        let dir = make_repo(&f, "roster").await;
        let p = f.svc.add_local(dir.to_str().unwrap()).await.unwrap();
        assert_eq!(p.name, "roster");
        assert_eq!(p.path, dir.to_str().unwrap());
        assert_eq!(p.default_branch, "main");
        assert_eq!(p.remote_url, None);
        assert_eq!(p.avatar_url, None);
        assert_eq!(p.workspace_count, 0);
        assert_eq!(f.svc.list().unwrap(), vec![p]);
    }

    #[tokio::test]
    async fn reports_when_the_repository_folder_is_gone() {
        let f = fixture();
        let dir = make_repo(&f, "moved").await;
        let p = f.svc.add_local(dir.to_str().unwrap()).await.unwrap();
        assert!(p.path_exists);

        std::fs::remove_dir_all(&dir).unwrap();
        let after = f.svc.get(&p.id).unwrap();
        assert!(!after.path_exists);
        assert!(!f.svc.list().unwrap()[0].path_exists);
        // A project whose folder is gone can still be removed.
        f.svc.remove(&p.id).unwrap();
        assert!(f.svc.list().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_subfolder_resolves_to_the_repository_root() {
        let f = fixture();
        let dir = make_repo(&f, "mono").await;
        std::fs::create_dir_all(dir.join("apps/web")).unwrap();
        let p = f
            .svc
            .add_local(dir.join("apps/web").to_str().unwrap())
            .await
            .unwrap();
        assert_eq!(p.path, dir.to_str().unwrap());
    }

    #[tokio::test]
    async fn github_remote_gives_an_avatar() {
        let f = fixture();
        let dir = make_repo(&f, "gh").await;
        sh(
            &dir,
            &["remote", "add", "origin", "git@github.com:octo/gh.git"],
        );
        let p = f.svc.add_local(dir.to_str().unwrap()).await.unwrap();
        assert_eq!(p.remote_url.as_deref(), Some("git@github.com:octo/gh.git"));
        assert_eq!(
            p.avatar_url.as_deref(),
            Some("https://github.com/octo.png?size=64")
        );
    }

    #[tokio::test]
    async fn rejects_bad_inputs() {
        let f = fixture();
        let plain = f.root.join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        let err = f.svc.add_local(plain.to_str().unwrap()).await.unwrap_err();
        assert_eq!(git_kind(&err), Some(GitErrorKind::NotARepo));

        assert_eq!(
            f.svc.add_local("relative/path").await.unwrap_err().code(),
            "invalid"
        );
        assert_eq!(
            f.svc
                .add_local("/definitely/not/here")
                .await
                .unwrap_err()
                .code(),
            "invalid"
        );
    }

    #[tokio::test]
    async fn rejects_duplicates() {
        let f = fixture();
        let dir = make_repo(&f, "dup").await;
        f.svc.add_local(dir.to_str().unwrap()).await.unwrap();
        let err = f.svc.add_local(dir.to_str().unwrap()).await.unwrap_err();
        assert_eq!(err.code(), "already_exists");
        assert_eq!(f.svc.list().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn rejects_a_linked_worktree() {
        let f = fixture();
        let dir = make_repo(&f, "main-repo").await;
        let wt = f.root.join("a-worktree");
        f.svc
            .git
            .worktree_add_new_branch(&dir, &wt, "feat", "main")
            .await
            .unwrap();
        let err = f.svc.add_local(wt.to_str().unwrap()).await.unwrap_err();
        assert_eq!(err.code(), "invalid");
        assert!(err.to_string().contains("worktree"));
        assert!(err.to_string().contains(dir.to_str().unwrap()));
    }

    #[tokio::test]
    async fn rejects_a_repository_without_commits() {
        let f = fixture();
        let dir = f.root.join("empty");
        sh(&f.root, &["init", "-b", "main", "empty"]);
        let err = f.svc.add_local(dir.to_str().unwrap()).await.unwrap_err();
        assert_eq!(git_kind(&err), Some(GitErrorKind::NoCommits));
    }

    #[tokio::test]
    async fn quick_start_creates_a_repository_with_a_commit() {
        let f = fixture();
        let p = f.svc.quick_start("  my-app  ").await.unwrap();
        let expected = f.svc.paths.repos_dir.join("my-app");
        assert_eq!(p.name, "my-app");
        assert_eq!(
            PathBuf::from(&p.path),
            std::fs::canonicalize(&expected).unwrap()
        );
        assert!(expected.join("README.md").exists());
        assert!(f.svc.git.has_commits(&expected).await.unwrap());
        assert_eq!(p.default_branch, "main");
    }

    #[tokio::test]
    async fn quick_start_validates_names() {
        let f = fixture();
        for bad in [
            "",
            "   ",
            ".hidden",
            "..",
            "a/b",
            "has space",
            "semi;colon",
            &"x".repeat(101),
        ] {
            assert_eq!(
                f.svc.quick_start(bad).await.unwrap_err().code(),
                "invalid",
                "{bad:?}"
            );
        }
        f.svc.quick_start("taken").await.unwrap();
        assert_eq!(
            f.svc.quick_start("taken").await.unwrap_err().code(),
            "already_exists"
        );
        // A failed attempt leaves nothing behind.
        assert!(!f.svc.paths.repos_dir.join(".hidden").exists());
    }

    #[tokio::test]
    async fn quick_start_cleans_up_when_git_identity_is_missing() {
        let f = fixture();
        let path = std::env::var("PATH").unwrap_or_default();
        let env = crate::env::ResolvedEnv::for_test(&[
            ("PATH", path.as_str()),
            ("HOME", f.root.to_str().unwrap()),
            ("GIT_CONFIG_GLOBAL", "/dev/null"),
            ("GIT_CONFIG_SYSTEM", "/dev/null"),
            ("GIT_CONFIG_COUNT", "1"),
            ("GIT_CONFIG_KEY_0", "user.useConfigOnly"),
            ("GIT_CONFIG_VALUE_0", "true"),
        ]);
        let svc = ProjectService {
            db: f.svc.db.clone(),
            git: Git::new(Arc::new(env)),
            gh: f.svc.gh.clone(),
            paths: f.svc.paths.clone(),
        };
        let err = svc.quick_start("no-identity").await.unwrap_err();
        assert_eq!(git_kind(&err), Some(GitErrorKind::IdentityMissing));
        assert!(!f.svc.paths.repos_dir.join("no-identity").exists());
        assert!(svc.list().unwrap().is_empty());
    }

    #[tokio::test]
    async fn remove_keeps_the_repository_on_disk() {
        let f = fixture();
        let dir = make_repo(&f, "keepme").await;
        let p = f.svc.add_local(dir.to_str().unwrap()).await.unwrap();
        f.svc.remove(&p.id).unwrap();
        assert!(f.svc.list().unwrap().is_empty());
        assert!(dir.join(".git").exists());
        assert!(dir.join("README.md").exists());
        assert_eq!(f.svc.remove(&p.id).unwrap_err().code(), "not_found");
    }

    #[tokio::test]
    async fn remove_is_blocked_while_workspaces_exist() {
        let f = fixture();
        let dir = make_repo(&f, "busy").await;
        let p = f.svc.add_local(dir.to_str().unwrap()).await.unwrap();
        f.svc
            .db
            .write(|c| {
                c.execute(
                    "INSERT INTO workspaces (id,project_id,city,branch,base_branch,path,state,created_at,updated_at)
                     VALUES ('w',?1,'Oslo','b','main','/w','ready',1,1)",
                    params![p.id],
                )
            })
            .unwrap();
        assert_eq!(f.svc.get(&p.id).unwrap().workspace_count, 1);
        let err = f.svc.remove(&p.id).unwrap_err();
        assert_eq!(err.code(), "invalid");
        assert_eq!(f.svc.list().unwrap().len(), 1);

        // Archived workspaces do not block removal.
        f.svc
            .db
            .write(|c| c.execute("UPDATE workspaces SET state='archived'", []))
            .unwrap();
        f.svc.remove(&p.id).unwrap();
    }

    #[tokio::test]
    async fn base_branch_can_be_changed_to_an_existing_branch() {
        let f = fixture();
        let dir = make_repo(&f, "bb").await;
        let p = f.svc.add_local(dir.to_str().unwrap()).await.unwrap();
        sh(&dir, &["branch", "develop"]);
        let updated = f.svc.set_base_branch(&p.id, " develop ").await.unwrap();
        assert_eq!(updated.default_branch, "develop");

        let err = f.svc.set_base_branch(&p.id, "nope").await.unwrap_err();
        assert_eq!(git_kind(&err), Some(GitErrorKind::BranchNotFound));
        assert_eq!(f.svc.get(&p.id).unwrap().default_branch, "develop");
        assert_eq!(
            f.svc.set_base_branch(&p.id, "  ").await.unwrap_err().code(),
            "invalid"
        );
        assert_eq!(
            f.svc
                .set_base_branch("missing", "main")
                .await
                .unwrap_err()
                .code(),
            "not_found"
        );
    }

    /// Clones a small public repository with the real `gh`. Run with
    /// `cargo test --lib -- --ignored real_github_clone`.
    #[tokio::test]
    #[ignore = "needs a signed in gh and network access"]
    async fn real_github_clone_adds_a_project() {
        let f = fixture();
        let env = Arc::new(crate::env::resolve().await);
        let svc = ProjectService {
            db: f.svc.db.clone(),
            git: Git::new(env.clone()),
            gh: Gh::new(env),
            paths: f.svc.paths.clone(),
        };
        let p = svc
            .add_github("https://github.com/octocat/Hello-World.git")
            .await
            .unwrap();
        assert_eq!(p.name, "Hello-World");
        assert_eq!(
            p.avatar_url.as_deref(),
            Some("https://github.com/octocat.png?size=64")
        );
        assert!(!p.default_branch.is_empty());
        println!("added {} on {}", p.name, p.default_branch);

        // The same repository cannot be added twice, in any spelling.
        let err = svc.add_github("octocat/hello-world").await.unwrap_err();
        assert_eq!(err.code(), "already_exists");
        assert!(!svc.paths.repos_dir.join("Hello-World-2").exists());
    }

    /// Walks through the manual review steps for phase 4 against the real git and gh, in scratch
    /// folders. Signed-out gh is simulated with an empty `GH_CONFIG_DIR`.
    /// Run with `cargo test --lib -- --ignored --nocapture real_walkthrough`.
    #[tokio::test]
    #[ignore = "needs a signed in gh and network access"]
    async fn real_walkthrough() {
        let f = fixture();
        let real = crate::env::resolve().await;
        let env = Arc::new(real.clone());
        let svc = ProjectService {
            db: f.svc.db.clone(),
            git: Git::new(env.clone()),
            gh: Gh::new(env),
            paths: f.svc.paths.clone(),
        };
        let init = |name: &str, branch: &str, commit: bool| {
            let dir = f.root.join(name);
            sh(&f.root, &["init", "-q", "-b", branch, name]);
            if commit {
                std::fs::write(dir.join("a.txt"), "a\n").unwrap();
                sh(&dir, &["add", "-A"]);
                sh(&dir, &["commit", "-q", "-m", "init"]);
            }
            dir
        };
        let show = |label: &str, r: &AppResult<Project>| match r {
            Ok(p) => println!(
                "[{label}] OK  name={} base={} remote={:?}",
                p.name, p.default_branch, p.remote_url
            ),
            Err(e) => println!(
                "[{label}] ERR code={} details={}\n        message: {}",
                e.code(),
                e.payload()
                    .details
                    .map(|d| d.to_string())
                    .unwrap_or_default(),
                e
            ),
        };

        println!("\n== Step 2: repo with no remote, base-branch fallback");
        for (name, branch) in [
            ("no-remote-main", "main"),
            ("no-remote-master", "master"),
            ("no-remote-trunk", "trunk"),
        ] {
            let dir = init(name, branch, true);
            show(branch, &svc.add_local(dir.to_str().unwrap()).await);
        }

        println!("\n== Step 3: worktree folder, empty repo, non-repo");
        let main = f.root.join("no-remote-main");
        let wt = f.root.join("some-worktree");
        svc.git
            .worktree_add_new_branch(&main, &wt, "feat/x", "main")
            .await
            .unwrap();
        show("worktree", &svc.add_local(wt.to_str().unwrap()).await);
        let empty = init("empty-repo", "main", false);
        show("empty repo", &svc.add_local(empty.to_str().unwrap()).await);
        let plain = f.root.join("plain-folder");
        std::fs::create_dir_all(&plain).unwrap();
        show("non-repo", &svc.add_local(plain.to_str().unwrap()).await);
        show("duplicate", &svc.add_local(main.to_str().unwrap()).await);

        println!("\n== Step 4: GitHub clone, then signed out");
        let listed = svc.github_repos().await.unwrap();
        println!(
            "[list] {} repos, first: {}",
            listed.repos.len(),
            listed
                .repos
                .first()
                .map(|r| r.name_with_owner.as_str())
                .unwrap_or("-")
        );
        let cloned = svc.add_github("decimozs/neovim-config").await;
        show("clone decimozs/neovim-config", &cloned);
        if let Ok(p) = &cloned {
            println!("        cloned into {} (avatar {:?})", p.path, p.avatar_url);
            assert!(p.path.starts_with(
                f.svc
                    .paths
                    .repos_dir
                    .canonicalize()
                    .unwrap()
                    .to_str()
                    .unwrap()
            ));
        }
        let empty_cfg = tempfile::tempdir().unwrap();
        let signed_out = Arc::new(
            real.clone()
                .without_var("GH_TOKEN")
                .without_var("GITHUB_TOKEN")
                .with_var("GH_CONFIG_DIR", empty_cfg.path().to_str().unwrap()),
        );
        let out_svc = ProjectService {
            db: f.svc.db.clone(),
            git: Git::new(signed_out.clone()),
            gh: Gh::new(signed_out),
            paths: f.svc.paths.clone(),
        };
        let list_err = out_svc.github_repos().await.unwrap_err();
        println!(
            "[signed out: list] code={} details={}\n        message: {}",
            list_err.code(),
            list_err.payload().details.unwrap(),
            list_err
        );
        show(
            "signed out: clone",
            &out_svc.add_github("decimozs/neovim-config").await,
        );

        println!("\n== Step 5: quick start, then remove");
        let qs = svc.quick_start("walkthrough-app").await;
        show("quick start", &qs);
        let qs = qs.unwrap();
        let dest = svc.paths.repos_dir.join("walkthrough-app");
        println!(
            "        files: {:?}",
            std::fs::read_dir(&dest)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        );
        svc.remove(&qs.id).unwrap();
        println!(
            "        after remove: folder exists={} README exists={} listed={}",
            dest.exists(),
            dest.join("README.md").exists(),
            svc.list().unwrap().iter().any(|p| p.id == qs.id)
        );
        assert!(dest.join(".git").exists());

        println!("\n== Step 6: edit base branch");
        sh(&main, &["branch", "develop"]);
        let id = svc
            .list()
            .unwrap()
            .into_iter()
            .find(|p| p.name == "no-remote-main")
            .unwrap()
            .id;
        for branch in ["develop", "does-not-exist", "main"] {
            match svc.set_base_branch(&id, branch).await {
                Ok(p) => println!("[{branch}] OK  base is now {}", p.default_branch),
                Err(e) => println!(
                    "[{branch}] ERR code={} details={}\n        message: {}",
                    e.code(),
                    e.payload()
                        .details
                        .map(|d| d.to_string())
                        .unwrap_or_default(),
                    e
                ),
            }
        }
        println!(
            "        final base = {}",
            svc.get(&id).unwrap().default_branch
        );
    }

    #[tokio::test]
    async fn github_actions_report_a_missing_gh() {
        let f = fixture();
        let err = f.svc.add_github("octo/hello").await.unwrap_err();
        assert_eq!(err.code(), "gh");
        assert_eq!(
            f.svc.add_github("not a repo").await.unwrap_err().code(),
            "invalid"
        );
        assert_eq!(f.svc.github_repos().await.unwrap_err().code(), "gh");
        assert!(!f.svc.paths.repos_dir.join("hello").exists());
    }
}
