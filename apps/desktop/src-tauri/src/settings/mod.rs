//! Layered `settings.toml` loading.
//!
//! Three levels, lowest to highest precedence:
//! 1. user:          `<app data>/settings.toml`
//! 2. project:       `<repo>/.circles/settings.toml` (shareable, committed)
//! 3. local project: `<app data>/projects/<project id>/settings.toml`
//!
//! Tables merge key by key. Any other value, including arrays, is replaced by the higher level.
//! Keys this version does not know are preserved in `Settings::extra`.

pub mod edit;
pub mod watch;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct Scripts {
    pub setup: Option<String>,
    /// Either a single command or a table of named run scripts. Refined in phase 7.
    #[ts(type = "unknown | null")]
    pub run: Option<toml::Value>,
    pub archive: Option<String>,
    /// `concurrent` or `nonconcurrent`.
    pub run_mode: Option<String>,
}

/// Text added to the prompts Circles sends for a kind of task, so a repository can say how it
/// wants reviews, pull requests and fixes done.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct Instructions {
    /// Added to every prompt below.
    pub general: Option<String>,
    pub review: Option<String>,
    /// How to write the title and description of a pull request.
    pub pr: Option<String>,
    pub fix_errors: Option<String>,
    pub conflicts: Option<String>,
    pub branch_rename: Option<String>,
    /// How commit messages should look.
    pub commits: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct GitSettings {
    /// `squash` (the default), `merge` or `rebase`.
    pub merge_method: Option<String>,
    /// Put in front of generated branch names, in place of your GitHub login.
    pub branch_prefix: Option<String>,
    /// How this project names its branches, for example `{ticket}-{slug}` or `feature/{slug}`.
    /// `{type}` is feat, fix, chore and so on, `{slug}` lowercase words joined by dashes and
    /// `{ticket}` an issue key such as PROJ-123. Without one, `{type}/{slug}`.
    pub branch_pattern: Option<String>,
}

/// How the chat behaves for the person using it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct General {
    /// `enter` sends with Enter, `mod_enter` with Cmd+Enter.
    pub send_key: Option<String>,
    /// What a message sent while the agent works does: `steer` or `queue`.
    pub follow_up: Option<String>,
    /// A message when an agent finishes or needs you, while the app is in the background.
    pub notifications: Option<bool>,
    /// A sound with it.
    pub sound: Option<bool>,
    /// Pasted text over 5000 characters becomes an attachment.
    pub long_text_to_attachment: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct Appearance {
    /// `system`, `light` or `dark`.
    pub theme: Option<String>,
    /// `small`, `medium` or `large`.
    pub font_size: Option<String>,
    /// `comfortable` or `compact`.
    pub density: Option<String>,
}

/// What a new chat starts with.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct Models {
    pub model: Option<String>,
    pub effort: Option<String>,
    /// New chats start in plan mode.
    pub plan_by_default: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct Agents {
    /// A `claude` to use instead of the one found on the PATH.
    pub claude_path: Option<String>,
    /// `manual`, `accept_edits`, `plan` or `bypass`.
    pub permission_mode: Option<String>,
    /// Added to every Claude Code start.
    #[serde(default)]
    pub extra_args: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct Advanced {
    /// `error`, `warn`, `info`, `debug` or `trace`. Takes effect the next time the app starts.
    pub log_level: Option<String>,
    /// How many days checkpoints are kept, from 1 to 365. 30 when not set.
    pub checkpoint_days: Option<u32>,
}

/// How Composers behave while nobody is watching.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct ComposerSettings {
    /// Runs that a schedule or a trigger may start in 24 hours, from 1 to 500. 20 when not set.
    pub daily_run_cap: Option<u32>,
    /// Keep Circles running in the menu bar after its window is closed. Takes effect the next
    /// time Circles starts.
    pub menu_bar: Option<bool>,
    /// Keep the Mac awake while a run started by a schedule or trigger is going.
    pub keep_awake: Option<bool>,
    /// Open Circles when you log in.
    pub start_at_login: Option<bool>,
    /// A notification when something waits for you, while Circles is in the background. On
    /// unless switched off.
    pub notify_waiting: Option<bool>,
}

