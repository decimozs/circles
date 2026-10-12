//! The first-run Setup screen: what the app needs, what it would like, and the permissions macOS
//! may ask about, each with its state and an action. Nothing here changes the Mac without the
//! person pressing the button for it.

use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;
use tauri::State;
use ts_rs::TS;

use super::diagnostics::{dependencies, Dependency};
use crate::error::AppError;
use crate::ipc::IpcResult;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[ts(export)]
pub struct SetupAction {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[ts(export)]
pub struct SetupItem {
    pub id: String,
    pub title: String,
    pub why: String,
    /// `ready`, `needs_you`, `optional` (not done, and fine without) or `unknown` (only learned by
    /// trying).
    pub state: String,
    /// Setup cannot finish while a required item is not ready.
    pub required: bool,
    pub detail: Option<String>,
    /// A command to run in a terminal.
    pub fix: Option<String>,
    pub action: Option<SetupAction>,
}

#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[ts(export)]
pub struct ActionResult {
    pub ok: bool,
    pub message: String,
}

fn item(id: &str, title: &str, why: &str, required: bool) -> SetupItem {
    SetupItem {
        id: id.into(),
        title: title.into(),
        why: why.into(),
        state: "optional".into(),
        required,
        detail: None,
        fix: None,
        action: None,
    }
}

fn action(id: &str, label: &str) -> Option<SetupAction> {
    Some(SetupAction {
        id: id.into(),
        label: label.into(),
    })
}

fn dep<'a>(deps: &'a [Dependency], name: &str) -> Option<&'a Dependency> {
    deps.iter().find(|d| d.name == name)
}

/// The rows, from what was found. Pure, so every state is testable.
pub fn build_items(
    deps: &[Dependency],
    model_found: bool,
    login_item: bool,
    menu_bar: bool,
) -> Vec<SetupItem> {
    let mut out = Vec::new();

    let mut git = item(
        "git",
        "git",
        "Every workspace is a git worktree. It comes with the Xcode command line tools.",
        true,
    );
    match dep(deps, "git") {
        Some(d) if d.ok => {
            git.state = "ready".into();
            git.detail = d.version.clone();
        }
        other => {
            git.state = "needs_you".into();
            git.detail = other.and_then(|d| d.problem.clone());
            git.fix = Some("xcode-select --install".into());
            git.action = action("install_git", "Install");
        }
    }
    out.push(git);

    let claude = dep(deps, "claude");
    let mut installed = item(
        "claude_installed",
        "Claude Code",
        "The agents are Claude Code. Circles runs the `claude` program on this Mac.",
        true,
    );
    let found = claude.is_some_and(|d| d.found);
    if found {
        installed.state = "ready".into();
        installed.detail = claude.and_then(|d| d.version.clone());
    } else {
        installed.state = "needs_you".into();
        installed.detail = Some("`claude` was not found.".into());
        installed.fix = Some("curl -fsSL https://claude.ai/install.sh | bash".into());
    }
    out.push(installed);

    let mut signed = item(
        "claude_signed_in",
        "Signed in to Claude",
        "Agents need your Claude account. Sign in once in a terminal.",
        true,
    );
    match claude {
        Some(d) if d.ok => signed.state = "ready".into(),
        Some(d) if d.found => {
            signed.state = "needs_you".into();
            signed.detail = d.problem.clone();
            signed.fix = Some("claude".into());
            signed.action = action("open_terminal_claude", "Open Terminal");
        }
        _ => {
            signed.state = "needs_you".into();
            signed.detail = Some("Install Claude Code first.".into());
        }
    }
    out.push(signed);

    let mut gh = item(
        "gh",
        "GitHub CLI",
        "For pull requests, checks and cloning. Without it everything else works.",
        false,
    );
    match dep(deps, "gh") {
        Some(d) if d.ok => {
            gh.state = "ready".into();
            gh.detail = d.version.clone();
        }
        Some(d) => {
            gh.state = "optional".into();
            gh.detail = d.problem.clone();
            gh.fix = d.fix.clone();
        }
        None => {}
    }
    out.push(gh);

    let mut notifications = item(
        "notifications",
        "Notifications",
        "To tell you when an agent or a Composer waits for you while Circles is in the background.",
        false,
    );
    notifications.state = "unknown".into();
    notifications.detail = Some(
        "Send a test. If nothing shows, allow Circles in System Settings, Notifications.".into(),
    );
    notifications.action = action("test_notification", "Send a test");
    out.push(notifications);

    let mut folders = item(
        "folders",
        "Folders where your code lives",
        "macOS asks before an app reads Desktop, Documents or Downloads. Doing it now means it does not ask in the middle of a task.",
        false,
    );
    folders.state = "unknown".into();
    folders.action = action("probe_folders", "Allow access");
    out.push(folders);

    let mut disk = item(
        "full_disk",
        "Full Disk Access",
        "Not needed. It lets Circles read anywhere without asking, which is more than it needs.",
        false,
    );
    disk.state = "optional".into();
    disk.action = action("open_full_disk", "Open System Settings");
    out.push(disk);

    let mut keychain = item(
        "keychain",
        "Keychain",
        "Connections such as Jira keep their login in your Keychain. macOS may ask once. Choose Always Allow.",
        false,
    );
    keychain.state = "optional".into();
    out.push(keychain);

    let mut login = item(
        "login",
        "Open at login and the menu bar",
        "For Composers that run on a schedule or a trigger while you are away.",
        false,
    );
    login.state = if login_item || menu_bar {
        "ready"
    } else {
        "optional"
    }
    .into();
    login.detail = Some(format!(
        "Open at login: {}. Menu bar: {}.",
        if login_item { "on" } else { "off" },
        if menu_bar { "on" } else { "off" }
    ));
    out.push(login);

    let mut model = item(
        "model",
        "Built-in model",
        "Names your chats and writes examples. It is inside Circles and runs on this Mac.",
        false,
    );
    if model_found {
        model.state = "ready".into();
        model.action = action("test_model", "Test");
    } else {
        model.state = "optional".into();
        model.detail = Some("The model file was not found in this copy of Circles.".into());
    }
    out.push(model);

    out
}

