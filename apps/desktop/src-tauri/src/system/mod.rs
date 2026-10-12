//! Small things the settings pages do on the machine: notify, play a sound, export logs, and start
//! over.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::error::{AppError, AppResult};
use crate::paths::AppPaths;

/// The sound played with a notification: one every Mac has.
pub const SOUND_FILE: &str = "/System/Library/Sounds/Glass.aiff";

/// The word that has to be typed to reset the app's data.
pub const RESET_WORD: &str = "reset";

fn applescript_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' | '\r' => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The AppleScript that shows a notification, with the text made safe to put in it.
pub fn notification_script(title: &str, body: &str) -> String {
    format!(
        "display notification {} with title {}",
        applescript_text(body),
        applescript_text(title)
    )
}

fn spawn_quiet(program: &str, args: &[&str]) -> AppResult<()> {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}

/// The app, once it exists, so a notification can come from Circles itself.
static APP: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();

pub fn set_app(app: tauri::AppHandle) {
    let _ = APP.set(app);
}

/// How a notification was sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyRoute {
    /// From Circles, with its own name and icon.
    Native,
    /// Through a script. It shows as coming from Script Editor.
    Script,
}

/// Shows a notification, from Circles when it can and through a script when it cannot.
pub fn notify_via(title: &str, body: &str) -> AppResult<NotifyRoute> {
    use tauri_plugin_notification::NotificationExt;
    if let Some(app) = APP.get() {
        let shown = app.notification().builder().title(title).body(body).show();
        match shown {
            Ok(()) => return Ok(NotifyRoute::Native),
            Err(e) => tracing::debug!(error = %e, "native notification failed, using a script"),
        }
    }
    spawn_quiet("osascript", &["-e", &notification_script(title, body)])?;
    Ok(NotifyRoute::Script)
}

pub fn notify(title: &str, body: &str) -> AppResult<()> {
    notify_via(title, body).map(|_| ())
}

pub fn play_sound() -> AppResult<()> {
    if !Path::new(SOUND_FILE).exists() {
        return Err(AppError::NotFound(
            "The notification sound is not on this Mac.".into(),
        ));
    }
    spawn_quiet("afplay", &[SOUND_FILE])
}

/// Shows a file or folder in Finder.
pub fn reveal(path: &Path) -> AppResult<()> {
    spawn_quiet("open", &["-R", &path.to_string_lossy()])
}

/// Opens a file in the default text editor.
pub fn open_text(path: &Path) -> AppResult<()> {
    spawn_quiet("open", &["-t", &path.to_string_lossy()])
}

/// Copies the log files into a new folder in `into` and returns it.
pub fn export_logs(paths: &AppPaths, into: &Path, stamp: &str) -> AppResult<PathBuf> {
    let folder = into.join(format!("circles-logs-{stamp}"));
    std::fs::create_dir_all(&folder)?;
    if paths.logs_dir.is_dir() {
        for entry in std::fs::read_dir(&paths.logs_dir)?.flatten() {
            if entry.file_type().is_ok_and(|t| t.is_file()) {
                std::fs::copy(entry.path(), folder.join(entry.file_name()))?;
            }
        }
    }
    Ok(folder)
}