/// Titles and suggestions made by Claude Haiku through Claude Code, which spends the Claude
/// subscription and sends the text to Anthropic.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct SmartSettings {
    /// A model names new chats. On unless switched off.
    pub titles: Option<bool>,
    /// A model suggests the next message as the placeholder. Off unless switched on.
    pub suggestions: Option<bool>,
    /// A model writes an example of what to ask a Composer, from its instructions, as the
    /// placeholder of its first message box. On unless switched off.
    pub first_prompt: Option<bool>,
}

/// Cards that agents show in the chat to ask for choices and details.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct GenuiSettings {
    /// Composers may ask with cards. On unless switched off.
    pub composers: Option<bool>,
    /// Ordinary chats may ask with cards. Off unless switched on.
    pub chats: Option<bool>,
}

/// Side questions: quick questions about a chat, answered in a pane above its message box.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct SideSettings {
    /// `haiku` (fast and cheap, the default) or `chat` (the model the chat uses, which can read
    /// the same prompt cache).
    pub model: Option<String>,
    /// Let a side question read files in the chat's folder. Off: it answers from the
    /// conversation only. It can never change anything either way.
    pub read_files: Option<bool>,
}

/// Outside services. Their logins are not here: the token is in the Keychain.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct IntegrationSettings {
    /// Let ordinary chats use Jira, once it is connected. On unless switched off.
    pub jira_in_chats: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct Settings {
    #[serde(default)]
    pub instructions: Instructions,
    #[serde(default)]
    pub git: GitSettings,
    #[serde(default)]
    pub general: General,
    #[serde(default)]
    pub appearance: Appearance,
    #[serde(default)]
    pub models: Models,
    #[serde(default)]
    pub agents: Agents,
    #[serde(default)]
    pub advanced: Advanced,
    #[serde(default)]
    pub composers: ComposerSettings,
    #[serde(default)]
    pub integrations: IntegrationSettings,
    #[serde(default)]
    pub smart: SmartSettings,
    #[serde(default)]
    pub side: SideSettings,
    #[serde(default)]
    pub genui: GenuiSettings,
    /// Switches for features that are not finished.
    #[serde(default)]
    pub experimental: BTreeMap<String, bool>,
    #[serde(default)]
    pub scripts: Scripts,
    #[serde(default)]
    pub environment_variables: BTreeMap<String, String>,
    /// Gitignore style patterns for ignored files copied into new workspaces (for example `.env`).
    #[serde(default)]
    pub files_to_copy: Vec<String>,
    /// Keys this version does not know about. Kept so newer files are not corrupted.
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Clone, Copy, Serialize, TS, PartialEq, Eq)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    User,
    Project,
    LocalProject,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct LayerInfo {
    pub scope: Scope,
    pub path: String,
    pub exists: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct SettingsPayload {
    pub settings: Settings,
    /// Lowest to highest precedence.
    pub layers: Vec<LayerInfo>,
}

/// Reads one layer. A missing file is `Ok(None)`. A file that does not parse is an error that
/// names the file, so the user can fix it.
pub fn read_table(path: &Path) -> AppResult<Option<toml::Table>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    text.parse::<toml::Table>()
        .map(Some)
        .map_err(|e| AppError::Settings(format!("{}: {}", path.display(), e.message())))
}

pub fn merge_tables(base: &mut toml::Table, over: toml::Table) {
    for (key, value) in over {
        match (base.get_mut(&key), value) {
            (Some(toml::Value::Table(existing)), toml::Value::Table(incoming)) => {
                merge_tables(existing, incoming);
            }
            (_, value) => {
                base.insert(key, value);
            }
        }
    }
}

pub fn load_merged(layers: &[(Scope, PathBuf)]) -> AppResult<SettingsPayload> {
    let mut merged = toml::Table::new();
    let mut infos = Vec::with_capacity(layers.len());
    for (scope, path) in layers {
        let table = read_table(path)?;
        infos.push(LayerInfo {
            scope: *scope,
            path: path.display().to_string(),
            exists: table.is_some(),
        });
        if let Some(table) = table {
            merge_tables(&mut merged, table);
        }
    }
    let settings: Settings = toml::Value::Table(merged)
        .try_into()
        .map_err(|e: toml::de::Error| AppError::Settings(e.message().to_string()))?;
    Ok(SettingsPayload {
        settings,
        layers: infos,
    })
}

/// Starting point for a repository's shareable settings. Everything is commented out, so it
/// changes nothing until the user opts in.
pub const PROJECT_TEMPLATE: &str = r#"# Circles settings for this repository. Commit this file to share it with your team.

