//! How a Composer's Claude Code is started: no built-in tools, only the Circles tools.

use std::path::Path;

use serde_json::json;

use super::prompt::system_prompt;
use super::ComposerHub;
use crate::harness::sessions::{LaunchExtras, LaunchHook};
use crate::harness::store::Session;

/// How long a tool may take to answer, since an approval can wait for a person.
const TOOL_TIMEOUT_MS: &str = "86400000";

/// The arguments for a Composer's Claude Code. `mcp_config` is the file with the tool server.
pub fn args(
    composer: &super::store::Composer,
    unattended: bool,
    cards: bool,
    mcp_config: Option<&Path>,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        // No file, shell or web tools. The only things it can do are Circles' tools.
        "--tools".into(),
        String::new(),
        "--append-system-prompt".into(),
        system_prompt(composer, unattended, cards),
        // The Composer works for the person's project, not by their personal instructions. Their
        // user level CLAUDE.md and settings are for their own chats. A Composer that read them
        // wrote briefs that followed rules meant for the person, such as a commit identity.
        "--setting-sources".into(),
        "project,local".into(),
    ];
    if let Some(config) = mcp_config {
        args.extend([
            "--strict-mcp-config".into(),
            "--mcp-config".into(),
            config.to_string_lossy().into_owned(),
            "--allowedTools".into(),
            "mcp__circles".into(),
        ]);
    }
    if let Some(budget) = composer.limits.max_budget_usd {
        args.extend(["--max-budget-usd".into(), format!("{budget}")]);
    }
    args
}

impl LaunchHook for ComposerHub {
    fn extras(&self, session: &Session) -> Option<LaunchExtras> {
        let Some(composer) = self.store.by_session(&session.id).ok().flatten() else {
            return self.chat_extras(session);
        };
        let unattended = self
            .store
            .run_info(&session.id)
            .ok()
            .flatten()
            .is_some_and(|run| run.started_by != "person");

        let mut config_path = None;
        let mut env = vec![
            ("CIRCLES_COMPOSER_ID".to_string(), composer.id.clone()),
            ("MCP_TOOL_TIMEOUT".to_string(), TOOL_TIMEOUT_MS.to_string()),
        ];
        // The helper is this same program. `CIRCLES_MCP_EXE` names another one, for tests and for
        // trying a build that is not the one running.
        let exe = std::env::var_os("CIRCLES_MCP_EXE")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::current_exe().ok());
        if let (Some(server), Some(exe)) = (self.server(), exe) {
            // The token is in a file only this user can read, not on the command line.
            let dir = self.data_dir().join("composers").join("run");
            let path = dir.join(format!("{}.mcp.json", session.id));
            let config = json!({
                "mcpServers": {
                    "circles": {
                        "command": exe,
                        "args": ["--circles-mcp"],
                        "env": {
                            "CIRCLES_SOCKET": server.socket,
                            "CIRCLES_TOKEN": server.token,
                            "CIRCLES_SESSION_ID": session.id,
                        },
                    }
                }
            });
            if write_private(&path, &config.to_string()).is_ok() {
                config_path = Some(path);
            } else {
                tracing::warn!("cannot write the tool server settings for a Composer run");
            }
        }
        env.shrink_to_fit();
        Some(LaunchExtras {
            args: args(
                &composer,
                unattended,
                self.genui.get().is_some_and(|g| g.composers_on()),
                config_path.as_deref(),
            ),
            env,
        })
    }
}

