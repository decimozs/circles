use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

/// Every location Amplify reads or writes outside a repository.
#[derive(Debug, Clone)]
pub struct AppPaths {
    /// `~/Library/Application Support/amplify`
    pub data_dir: PathBuf,
    pub db_file: PathBuf,
    /// User level `settings.toml`.
    pub settings_file: PathBuf,
    pub logs_dir: PathBuf,
    /// Local per-project settings overrides live under `<projects_dir>/<project id>/settings.toml`.
    pub projects_dir: PathBuf,
    /// `~/amplify`
    pub home_dir: PathBuf,
    pub workspaces_dir: PathBuf,
    pub repos_dir: PathBuf,
}

/// What the app was called before it was Amplify. Folders and files of that name are moved or still
/// read, so nothing a person made is lost.
const EARLIER: &str = "circles";

/// Moves the data folder of the earlier name to the new one, with its database, when the new one is not
/// there yet. When it cannot be moved, the earlier folder keeps being used.
fn data_folder(base: &Path) -> PathBuf {
    let new = base.join("amplify");
    let old = base.join(EARLIER);
    if new.exists() || !old.is_dir() {
        return new;
    }
    if std::fs::rename(&old, &new).is_err() {
        return old;
    }
    for suffix in ["", "-wal", "-shm"] {
        let from = new.join(format!("{EARLIER}.db{suffix}"));
        let to = new.join(format!("amplify.db{suffix}"));
        if from.exists() && !to.exists() {
            let _ = std::fs::rename(from, to);
        }
    }
    new
}

/// The folder for workspaces and clones: `~/amplify`, or `~/circles` where that is what a person
/// already has, since the paths of their workspaces are stored in it.
fn home_folder(home: &Path) -> PathBuf {
    let new = home.join("amplify");
    let old = home.join(EARLIER);
    if !new.exists() && old.is_dir() {
        old
    } else {
        new
    }
}

/// The data folder an old installation left, when it could not be moved.
fn database_name(dir: &Path) -> &'static str {
    if dir.join("amplify.db").exists() || !dir.join("circles.db").exists() {
        "amplify.db"
    } else {
        "circles.db"
    }
}

impl AppPaths {
    pub fn resolve() -> AppResult<Self> {
        let given = |new: &str, old: &str| {
            std::env::var_os(new)
                .or_else(|| std::env::var_os(old))
                .map(PathBuf::from)
        };
        let data_root = given("AMPLIFY_DATA_DIR", "CIRCLES_DATA_DIR")
            .or_else(|| dirs::data_dir().map(|d| data_folder(&d)))
            .ok_or_else(|| {
                AppError::Env("cannot locate the application support directory".into())
            })?;
        let home = given("AMPLIFY_HOME", "CIRCLES_HOME")
            .or_else(|| dirs::home_dir().map(|d| home_folder(&d)))
            .ok_or_else(|| AppError::Env("cannot locate the home directory".into()))?;
        let mut paths = Self::from_roots(&data_root, &home);
        paths.db_file = data_root.join(database_name(&data_root));
        Ok(paths)
    }

    pub fn from_roots(data_dir: &Path, home_dir: &Path) -> Self {
        Self {
            data_dir: data_dir.to_path_buf(),
            db_file: data_dir.join("amplify.db"),
            settings_file: data_dir.join("settings.toml"),
            logs_dir: data_dir.join("logs"),
            projects_dir: data_dir.join("projects"),
            home_dir: home_dir.to_path_buf(),
            workspaces_dir: home_dir.join("workspaces"),
            repos_dir: home_dir.join("repos"),
        }
    }

    pub fn ensure(&self) -> AppResult<()> {
        for dir in [
            &self.data_dir,
            &self.logs_dir,
            &self.projects_dir,
            &self.workspaces_dir,
            &self.repos_dir,
        ] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }

    /// Local override file for one project.
    pub fn local_project_settings(&self, project_id: &str) -> PathBuf {
        self.projects_dir.join(project_id).join("settings.toml")
    }
}