# Gitignored files copied into every new workspace (gitignore syntax).
# files_to_copy = [".env", ".env.local"]

[scripts]
# Runs in the Setup tab when a workspace is created.
# setup = "pnpm install"

# Runs when you press Run. Use $CIRCLES_PORT so workspaces never fight over a port.
# run = "pnpm dev --port $CIRCLES_PORT"

# Runs before a workspace is archived, to clean up things outside the repository.
# archive = "./scripts/cleanup.sh"

# "concurrent" (the default) or "nonconcurrent", which stops the other workspaces'
# run scripts of this project before starting one.
# run_mode = "concurrent"

# Several run scripts show up in the Run menu. The default one starts with the Run button.
# [scripts.run.web]
# command = "pnpm dev --port $CIRCLES_PORT"
# default = true
# icon = "server"
#
# [scripts.run.tests]
# command = "pnpm test --watch"
# icon = "flask-conical"

[environment_variables]
# API_URL = "http://localhost:3000"

# Extra guidance added to the prompts Circles sends the agent.
[instructions]
# general = "Follow the style guide in CONTRIBUTING.md."
# review = "Focus on security and error handling."
# pr = "Title in the imperative mood. Describe what changed and how to test it."
# fix_errors = "Run the full test suite before you commit a fix."
# conflicts = "Prefer rebasing over merging."
# commits = "Conventional commits: feat, fix, chore."

[git]
# How pull requests are merged: "squash" (the default), "merge" or "rebase".
# merge_method = "squash"
# How this project names its branches. {type} is feat, fix, chore and so on, {slug} lowercase
# words with dashes, {ticket} an issue key like PROJ-123. Composers follow it.
# branch_pattern = "{ticket}-{slug}"
"#;