impl ComposerHub {
    /// What an ordinary chat gets when Jira is connected and chats may use it: the Jira tools as one
    /// more tool server next to the user's own. Reading is allowed up front. Changing Jira is not on
    /// the list, so Claude's own permission prompt asks, in the chat, like for any other change.
    fn chat_extras(&self, session: &Session) -> Option<LaunchExtras> {
        // What a chat may use: the Jira tools when they are on, and cards when they are on.
        let mut tools = Vec::new();
        if self.integrations.get().is_some_and(|i| i.jira_for_chats()) {
            tools.extend(crate::integrations::jira::tools::defs());
        }
        if self.genui.get().is_some_and(|g| g.chats_on()) {
            tools.push(crate::genui::tool_def());
        }
        if tools.is_empty() {
            return None;
        }
        let server = self.server()?;
        let exe = std::env::var_os("CIRCLES_MCP_EXE")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::current_exe().ok())?;
        let dir = self.data_dir().join("composers").join("run");
        let path = dir.join(format!("{}.mcp.json", session.id));
        let config = json!({
            "mcpServers": {
                "circles-jira": {
                    "command": exe,
                    "args": ["--circles-mcp"],
                    "env": {
                        "CIRCLES_SOCKET": server.socket,
                        "CIRCLES_TOKEN": server.token,
                        "CIRCLES_SESSION_ID": session.id,
                    },
                }
            }
        });
        write_private(&path, &config.to_string()).ok()?;
        Some(LaunchExtras {
            args: chat_args(&path, &tools),
            // A card waits up to ten minutes for an answer, so a tool call may take that long.
            env: vec![("MCP_TOOL_TIMEOUT".to_string(), "700000".to_string())],
        })
    }
}

/// The arguments that give a chat the Jira tools. Not strict, so the user's own tool servers still
/// load. `--allowedTools` takes a list, so it goes last. Only reading is allowed up front.
fn chat_args(config: &Path, tools: &[crate::tools::ToolDef]) -> Vec<String> {
    let reads: Vec<String> = tools
        .iter()
        .filter(|d| d.tier == crate::tools::Tier::Read)
        .map(|d| format!("mcp__circles-jira__{}", d.name))
        .collect();
    vec![
        "--mcp-config".into(),
        config.to_string_lossy().into_owned(),
        "--allowedTools".into(),
        reads.join(","),
    ]
}

fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composer::store::{ComposerInput, ComposerStore};
    use crate::db::Db;
    use std::sync::Arc;

    fn composer(budget: Option<f64>) -> crate::composer::store::Composer {
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::open(&dir.path().join("t.db")).unwrap());
        let store = ComposerStore::new(db, dir.path());
        let mut input = ComposerInput {
            name: "C".into(),
            ..Default::default()
        };
        input.limits.max_budget_usd = budget;
        let c = store.create(&input).unwrap();
        std::mem::forget(dir);
        c
    }

    #[test]
    fn built_in_tools_are_switched_off_with_an_empty_list_and_only_the_circles_server_is_used() {
        let a = args(&composer(None), false, false, Some(Path::new("/x/y.json")));
        let at = a.iter().position(|x| x == "--tools").unwrap();
        assert_eq!(a[at + 1], "", "the empty list is kept");
        assert!(a.contains(&"--strict-mcp-config".to_string()));
        let cfg = a.iter().position(|x| x == "--mcp-config").unwrap();
        assert_eq!(a[cfg + 1], "/x/y.json");
        assert!(a.contains(&"mcp__circles".to_string()));
        assert!(!a.contains(&"--max-budget-usd".to_string()));
        assert!(a.iter().any(|x| x.contains("You are \"C\"")));
    }

    #[test]
    fn the_persons_user_level_instructions_are_not_loaded() {
        let a = args(&composer(None), false, false, None);
        let at = a.iter().position(|x| x == "--setting-sources").unwrap();
        assert_eq!(a[at + 1], "project,local");
    }

    #[test]
    fn the_spending_limit_goes_to_claude_code() {
        let a = args(&composer(Some(2.5)), false, false, None);
        let i = a.iter().position(|x| x == "--max-budget-usd").unwrap();
        assert_eq!(a[i + 1], "2.5");
        // Without the tool server there is no server flag at all, and still no built-in tools.
        assert!(!a.contains(&"--mcp-config".to_string()));
        assert!(a.contains(&"--tools".to_string()));
    }

    #[test]
    fn the_settings_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a/b.json");
        write_private(&path, "{}").unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        write_private(&path, "{\"x\":1}").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{\"x\":1}");
    }

    /// A real Composer run, start to finish, through the real session manager, the launch hook,
    /// the tool socket and real Claude Code, with a fake app behind the tools. It checks what
    /// the unit tests cannot: that Claude Code takes the arguments the app builds, uses the tools,
    /// and waits for an approval that a person gives. Costs a few cents. Build the app first.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "uses the real Claude Code and a little API credit"]
    async fn a_real_run_uses_the_tools_and_waits_for_an_approval() {
        use crate::composer::dispatch::{ComposerEvents, Dispatcher, ToolRuntime};
        use crate::harness::claude::ClaudeCode;
        use crate::harness::sessions::{NoSync, NullEvents, SessionManager};
        use crate::harness::store::{NewSession, PermissionMode, SessionStatus, SessionStore};
        use crate::logging::Redactor;
        use crate::terminal::Context;
        use crate::tools::audit::AuditStore;
        use async_trait::async_trait;
        use serde_json::Value;
        use std::time::{Duration, Instant};

        struct Fake;
        #[async_trait]
        impl ToolRuntime for Fake {
            async fn project_of(&self, _t: &str, _i: &Value) -> Option<String> {
                None
            }
            async fn run(
                &self,
                _c: &crate::composer::store::Composer,
                _s: &str,
                tool: &str,
                input: Value,
            ) -> crate::error::AppResult<Value> {
                Ok(match tool {
                    "list_projects" => {
                        json!({"projects": [{"project_id": "p1", "name": "roster"}]})
                    }
                    "create_workspace" => {
                        json!({"workspace_id": "w-new", "name": "Oslo", "input": input})
                    }
                    _ => json!({"ok": true}),
                })
            }
            async fn chat_active(&self, _c: &str) -> bool {
                false
            }
        }
        struct Quiet;
        impl ComposerEvents for Quiet {
            fn approvals_changed(&self, _c: &str, _s: &str) {}
        }

        let exe = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find(|p| p.join("Cargo.lock").is_file())
            .expect("a workspace with a Cargo.lock")
            .join("target/debug/circles");
        assert!(exe.is_file(), "run `cargo build` first: {}", exe.display());
        std::env::set_var("CIRCLES_MCP_EXE", &exe);
        let tmp = tempfile::tempdir().unwrap();
        let dir = std::fs::canonicalize(tmp.path()).unwrap();
        let db = Arc::new(Db::open(&dir.join("t.db")).unwrap());
        let hub = crate::composer::ComposerHub::new(db.clone(), &dir);
        let dispatcher = Arc::new(Dispatcher::new(
            hub.store.clone(),
            AuditStore::new(db.clone()),
            Arc::new(Fake),
            Arc::new(Quiet),
            Redactor::default(),
        ));
        let server =
            crate::composer::server::start(dispatcher.clone(), &dir.join("composers/run")).unwrap();
        let _ = hub.started.set(crate::composer::Started {
            dispatcher: dispatcher.clone(),
            server,
        });

        let composer = hub
            .store
            .create(&ComposerInput {
                name: "Release captain".into(),
                instructions: "Use the tools you are given. Be brief.".into(),
                ..Default::default()
            })
            .unwrap();
        let env = Arc::new(crate::env::resolve().await);
        let manager = SessionManager::new(
            SessionStore::new(db.clone()),
            Arc::new(ClaudeCode::new(env, Some(db.clone()))),
            Arc::new(NullEvents),
            Arc::new(NoSync),
        );
        manager.set_launch_hook(hub.clone());
        let ctx = Context {
            workspace_id: composer.workspace_id.clone(),
            project_id: crate::composer::store::HOME_PROJECT_ID.into(),
            cwd: dir.join("composers").join(&composer.id),
            env: vec![],
        };
        let session = manager
            .create(
                &composer.workspace_id,
                NewSession {
                    model: Some("haiku".into()),
                    effort: Some("low".into()),
                    permission_mode: Some(PermissionMode::Manual),
                    ..Default::default()
                },
            )
            .unwrap();
        hub.store
            .record_run(&session.id, &composer.id, "person", None)
            .unwrap();

        let wait_idle = |after_turns: i64| {
            let manager = manager.clone();
            let id = session.id.clone();
            async move {
                let deadline = Instant::now() + Duration::from_secs(150);
                loop {
                    let s = manager.store().get(&id).unwrap();
                    if s.status == SessionStatus::Idle && s.turns >= after_turns {
                        return;
                    }
                    assert!(Instant::now() < deadline, "timed out: {:?}", s.status);
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
            }
        };

        // A read tool goes straight through.
        manager
            .send(
                &ctx,
                &session.id,
                "Call the list_projects tool once, then reply with just the project name.",
            )
            .await
            .unwrap();
        wait_idle(1).await;
        let calls = dispatcher
            .audit
            .list(None, Some(&session.id), None, 20)
            .unwrap();
        println!("{calls:?}");
        if calls.is_empty() {
            let s = manager.store().get(&session.id).unwrap();
            println!("session: {s:?}");
            for m in manager.messages(&session.id, None).unwrap() {
                println!("message: {:?} {:?} {}", m.role, m.kind, m.content);
            }
        }
        assert!(calls
            .iter()
            .any(|c| c.tool == "list_projects" && c.outcome == "ok"));
        let said = manager
            .messages(&session.id, None)
            .unwrap()
            .iter()
            .filter(|m| m.role == crate::harness::parser::Role::Assistant)
            .map(|m| m.content.to_string().to_lowercase())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            said.contains("roster"),
            "the answer did not use the result: {said}"
        );

        // A write tool waits for a person, and goes ahead once they say yes.
        manager
            .send(
                &ctx,
                &session.id,
                "Now call create_workspace for project p1, then reply with the workspace id.",
            )
            .await
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(120);
        let pending = loop {
            if let Some(a) = hub
                .store
                .approvals(None, Some(&session.id), true, 5)
                .unwrap()
                .into_iter()
                .next()
            {
                break a;
            }
            assert!(Instant::now() < deadline, "no approval was asked for");
            tokio::time::sleep(Duration::from_millis(200)).await;
        };
        assert_eq!(pending.tool, "create_workspace");
        assert!(
            !dispatcher
                .audit
                .list(None, Some(&session.id), None, 20)
                .unwrap()
                .iter()
                .any(|c| c.tool == "create_workspace" && c.outcome == "ok"),
            "it ran before the answer"
        );
        dispatcher.answer(&pending.id, true, false).unwrap();
        wait_idle(2).await;
        let calls = dispatcher
            .audit
            .list(None, Some(&session.id), None, 20)
            .unwrap();
        assert!(
            calls
                .iter()
                .any(|c| c.tool == "create_workspace" && c.outcome == "ok"),
            "{calls:?}"
        );
        assert_eq!(hub.store.children_of_run(&session.id).unwrap().len(), 1);
    }

    #[test]
    fn a_chat_gets_jira_next_to_its_own_tools_and_may_only_read_unasked() {
        let tools = crate::integrations::jira::tools::defs();
        let args = chat_args(Path::new("/x/c.json"), &tools);
        assert_eq!(args[0], "--mcp-config");
        assert!(!args
            .iter()
            .any(|a| a == "--strict-mcp-config" || a == "--tools"));
        assert_eq!(args[args.len() - 2], "--allowedTools");
        let allowed = args.last().unwrap();
        assert!(allowed.contains("mcp__circles-jira__jira_search"));
        assert!(allowed.contains("mcp__circles-jira__jira_get_issue"));
        for write in [
            "jira_add_comment",
            "jira_transition_issue",
            "jira_create_issue",
            "jira_edit_issue",
            "jira_assign_issue",
        ] {
            assert!(!allowed.contains(write), "{write}");
        }
    }
}