/// The folder Amplify keeps in a repository: `.amplify`, or `.circles` in a repository that has only
/// that, from before the rename.
pub fn repo_folder(repo_root: &Path) -> PathBuf {
    let new = repo_root.join(".amplify");
    let old = repo_root.join(format!(".{EARLIER}"));
    if !new.exists() && old.is_dir() {
        old
    } else {
        new
    }
}

/// Project level shareable settings file inside a repository.
pub fn project_settings_file(repo_root: &Path) -> PathBuf {
    repo_folder(repo_root).join("settings.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_matches_the_spec() {
        let p = AppPaths::from_roots(Path::new("/data/amplify"), Path::new("/home/u/amplify"));
        assert_eq!(p.db_file, Path::new("/data/amplify/amplify.db"));
        assert_eq!(p.settings_file, Path::new("/data/amplify/settings.toml"));
        assert_eq!(p.workspaces_dir, Path::new("/home/u/amplify/workspaces"));
        assert_eq!(
            p.local_project_settings("abc"),
            Path::new("/data/amplify/projects/abc/settings.toml")
        );
        assert_eq!(
            project_settings_file(Path::new("/repo")),
            Path::new("/repo/.amplify/settings.toml")
        );
    }

    #[test]
    fn the_earlier_data_folder_is_moved_with_its_database() {
        let tmp = tempfile::tempdir().unwrap();
        let old = tmp.path().join("circles");
        std::fs::create_dir_all(&old).unwrap();
        for f in ["circles.db", "circles.db-wal", "settings.toml"] {
            std::fs::write(old.join(f), f).unwrap();
        }
        let dir = data_folder(tmp.path());
        assert_eq!(dir, tmp.path().join("amplify"));
        assert!(!old.exists());
        assert_eq!(
            std::fs::read_to_string(dir.join("amplify.db")).unwrap(),
            "circles.db"
        );
        assert!(dir.join("amplify.db-wal").exists() && !dir.join("circles.db").exists());
        assert!(dir.join("settings.toml").exists());
        assert_eq!(database_name(&dir), "amplify.db");
        // A second start finds it there and changes nothing.
        assert_eq!(data_folder(tmp.path()), dir);
    }

    #[test]
    fn a_new_install_uses_the_new_names_and_both_folders_present_keep_the_new() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(data_folder(tmp.path()), tmp.path().join("amplify"));
        assert_eq!(home_folder(tmp.path()), tmp.path().join("amplify"));
        assert_eq!(database_name(&tmp.path().join("amplify")), "amplify.db");
        std::fs::create_dir_all(tmp.path().join("circles")).unwrap();
        std::fs::create_dir_all(tmp.path().join("amplify")).unwrap();
        assert_eq!(data_folder(tmp.path()), tmp.path().join("amplify"));
        assert!(
            tmp.path().join("circles").exists(),
            "not touched when both exist"
        );
        assert_eq!(home_folder(tmp.path()), tmp.path().join("amplify"));
    }

    #[test]
    fn workspaces_stay_where_the_earlier_version_put_them() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("circles/workspaces")).unwrap();
        assert_eq!(home_folder(tmp.path()), tmp.path().join("circles"));
    }

    #[test]
    fn a_repository_keeps_using_its_earlier_folder_until_it_has_the_new_one() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(repo_folder(tmp.path()), tmp.path().join(".amplify"));
        std::fs::create_dir_all(tmp.path().join(".circles")).unwrap();
        assert_eq!(
            project_settings_file(tmp.path()),
            tmp.path().join(".circles/settings.toml")
        );
        std::fs::create_dir_all(tmp.path().join(".amplify")).unwrap();
        assert_eq!(
            project_settings_file(tmp.path()),
            tmp.path().join(".amplify/settings.toml")
        );
    }

    #[test]
    fn ensure_creates_directories() {
        let tmp = tempfile::tempdir().unwrap();
        let p = AppPaths::from_roots(&tmp.path().join("data"), &tmp.path().join("home"));
        p.ensure().unwrap();
        assert!(p.logs_dir.is_dir());
        assert!(p.workspaces_dir.is_dir());
        assert!(p.repos_dir.is_dir());
    }
}