/// Deletes what the app has stored about workspaces, chats, projects and terminals, so it starts
/// as new. The user's own settings, the logs, the repositories and the workspace folders on disk are
/// left alone. `confirm` must be the reset word, so it cannot happen by accident.
pub fn reset_data(paths: &AppPaths, confirm: &str) -> AppResult<()> {
    if confirm.trim() != RESET_WORD {
        return Err(AppError::Invalid(format!(
            "Type \"{RESET_WORD}\" to confirm."
        )));
    }
    for suffix in ["", "-wal", "-shm"] {
        let mut name = paths.db_file.as_os_str().to_os_string();
        name.push(suffix);
        let file = PathBuf::from(name);
        match std::fs::remove_file(&file) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    for folder in [
        paths.data_dir.join("scrollback"),
        paths.data_dir.join("context"),
        paths.projects_dir.clone(),
    ] {
        match std::fs::remove_dir_all(&folder) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(root: &Path) -> AppPaths {
        AppPaths {
            data_dir: root.to_path_buf(),
            db_file: root.join("circles.db"),
            settings_file: root.join("settings.toml"),
            logs_dir: root.join("logs"),
            projects_dir: root.join("projects"),
            home_dir: root.join("home"),
            workspaces_dir: root.join("home/workspaces"),
            repos_dir: root.join("home/repos"),
        }
    }

    #[test]
    fn notification_text_cannot_break_out_of_the_script() {
        assert_eq!(
            notification_script("Circles", "Done"),
            "display notification \"Done\" with title \"Circles\""
        );
        let script = notification_script(
            "A \"title\"",
            "back\\slash\nand \"quotes\"; do shell script \"rm -rf /\"",
        );
        assert!(script.contains("\\\"title\\\""));
        assert!(
            script.contains("back\\\\slash and \\\"quotes\\\"; do shell script \\\"rm -rf /\\\""),
            "{script}"
        );
        assert!(!script.contains('\n'));
    }

    #[test]
    fn logs_are_copied_into_a_new_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = paths(tmp.path());
        std::fs::create_dir_all(&paths.logs_dir).unwrap();
        std::fs::write(paths.logs_dir.join("circles.log.2026-01-01"), "line").unwrap();
        std::fs::create_dir_all(paths.logs_dir.join("subfolder")).unwrap();
        let downloads = tmp.path().join("Downloads");
        std::fs::create_dir_all(&downloads).unwrap();
        let folder = export_logs(&paths, &downloads, "now").unwrap();
        assert_eq!(folder, downloads.join("circles-logs-now"));
        assert_eq!(
            std::fs::read_to_string(folder.join("circles.log.2026-01-01")).unwrap(),
            "line"
        );
        assert!(!folder.join("subfolder").exists());
        // No logs yet is still fine.
        let empty = tempfile::tempdir().unwrap();
        assert!(
            export_logs(&self::tests::paths(empty.path()), &downloads, "later")
                .unwrap()
                .is_dir()
        );
    }

    #[test]
    fn a_reset_needs_the_word_and_keeps_what_is_the_users() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = paths(tmp.path());
        for file in [
            "circles.db",
            "circles.db-wal",
            "circles.db-shm",
            "settings.toml",
        ] {
            std::fs::write(tmp.path().join(file), "x").unwrap();
        }
        for folder in [
            "scrollback",
            "context",
            "projects/p1",
            "logs",
            "home/workspaces/roster/oslo",
        ] {
            std::fs::create_dir_all(tmp.path().join(folder)).unwrap();
            std::fs::write(tmp.path().join(folder).join("f.txt"), "x").unwrap();
        }

        assert_eq!(reset_data(&paths, "").unwrap_err().code(), "invalid");
        assert_eq!(reset_data(&paths, "yes").unwrap_err().code(), "invalid");
        assert!(
            tmp.path().join("circles.db").exists(),
            "nothing was deleted without the word"
        );

        reset_data(&paths, " reset ").unwrap();
        for gone in [
            "circles.db",
            "circles.db-wal",
            "circles.db-shm",
            "scrollback",
            "context",
            "projects",
        ] {
            assert!(!tmp.path().join(gone).exists(), "{gone} should be gone");
        }
        assert!(
            tmp.path().join("settings.toml").exists(),
            "the user's settings stay"
        );
        assert!(tmp.path().join("logs/f.txt").exists(), "the logs stay");
        assert!(
            tmp.path()
                .join("home/workspaces/roster/oslo/f.txt")
                .exists(),
            "the workspaces on disk stay"
        );
        reset_data(&paths, "reset").unwrap();
    }
}