fn protected_folders() -> Vec<(String, PathBuf)> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    ["Desktop", "Documents", "Downloads"]
        .iter()
        .map(|n| ((*n).to_string(), home.join(n)))
        .collect()
}

#[tauri::command]
pub async fn setup_status(state: State<'_, AppState>) -> IpcResult<Vec<SetupItem>> {
    let deps = dependencies(&state).await;
    let settings = crate::commands::app::settings_layers(&state, None)
        .ok()
        .and_then(|layers| crate::settings::load_merged(&layers).ok())
        .map(|p| p.settings.composers)
        .unwrap_or_default();
    Ok(build_items(
        &deps,
        state.smart.embedded().is_some(),
        settings.start_at_login == Some(true),
        settings.menu_bar == Some(true),
    ))
}

fn open_quiet(args: &[&str]) -> Result<(), AppError> {
    std::process::Command::new("open")
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    Ok(())
}

/// Does what a row's button says. Returns what to tell the person.
#[tauri::command]
pub async fn setup_action(state: State<'_, AppState>, id: String) -> IpcResult<ActionResult> {
    let done = |message: &str| ActionResult {
        ok: true,
        message: message.to_string(),
    };
    match id.as_str() {
        "install_git" => {
            std::process::Command::new("xcode-select")
                .arg("--install")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()?;
            Ok(done(
                "macOS opened its installer. Finish it, then check again.",
            ))
        }
        "open_terminal_claude" => {
            std::process::Command::new("osascript")
                .args([
                    "-e",
                    "tell application \"Terminal\" to activate",
                    "-e",
                    "tell application \"Terminal\" to do script \"claude\"",
                ])
                .stdin(std::process::Stdio::null())
                .spawn()?;
            Ok(done(
                "Terminal is open with `claude`. Sign in there, then check again.",
            ))
        }
        "test_notification" => {
            let route = crate::system::notify_via("Circles", "This is a test notification.")?;
            Ok(done(match route {
                crate::system::NotifyRoute::Native => {
                    "Sent from Circles. If you saw it, notifications work."
                }
                crate::system::NotifyRoute::Script => {
                    "Sent through a script, so it may show as Script Editor. Allow Script Editor, or use the installed app, to see it as Circles."
                }
            }))
        }
        "probe_folders" => {
            // Reading the folder is what makes macOS ask, so it is done here, now.
            let folders = protected_folders();
            let probed = tokio::task::spawn_blocking(move || {
                folders
                    .into_iter()
                    .map(|(name, path)| {
                        let state = match std::fs::read_dir(&path) {
                            Ok(_) => "allowed",
                            Err(e) if e.kind() == std::io::ErrorKind::NotFound => "not there",
                            Err(_) => "not allowed",
                        };
                        format!("{name}: {state}")
                    })
                    .collect::<Vec<_>>()
            });
            let lines = tokio::time::timeout(Duration::from_secs(120), probed)
                .await
                .map_err(|_| AppError::Invalid("macOS did not get an answer in time.".into()))?
                .map_err(|e| AppError::Internal(e.to_string()))?;
            Ok(ActionResult {
                ok: !lines.iter().any(|l| l.ends_with("not allowed")),
                message: lines.join(". ") + ".",
            })
        }
        "open_full_disk" => {
            open_quiet(&[
                "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles",
            ])?;
            Ok(done("System Settings is open. You can leave this off."))
        }
        "test_model" => {
            let Some(model) = state.smart.embedded() else {
                return Ok(ActionResult {
                    ok: false,
                    message: "The model file was not found.".into(),
                });
            };
            let start = std::time::Instant::now();
            let title = crate::smart::title_for(
                model.as_ref(),
                "fix the failing test in the checkout flow and open a pull request",
            )
            .await;
            Ok(match title {
                Some(t) => ActionResult {
                    ok: true,
                    message: format!("“{t}” in {} ms.", start.elapsed().as_millis()),
                },
                None => ActionResult {
                    ok: false,
                    message: "The model did not answer.".into(),
                },
            })
        }
        other => Err(AppError::Invalid(format!(
            "\"{other}\" is not a setup action."
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dep(name: &str, found: bool, ok: bool, problem: Option<&str>) -> Dependency {
        Dependency {
            name: name.into(),
            found,
            path: None,
            version: found.then(|| "1.0".to_string()),
            ok,
            problem: problem.map(str::to_string),
            fix: None,
        }
    }

    fn get<'a>(items: &'a [SetupItem], id: &str) -> &'a SetupItem {
        items.iter().find(|i| i.id == id).unwrap()
    }

    #[test]
    fn a_ready_machine_has_every_required_row_ready() {
        let deps = [
            dep("git", true, true, None),
            dep("gh", true, true, None),
            dep("claude", true, true, None),
        ];
        let items = build_items(&deps, true, false, false);
        for id in ["git", "claude_installed", "claude_signed_in"] {
            assert_eq!(get(&items, id).state, "ready", "{id}");
            assert!(get(&items, id).required);
        }
        assert_eq!(get(&items, "gh").state, "ready");
        assert_eq!(get(&items, "model").state, "ready");
    }

    #[test]
    fn a_missing_claude_blocks_with_a_command_to_run() {
        let deps = [
            dep("git", true, true, None),
            dep("claude", false, false, Some("not found")),
        ];
        let items = build_items(&deps, false, false, false);
        let installed = get(&items, "claude_installed");
        assert_eq!(installed.state, "needs_you");
        assert!(installed
            .fix
            .as_deref()
            .unwrap()
            .contains("claude.ai/install"));
        // Signing in cannot be asked for before it is installed, and has no button yet.
        let signed = get(&items, "claude_signed_in");
        assert_eq!(signed.state, "needs_you");
        assert!(signed.action.is_none());
    }

    #[test]
    fn claude_found_but_not_signed_in_offers_the_terminal() {
        let deps = [
            dep("git", true, true, None),
            dep("claude", true, false, Some("Not logged in")),
        ];
        let items = build_items(&deps, true, false, false);
        assert_eq!(get(&items, "claude_installed").state, "ready");
        let signed = get(&items, "claude_signed_in");
        assert_eq!(signed.state, "needs_you");
        assert_eq!(signed.detail.as_deref(), Some("Not logged in"));
        assert_eq!(signed.action.as_ref().unwrap().id, "open_terminal_claude");
    }

    #[test]
    fn git_missing_offers_the_installer() {
        let deps = [
            dep("git", false, false, Some("git was not found.")),
            dep("claude", true, true, None),
        ];
        let git = &build_items(&deps, true, false, false)[0];
        assert_eq!(git.state, "needs_you");
        assert_eq!(git.action.as_ref().unwrap().id, "install_git");
        assert_eq!(git.fix.as_deref(), Some("xcode-select --install"));
    }

    #[test]
    fn what_is_not_required_never_blocks() {
        let deps = [
            dep("git", true, true, None),
            dep("claude", true, true, None),
            dep("gh", false, false, Some("no gh")),
        ];
        let items = build_items(&deps, false, false, false);
        let required_not_ready = items
            .iter()
            .filter(|i| i.required && i.state != "ready")
            .count();
        assert_eq!(required_not_ready, 0);
        assert!(!get(&items, "gh").required);
        assert!(!get(&items, "full_disk").required);
        assert_eq!(get(&items, "model").state, "optional");
    }

    #[test]
    fn the_login_row_reflects_the_settings() {
        let deps = [
            dep("git", true, true, None),
            dep("claude", true, true, None),
        ];
        let off = build_items(&deps, true, false, false);
        assert_eq!(get(&off, "login").state, "optional");
        let on = build_items(&deps, true, true, false);
        assert_eq!(get(&on, "login").state, "ready");
        assert!(get(&on, "login")
            .detail
            .as_deref()
            .unwrap()
            .contains("Open at login: on"));
    }
}
