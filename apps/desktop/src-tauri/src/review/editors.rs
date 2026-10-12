//! Code editors the user can open a workspace in.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Serialize;
use ts_rs::TS;

use crate::env::which_in;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct Editor {
    pub id: String,
    pub name: String,
}

struct Known {
    id: &'static str,
    name: &'static str,
    /// The command line tool the editor installs, when it does.
    cli: &'static str,
    /// The macOS application.
    app: &'static str,
}

const KNOWN: &[Known] = &[
    Known {
        id: "cursor",
        name: "Cursor",
        cli: "cursor",
        app: "Cursor.app",
    },
    Known {
        id: "vscode",
        name: "VS Code",
        cli: "code",
        app: "Visual Studio Code.app",
    },
];

fn app_dirs(home: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = vec![PathBuf::from("/Applications")];
    if let Some(home) = home {
        dirs.push(home.join("Applications"));
    }
    dirs
}

/// Editors found through `path` (the login shell's PATH) or in the applications folders.
pub fn detect(path: &str, home: Option<&Path>) -> Vec<Editor> {
    detect_in(path, &app_dirs(home))
}

pub fn detect_in(path: &str, app_dirs: &[PathBuf]) -> Vec<Editor> {
    KNOWN
        .iter()
        .filter(|k| {
            which_in(path, k.cli).is_some() || app_dirs.iter().any(|d| d.join(k.app).exists())
        })
        .map(|k| Editor {
            id: k.id.into(),
            name: k.name.into(),
        })
        .collect()
}

/// Opens `target` (a folder or a file) in the editor, preferring its command line tool.
pub fn open(editor_id: &str, path: &str, home: Option<&Path>, target: &Path) -> AppResult<()> {
    let known = KNOWN
        .iter()
        .find(|k| k.id == editor_id)
        .ok_or_else(|| AppError::Invalid(format!("\"{editor_id}\" is not a supported editor.")))?;
    let mut command = if let Some(cli) = which_in(path, known.cli) {
        let mut c = Command::new(cli);
        c.arg(target);
        c
    } else if app_dirs(home).iter().any(|d| d.join(known.app).exists()) {
        let mut c = Command::new("open");
        c.arg("-a")
            .arg(known.app.trim_end_matches(".app"))
            .arg(target);
        c
    } else {
        return Err(AppError::Invalid(format!(
            "{} is not installed.",
            known.name
        )));
    };
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editors_are_found_by_command_or_by_application() {
        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let tool = bin.join("code");
        std::fs::write(&tool, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let apps = tmp.path().join("Applications");
        std::fs::create_dir_all(apps.join("Cursor.app")).unwrap();

        let found = detect_in(bin.to_str().unwrap(), std::slice::from_ref(&apps));
        let ids: Vec<_> = found.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["cursor", "vscode"]);
        assert!(detect_in("/nonexistent", &[tmp.path().join("empty")]).is_empty());
    }

    #[test]
    fn an_unknown_or_missing_editor_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(
            open("emacs", "/nonexistent", None, tmp.path())
                .unwrap_err()
                .code(),
            "invalid"
        );
        let err = open("cursor", "/nonexistent", Some(tmp.path()), tmp.path());
        // Only fails when Cursor is not installed on the machine running the test.
        if let Err(e) = err {
            assert_eq!(e.code(), "invalid");
        }
    }
}