/// Makes sure `<repo>/.circles/settings.toml` exists, creating it from the template. An existing
/// file is never touched.
pub fn ensure_project_settings(repo_root: &Path) -> AppResult<PathBuf> {
    let file = crate::paths::project_settings_file(repo_root);
    if !file.exists() {
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&file, PROJECT_TEMPLATE)?;
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_template_is_valid_and_changes_nothing() {
        let table: toml::Table = PROJECT_TEMPLATE.parse().unwrap();
        let settings: Settings = toml::Value::Table(table).try_into().unwrap();
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn instructions_and_the_merge_method_are_read_and_layered() {
        let user: toml::Table = "[instructions]\ngeneral = \"Be brief.\"\npr = \"User PR rule\"\n[git]\nmerge_method = \"merge\"\n"
            .parse()
            .unwrap();
        let mut merged = user;
        let project: toml::Table =
            "[instructions]\npr = \"Project PR rule\"\nconflicts = \"Rebase.\"\n"
                .parse()
                .unwrap();
        merge_tables(&mut merged, project);
        let settings: Settings = toml::Value::Table(merged).try_into().unwrap();
        assert_eq!(settings.instructions.general.as_deref(), Some("Be brief."));
        assert_eq!(settings.instructions.pr.as_deref(), Some("Project PR rule"));
        assert_eq!(settings.instructions.conflicts.as_deref(), Some("Rebase."));
        assert_eq!(settings.instructions.review, None);
        assert_eq!(settings.git.merge_method.as_deref(), Some("merge"));
    }

    #[test]
    fn uncommenting_the_template_gives_working_scripts() {
        let uncommented: String = PROJECT_TEMPLATE
            .lines()
            .map(|l| {
                l.strip_prefix("# ")
                    .or_else(|| l.strip_prefix("#"))
                    .unwrap_or(l)
            })
            .collect::<Vec<_>>()
            .join("\n");
        // The prose lines are not valid TOML, so check only the keys users are meant to enable.
        for snippet in [
            "files_to_copy = [\".env\", \".env.local\"]",
            "setup = \"pnpm install\"",
            "[scripts.run.web]",
            "run_mode = \"concurrent\"",
        ] {
            assert!(uncommented.contains(snippet), "{snippet}");
        }
    }

    #[test]
    fn creates_the_file_once_and_never_overwrites_it() {
        let tmp = tempfile::tempdir().unwrap();
        let file = ensure_project_settings(tmp.path()).unwrap();
        assert_eq!(file, tmp.path().join(".circles/settings.toml"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), PROJECT_TEMPLATE);

        std::fs::write(&file, "[scripts]\nsetup = \"mine\"\n").unwrap();
        ensure_project_settings(tmp.path()).unwrap();
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "[scripts]\nsetup = \"mine\"\n"
        );
    }

    fn write(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn higher_levels_win_per_key() {
        let tmp = tempfile::tempdir().unwrap();
        let user = write(
            tmp.path(),
            "user.toml",
            "[scripts]\nsetup = \"user setup\"\nrun_mode = \"concurrent\"\n[environment_variables]\nA = \"user\"\nB = \"user\"\n",
        );
        let project = write(
            tmp.path(),
            "project.toml",
            "[scripts]\nsetup = \"project setup\"\n[environment_variables]\nB = \"project\"\nC = \"project\"\n",
        );
        let local = write(
            tmp.path(),
            "local.toml",
            "[scripts]\nsetup = \"local setup\"\n[environment_variables]\nC = \"local\"\n",
        );

        let payload = load_merged(&[
            (Scope::User, user),
            (Scope::Project, project),
            (Scope::LocalProject, local),
        ])
        .unwrap();
        let s = payload.settings;

        assert_eq!(s.scripts.setup.as_deref(), Some("local setup"));
        assert_eq!(s.scripts.run_mode.as_deref(), Some("concurrent"));
        assert_eq!(s.environment_variables["A"], "user");
        assert_eq!(s.environment_variables["B"], "project");
        assert_eq!(s.environment_variables["C"], "local");
        assert!(payload.layers.iter().all(|l| l.exists));
    }

    #[test]
    fn project_beats_user_when_no_local_file() {
        let tmp = tempfile::tempdir().unwrap();
        let user = write(tmp.path(), "u.toml", "[scripts]\nsetup = \"user\"\n");
        let project = write(tmp.path(), "p.toml", "[scripts]\nsetup = \"project\"\n");
        let missing = tmp.path().join("nope.toml");

        let payload = load_merged(&[
            (Scope::User, user),
            (Scope::Project, project),
            (Scope::LocalProject, missing),
        ])
        .unwrap();

        assert_eq!(payload.settings.scripts.setup.as_deref(), Some("project"));
        assert!(!payload.layers[2].exists);
    }

    #[test]
    fn unknown_keys_are_preserved() {
        let tmp = tempfile::tempdir().unwrap();
        let user = write(
            tmp.path(),
            "u.toml",
            "future_flag = true\n[future_table]\nx = 1\n[scripts]\nsetup = \"s\"\n",
        );
        let payload = load_merged(&[(Scope::User, user)]).unwrap();
        assert_eq!(
            payload.settings.extra["future_flag"],
            toml::Value::Boolean(true)
        );
        assert!(payload.settings.extra["future_table"].is_table());
    }

    #[test]
    fn arrays_are_replaced_not_concatenated() {
        let mut base: toml::Table = "list = [1, 2]".parse().unwrap();
        let over: toml::Table = "list = [3]".parse().unwrap();
        merge_tables(&mut base, over);
        assert_eq!(base["list"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn invalid_toml_names_the_file() {
        let tmp = tempfile::tempdir().unwrap();
        let bad = write(tmp.path(), "bad.toml", "[scripts\nsetup = ");
        let err = load_merged(&[(Scope::User, bad)]).unwrap_err();
        assert_eq!(err.code(), "settings");
        assert!(err.to_string().contains("bad.toml"));
    }

    #[test]
    fn empty_layers_give_defaults() {
        let payload =
            load_merged(&[(Scope::User, PathBuf::from("/definitely/missing.toml"))]).unwrap();
        assert_eq!(payload.settings, Settings::default());
    }

    #[test]
    fn run_accepts_a_string_or_a_table() {
        let tmp = tempfile::tempdir().unwrap();
        let single = write(tmp.path(), "a.toml", "[scripts]\nrun = \"pnpm dev\"\n");
        let named = write(
            tmp.path(),
            "b.toml",
            "[scripts.run.dev]\ncommand = \"pnpm dev\"\ndefault = true\n",
        );
        assert!(load_merged(&[(Scope::User, single)])
            .unwrap()
            .settings
            .scripts
            .run
            .unwrap()
            .is_str());
        assert!(load_merged(&[(Scope::User, named)])
            .unwrap()
            .settings
            .scripts
            .run
            .unwrap()
            .is_table());
    }
}
