use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

/// Every location Circles reads or writes outside a repository.
#[derive(Debug, Clone)]
pub struct AppPaths {
    /// `~/Library/Application Support/circles`
    pub data_dir: PathBuf,
    pub db_file: PathBuf,
    /// User level `settings.toml`.
    pub settings_file: PathBuf,
    pub logs_dir: PathBuf,
    /// Local per-project settings overrides live under `<projects_dir>/<project id>/settings.toml`.
    pub projects_dir: PathBuf,
    /// `~/circles`
    pub home_dir: PathBuf,
    pub workspaces_dir: PathBuf,
    pub repos_dir: PathBuf,
}

impl AppPaths {
    pub fn resolve() -> AppResult<Self> {
        let data_root = std::env::var_os("CIRCLES_DATA_DIR")
            .map(PathBuf::from)
            .or_else(|| dirs::data_dir().map(|d| d.join("circles")))
            .ok_or_else(|| {
                AppError::Env("cannot locate the application support directory".into())
            })?;
        let home = std::env::var_os("CIRCLES_HOME")
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|d| d.join("circles")))
            .ok_or_else(|| AppError::Env("cannot locate the home directory".into()))?;
        Ok(Self::from_roots(&data_root, &home))
    }

    pub fn from_roots(data_dir: &Path, home_dir: &Path) -> Self {
        Self {
            data_dir: data_dir.to_path_buf(),
            db_file: data_dir.join("circles.db"),
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

/// Project level shareable settings file inside a repository.
pub fn project_settings_file(repo_root: &Path) -> PathBuf {
    repo_root.join(".circles").join("settings.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_matches_the_spec() {
        let p = AppPaths::from_roots(Path::new("/data/circles"), Path::new("/home/u/circles"));
        assert_eq!(p.db_file, Path::new("/data/circles/circles.db"));
        assert_eq!(p.settings_file, Path::new("/data/circles/settings.toml"));
        assert_eq!(p.workspaces_dir, Path::new("/home/u/circles/workspaces"));
        assert_eq!(
            p.local_project_settings("abc"),
            Path::new("/data/circles/projects/abc/settings.toml")
        );
        assert_eq!(
            project_settings_file(Path::new("/repo")),
            Path::new("/repo/.circles/settings.toml")
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
