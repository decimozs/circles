//! Setup, run and archive scripts from `settings.toml`, and the environment they run in.

use serde::Serialize;
use ts_rs::TS;

use crate::error::{AppError, AppResult};
use crate::project::Project;
use crate::settings::{Scripts, Settings};
use crate::workspace::{names, Workspace};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum RunMode {
    /// Run scripts of different workspaces may run at the same time.
    Concurrent,
    /// Starting a run script stops the run scripts of the project's other workspaces first.
    /// For scripts that share a port or a database.
    Nonconcurrent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct RunScript {
    /// Name shown in the Run menu.
    pub name: String,
    pub command: String,
    pub default: bool,
    /// Lucide icon name in kebab-case, for example `play` or `server`.
    pub icon: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ScriptsInfo {
    pub setup: Option<String>,
    pub run: Vec<RunScript>,
    pub archive: Option<String>,
    pub run_mode: RunMode,
}

fn bad(path: &str, problem: &str) -> AppError {
    AppError::Settings(format!("scripts.{path}: {problem}"))
}

fn non_empty(command: &str) -> Option<String> {
    let command = command.trim();
    (!command.is_empty()).then(|| command.to_string())
}

/// `available_in` is a string or a list of `local` and `cloud`. Circles only runs locally.
fn available_locally(path: &str, value: Option<&toml::Value>) -> AppResult<bool> {
    let Some(value) = value else {
        return Ok(true);
    };
    let scopes: Vec<&str> = match value {
        toml::Value::String(s) => vec![s.as_str()],
        toml::Value::Array(items) => items
            .iter()
            .map(|v| {
                v.as_str()
                    .ok_or_else(|| bad(path, "available_in must hold strings"))
            })
            .collect::<AppResult<_>>()?,
        _ => {
            return Err(bad(
                path,
                "available_in must be \"local\", \"cloud\" or a list of them",
            ))
        }
    };
    for scope in &scopes {
        if !matches!(*scope, "local" | "cloud") {
            return Err(bad(
                path,
                &format!("unknown scope \"{scope}\" in available_in"),
            ));
        }
    }
    Ok(scopes.contains(&"local"))
}

/// Turns `scripts.run` into a list. Accepts a single command or a table of named scripts.
fn parse_run(value: &toml::Value) -> AppResult<Vec<RunScript>> {
    let mut scripts = Vec::new();
    match value {
        toml::Value::String(command) => {
            if let Some(command) = non_empty(command) {
                scripts.push(RunScript {
                    name: "Run".into(),
                    command,
                    default: true,
                    icon: None,
                });
            }
        }
        toml::Value::Table(table) => {
            for (name, entry) in table {
                let path = format!("run.{name}");
                let (command, default, icon, scope) = match entry {
                    toml::Value::String(command) => (command.as_str(), false, None, None),
                    toml::Value::Table(t) => {
                        let command = t
                            .get("command")
                            .and_then(toml::Value::as_str)
                            .ok_or_else(|| bad(&path, "command is required"))?;
                        let default = match t.get("default") {
                            None => false,
                            Some(v) => v
                                .as_bool()
                                .ok_or_else(|| bad(&path, "default must be true or false"))?,
                        };
                        let icon = match t.get("icon") {
                            None => None,
                            Some(v) => Some(
                                v.as_str()
                                    .filter(|s| {
                                        s.chars().all(|c| {
                                            c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'
                                        })
                                    })
                                    .ok_or_else(|| {
                                        bad(&path, "icon must be a lucide icon name in kebab-case")
                                    })?
                                    .to_string(),
                            ),
                        };
                        (command, default, icon, t.get("available_in"))
                    }
                    _ => return Err(bad(&path, "expected a command or a table")),
                };
                let Some(command) = non_empty(command) else {
                    return Err(bad(&path, "command is empty"));
                };
                if !available_locally(&path, scope)? {
                    continue;
                }
                scripts.push(RunScript {
                    name: name.clone(),
                    command,
                    default,
                    icon,
                });
            }
        }
        _ => return Err(bad("run", "expected a command or a table of named scripts")),
    }

    if scripts.iter().filter(|s| s.default).count() > 1 {
        return Err(bad("run", "only one script can be the default"));
    }
    if !scripts.is_empty() && !scripts.iter().any(|s| s.default) {
        scripts[0].default = true;
    }
    // The default script comes first so the Run button and ⌘R pick it.
    scripts.sort_by_key(|s| !s.default);
    Ok(scripts)
}

pub fn resolve(scripts: &Scripts) -> AppResult<ScriptsInfo> {
    let run_mode = match scripts.run_mode.as_deref() {
        None | Some("concurrent") => RunMode::Concurrent,
        Some("nonconcurrent") => RunMode::Nonconcurrent,
        Some(other) => {
            return Err(bad(
                "run_mode",
                &format!("\"{other}\" is not valid, use \"concurrent\" or \"nonconcurrent\""),
            ));
        }
    };
    Ok(ScriptsInfo {
        setup: scripts.setup.as_deref().and_then(non_empty),
        run: match &scripts.run {
            Some(value) => parse_run(value)?,
            None => Vec::new(),
        },
        archive: scripts.archive.as_deref().and_then(non_empty),
        run_mode,
    })
}

/// Environment for a script or terminal of `ws`. Custom `[environment_variables]` come last, so
/// they can override anything but are applied on top of the login environment.
pub fn workspace_env(
    ws: &Workspace,
    project: &Project,
    port: Option<u16>,
    settings: &Settings,
) -> Vec<(String, String)> {
    let mut vars = vec![
        (
            "CIRCLES_WORKSPACE_NAME".to_string(),
            names::city_slug(&ws.city),
        ),
        ("CIRCLES_WORKSPACE_PATH".to_string(), ws.path.clone()),
        ("CIRCLES_ROOT_PATH".to_string(), project.path.clone()),
        ("CIRCLES_DEFAULT_BRANCH".to_string(), ws.base_branch.clone()),
        ("CIRCLES_IS_LOCAL".to_string(), "1".to_string()),
    ];
    if let Some(port) = port {
        vars.push(("CIRCLES_PORT".to_string(), port.to_string()));
    }
    vars.extend(
        settings
            .environment_variables
            .iter()
            .map(|(k, v)| (k.clone(), v.clone())),
    );
    vars
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{load_merged, Scope};
    use crate::test_support::{project_fixture, workspace_fixture};

    fn scripts_from(toml: &str) -> AppResult<ScriptsInfo> {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("s.toml");
        std::fs::write(&file, toml).unwrap();
        let settings = load_merged(&[(Scope::User, file)])?.settings;
        resolve(&settings.scripts)
    }

    #[test]
    fn nothing_configured_is_fine() {
        let info = scripts_from("").unwrap();
        assert_eq!(info.setup, None);
        assert!(info.run.is_empty());
        assert_eq!(info.archive, None);
        assert_eq!(info.run_mode, RunMode::Concurrent);
    }

    #[test]
    fn simple_commands() {
        let info = scripts_from(
            "[scripts]\nsetup = \"pnpm install\"\nrun = \"pnpm dev --port $CIRCLES_PORT\"\narchive = \"./cleanup.sh\"\nrun_mode = \"nonconcurrent\"\n",
        )
        .unwrap();
        assert_eq!(info.setup.as_deref(), Some("pnpm install"));
        assert_eq!(info.archive.as_deref(), Some("./cleanup.sh"));
        assert_eq!(info.run_mode, RunMode::Nonconcurrent);
        assert_eq!(
            info.run,
            vec![RunScript {
                name: "Run".into(),
                command: "pnpm dev --port $CIRCLES_PORT".into(),
                default: true,
                icon: None,
            }]
        );
    }

    #[test]
    fn blank_commands_count_as_unset() {
        let info =
            scripts_from("[scripts]\nsetup = \"   \"\nrun = \"\"\narchive = \"\"\n").unwrap();
        assert_eq!(info.setup, None);
        assert!(info.run.is_empty());
        assert_eq!(info.archive, None);
    }

    #[test]
    fn named_run_scripts_keep_file_order_with_the_default_first() {
        let info = scripts_from(
            "[scripts.run.test]\ncommand = \"pnpm test --watch\"\nicon = \"flask-conical\"\n\n[scripts.run.web]\ncommand = \"pnpm dev\"\ndefault = true\nicon = \"server\"\n\n[scripts.run.docs]\ncommand = \"pnpm docs\"\n",
        )
        .unwrap();
        let names: Vec<_> = info.run.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["web", "test", "docs"]);
        assert!(info.run[0].default && !info.run[1].default);
        assert_eq!(info.run[0].icon.as_deref(), Some("server"));
        assert_eq!(info.run[1].icon.as_deref(), Some("flask-conical"));
    }

    #[test]
    fn the_first_script_is_the_default_when_none_is_marked() {
        let info = scripts_from(
            "[scripts.run.a]\ncommand = \"one\"\n[scripts.run.b]\ncommand = \"two\"\n",
        )
        .unwrap();
        assert!(info.run[0].default);
        assert_eq!(info.run[0].name, "a");
    }

    #[test]
    fn shorthand_entries_are_accepted() {
        let info =
            scripts_from("[scripts.run]\ndev = \"pnpm dev\"\ntest = \"pnpm test\"\n").unwrap();
        assert_eq!(info.run.len(), 2);
        assert_eq!(info.run[0].command, "pnpm dev");
    }

    #[test]
    fn available_in_limits_scripts_to_local() {
        let info = scripts_from(
            "[scripts.run.local_only]\ncommand = \"a\"\navailable_in = \"local\"\n[scripts.run.cloud_only]\ncommand = \"b\"\navailable_in = \"cloud\"\n[scripts.run.both]\ncommand = \"c\"\navailable_in = [\"local\", \"cloud\"]\n",
        )
        .unwrap();
        let names: Vec<_> = info.run.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["local_only", "both"]);
    }

    #[test]
    fn mistakes_are_reported_with_the_script_name() {
        let cases = [
            ("[scripts.run.dev]\ndefault = true\n", "run.dev: command is required"),
            ("[scripts.run.dev]\ncommand = \"\"\n", "run.dev: command is empty"),
            ("[scripts.run.dev]\ncommand = \"x\"\nicon = \"Not Kebab\"\n", "run.dev: icon must be"),
            ("[scripts.run.dev]\ncommand = \"x\"\navailable_in = \"moon\"\n", "unknown scope"),
            ("[scripts.run.dev]\ncommand = \"x\"\ndefault = \"yes\"\n", "default must be true or false"),
            (
                "[scripts.run.a]\ncommand = \"x\"\ndefault = true\n[scripts.run.b]\ncommand = \"y\"\ndefault = true\n",
                "only one script can be the default",
            ),
            ("[scripts]\nrun_mode = \"sometimes\"\n", "run_mode"),
            ("[scripts]\nrun = 5\n", "scripts.run"),
        ];
        for (toml, expected) in cases {
            let err = scripts_from(toml).unwrap_err();
            assert_eq!(err.code(), "settings", "{toml}");
            assert!(err.to_string().contains(expected), "{toml} -> {err}");
        }
    }

    #[test]
    fn workspace_environment_has_the_documented_variables() {
        let project = project_fixture("roster", "/code/roster");
        let ws = workspace_fixture("Algiers", "/home/circles/workspaces/roster/algiers", "main");
        let mut settings = Settings::default();
        settings
            .environment_variables
            .insert("API_URL".into(), "http://localhost".into());

        let vars: std::collections::BTreeMap<_, _> =
            workspace_env(&ws, &project, Some(55010), &settings)
                .into_iter()
                .collect();
        assert_eq!(vars["CIRCLES_WORKSPACE_NAME"], "algiers");
        assert_eq!(
            vars["CIRCLES_WORKSPACE_PATH"],
            "/home/circles/workspaces/roster/algiers"
        );
        assert_eq!(vars["CIRCLES_ROOT_PATH"], "/code/roster");
        assert_eq!(vars["CIRCLES_DEFAULT_BRANCH"], "main");
        assert_eq!(vars["CIRCLES_PORT"], "55010");
        assert_eq!(vars["CIRCLES_IS_LOCAL"], "1");
        assert_eq!(vars["API_URL"], "http://localhost");

        let no_port = workspace_env(&ws, &project, None, &settings);
        assert!(no_port.iter().all(|(k, _)| k != "CIRCLES_PORT"));
    }
}
