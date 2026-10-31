// Phase 3 builds the shared foundation. Most of it is first used in phases 4-8, so dead-code
// warnings are silenced until then. Remove this once the APIs are wired up.
#![allow(dead_code)]

mod chat;
mod checkpoint;
mod commands;
mod composer;
mod db;
mod env;
mod error;
mod files;
mod genui;
mod git;
mod github;
mod harness;
mod integrations;
mod ipc;
mod logging;
mod machine;
mod paths;
mod ports;
mod pr;
mod process;
mod project;
mod pty;
mod review;
mod scripts;
mod settings;
mod side;
mod smart;
mod state;
mod system;
mod terminal;
#[cfg(test)]
mod test_support;
mod tools;
mod usage;
mod workspace;

use std::sync::Arc;

use std::sync::atomic::Ordering;

use tauri::{Emitter, Manager, RunEvent, WindowEvent};

use crate::db::Db;
use crate::env::EnvResolver;
use crate::logging::Redactor;
use crate::paths::AppPaths;
use crate::state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Claude Code starts Circles again as the tool server of a Composer. That is all it does then.
    if std::env::args().any(|a| a == "--circles-mcp") {
        std::process::exit(composer::mcp::run_stdio());
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None::<Vec<&'static str>>,
        ))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .on_window_event(|window, event| {
            // Closing the window with processes still running asks first.
            if let WindowEvent::CloseRequested { api, .. } = event {
                let state = window.app_handle().state::<AppState>();
                // With the menu bar item on, closing the window only hides it, so schedules and
                // triggers go on. The item's Quit ends the app.
                if composer::tray::is_active() && !state.quit_confirmed.load(Ordering::SeqCst) {
                    api.prevent_close();
                    let _ = window.hide();
                    return;
                }
                if !state.quit_confirmed.load(Ordering::SeqCst) && state.running_work() > 0 {
                    api.prevent_close();
                    let _ = window.emit("app:confirm-quit", state.running_work());
                }
            }
        })
        .setup(|app| {
            let paths = AppPaths::resolve()?;
            paths.ensure()?;

            let redactor = Redactor::default();
            let log_guard = logging::init(
                &paths.logs_dir,
                redactor.clone(),
                &logging::level_from_settings(&paths.settings_file),
            )?;
            tracing::info!(version = env!("CARGO_PKG_VERSION"), "circles starting");

            let db = Arc::new(Db::open(&paths.db_file)?);
            let report = process::reap_orphans(&db, &process::SystemOps)?;
            if report.killed > 0 || report.stale > 0 {
                tracing::info!(
                    killed = report.killed,
                    stale = report.stale,
                    "reaped process records"
                );
            }

            // Resolve the login-shell environment in the background. Commands that need it
            // await the same cached result.
            let env = EnvResolver::default();
            let sink: Arc<dyn workspace::EventSink> =
                Arc::new(workspace::TauriSink(app.handle().clone()));
            let notifier: Arc<dyn terminal::TerminalNotifier> =
                Arc::new(terminal::tauri::TauriNotifier(app.handle().clone()));
            let agent_events: Arc<dyn harness::sessions::AgentEvents> =
                Arc::new(harness::tauri::TauriAgentEvents(app.handle().clone()));
            let workspace_sync: Arc<dyn harness::sessions::WorkspaceSync> =
                Arc::new(harness::tauri::TauriWorkspaceSync(app.handle().clone()));
            app.manage(AppState::new(
                paths,
                env,
                db,
                redactor,
                state::Outputs {
                    workspace_events: sink,
                    terminal_events: notifier,
                    agent_events,
                    workspace_sync,
                },
                log_guard,
            ));
            app.state::<AppState>().terminals.start_persisting();
            {
                let state = app.state::<AppState>();
                // Notifications come from Circles itself when they can.
                system::set_app(app.handle().clone());
                // Cards agents show: events reach the window, and what waited before is over.
                state
                    .genui_events
                    .connect(std::sync::Arc::new(composer::TauriCardEvents(
                        app.handle().clone(),
                    )));
                state.genui.expire_after_restart();
                if let Err(e) = state.composers.start(
                    app.handle(),
                    state.redactor.clone(),
                    state.integrations.clone(),
                    state.genui.clone(),
                ) {
                    tracing::error!(error = %e, "Composers could not start");
                }
                let wanted = commands::app::settings_layers(&state, None)
                    .ok()
                    .and_then(|layers| settings::load_merged(&layers).ok())
                    .map(|payload| payload.settings)
                    .unwrap_or_default();
                state
                    .integrations
                    .set_chats(wanted.integrations.jira_in_chats.unwrap_or(true));
                state
                    .genui
                    .set_composers(wanted.genui.composers.unwrap_or(true));
                state.genui.set_chats(wanted.genui.chats.unwrap_or(false));
                let wanted = wanted.composers;
                if wanted.menu_bar == Some(true) {
                    if let Err(e) = composer::tray::build(app.handle()) {
                        tracing::warn!(error = %e, "cannot make the menu bar item");
                    }
                }
                commands::settings::apply_login_item(app.handle(), wanted.start_at_login);
                composer::triggers::Scheduler::spawn(app.handle().clone());
            }
            // The usage history is brought up to date a little after the app is up, so the Usage
            // page is current by the time it is opened.
            let usage = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                let state = usage.state::<AppState>();
                commands::usage::refresh(&usage, &state).await;
            });
            // Old checkpoints are cleared in the background, a little after the app is up.
            let cleaner = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(20)).await;
                let state = cleaner.state::<AppState>();
                let days = checkpoint::keep_days_from_settings(&state.paths.settings_file);
                match state.checkpoints().await.prune_older_than(days).await {
                    Ok(0) => {}
                    Ok(n) => tracing::info!(checkpoints = n, "removed old checkpoints"),
                    Err(e) => tracing::warn!(error = %e, "cannot remove old checkpoints"),
                }
            });
            match app.state::<AppState>().sessions.recover_after_restart() {
                Ok(0) => {}
                Ok(n) => tracing::info!(chats = n, "chats interrupted by the last shutdown"),
                Err(e) => tracing::warn!(error = %e, "cannot recover chats after a restart"),
            }

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let state = handle.state::<AppState>();
                let resolved = state.env.get().await;
                state.redactor.set_values(resolved.secret_values());
                tracing::info!(
                    source = ?resolved.source(),
                    vars = resolved.var_count(),
                    "environment resolved"
                );
                for tool in env::TOOLS {
                    match resolved.which(tool) {
                        Some(path) => tracing::info!(tool, path = %path.display(), "tool found"),
                        None => tracing::warn!(tool, "tool not found on PATH"),
                    }
                }

                match state.workspace_service().await.reconcile().await {
                    Ok(report) => tracing::info!(
                        errored = report.errored,
                        recovered = report.recovered,
                        reset = report.reset,
                        "workspaces reconciled"
                    ),
                    Err(e) => tracing::warn!(error = %e, "workspace reconcile failed"),
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app::get_app_info,
            commands::app::settings_get,
            commands::project::project_list,
            commands::project::project_add_local,
            commands::project::project_add_github,
            commands::project::project_quick_start,
            commands::project::project_remove,
            commands::project::project_set_base_branch,
            commands::project::github_list_repos,
            commands::project::project_learn_conventions,
            commands::session::session_create,
            commands::session::session_list,
            commands::session::session_get,
            commands::session::session_rename,
            commands::session::session_messages,
            commands::session::session_events,
            commands::session::session_attach,
            commands::session::session_detach,
            commands::session::session_send,
            commands::session::session_resume,
            commands::session::session_interrupt,
            commands::session::session_stop,
            commands::session::session_close,
            commands::session::session_update,
            commands::session::session_reopen,
            commands::session::session_reorder,
            commands::chat::workspace_search_files,
            commands::chat::slash_commands,
            commands::chat::attachment_save,
            commands::files::fs_list,
            commands::files::fs_read,
            commands::files::fs_write,
            commands::files::fs_reveal,
            commands::files::changes_list,
            commands::files::diff_files,
            commands::files::git_graph,
            commands::files::files_watch,
            commands::files::files_unwatch,
            commands::files::editor_list,
            commands::files::editor_open,
            commands::files::comment_list,
            commands::files::comment_create,
            commands::files::comment_set_status,
            commands::files::comment_delete,
            commands::files::comment_send,
            commands::checkpoint::checkpoint_list,
            commands::checkpoint::checkpoint_preview,
            commands::checkpoint::checkpoint_revert,
            commands::checkpoint::checkpoint_last_revert,
            commands::checkpoint::checkpoint_undo_revert,
            commands::checkpoint::session_branch,
            commands::settings::settings_view,
            commands::settings::settings_set,
            commands::settings::settings_watch,
            commands::settings::settings_unwatch,
            commands::settings::settings_open_file,
            commands::settings::git_identity,
            commands::settings::claude_check,
            commands::settings::sound_play,
            commands::settings::notify_test,
            commands::settings::data_reveal,
            commands::settings::logs_export,
            commands::settings::app_reset_data,
            commands::diagnostics::dependency_check,
            commands::diagnostics::diagnostics_report,
            commands::pr::pr_readiness,
            commands::pr::pr_status,
            commands::pr::pr_checks,
            commands::pr::pr_threads,
            commands::pr::pr_thread_resolve,
            commands::pr::pr_deployments,
            commands::pr::pr_draft_seed,
            commands::pr::pr_draft,
            commands::pr::commit_with_claude,
            commands::pr::commit_everything,
            commands::pr::pr_create,
            commands::pr::pr_merge,
            commands::pr::agent_fix_checks,
            commands::pr::agent_respond_review,
            commands::pr::agent_resolve_conflicts,
            commands::composer::tool_catalog,
            commands::account::claude_account,
            commands::account::github_accounts,
            commands::account::github_switch_account,
            commands::composer::composer_improve_instructions,
            commands::composer::composer_list,
            commands::composer::composer_get,
            commands::composer::composer_create,
            commands::composer::composer_update,
            commands::composer::composer_pin,
            commands::composer::composer_set_mode,
            commands::composer::composer_skills,
            commands::setup::setup_status,
            commands::setup::setup_action,
            commands::genui::genui_answer,
            commands::genui::genui_dismiss,
            commands::genui::genui_cards,
            commands::genui::genui_pending,
            commands::side::session_side_question,
            commands::side::session_side_stop,
            commands::smart::session_auto_title,
            commands::smart::smart_suggest,
            commands::smart::smart_first_prompt,
            commands::smart::smart_status,
            commands::smart::smart_test,
            commands::integrations::jira_status,
            commands::integrations::jira_connect,
            commands::integrations::jira_disconnect,
            commands::integrations::jira_sign_in,
            commands::integrations::jira_cancel_sign_in,
            commands::integrations::jira_choose_site,
            commands::composer::composer_set_notes,
            commands::composer::composer_delete,
            commands::composer::composer_runs,
            commands::composer::composer_run_start,
            commands::composer::composer_approvals,
            commands::composer::approval_answer,
            commands::composer::composer_fleet,
            commands::composer::composer_stop_child,
            commands::composer::audit_list,
            commands::composer::audit_export,
            commands::composer::composers_status,
            commands::composer::composers_pause,
            commands::composer::composers_stop_all,
            commands::composer::composer_workspace_links,
            commands::composer::composer_shared_list,
            commands::composer::composer_export,
            commands::composer::composer_shared_input,
            commands::composer::trigger_list,
            commands::composer::trigger_create,
            commands::composer::trigger_update,
            commands::composer::trigger_delete,
            commands::session::permission_reply,
            commands::session::claude_rate_limit,
            commands::usage::usage_summary,
            commands::usage::usage_refresh,
            commands::usage::usage_rebuild,
            commands::pr_browse::pr_browse_list,
            commands::pr_browse::pr_browse_detail,
            commands::pr_browse::pr_browse_files,
            commands::pr_browse::pr_browse_file,
            commands::workspace::workspace_take_pending_prompt,
            commands::terminal::terminal_list,
            commands::terminal::terminal_create,
            commands::terminal::terminal_rename,
            commands::terminal::terminal_delete,
            commands::terminal::terminal_attach,
            commands::terminal::terminal_detach,
            commands::terminal::terminal_write,
            commands::terminal::terminal_resize,
            commands::terminal::terminal_stop,
            commands::terminal::terminal_restart,
            commands::terminal::scripts_get,
            commands::terminal::script_run,
            commands::terminal::script_stop,
            commands::terminal::script_run_setup,
            commands::terminal::terminals_running,
            commands::terminal::app_quit,
            commands::terminal::project_open_settings,
            commands::workspace::project_branches,
            commands::workspace::github_list_prs,
            commands::workspace::workspace_list,
            commands::workspace::workspace_branch_states,
            commands::workspace::workspace_get,
            commands::workspace::workspace_events,
            commands::workspace::workspace_create,
            commands::workspace::workspace_archive,
            commands::workspace::workspace_restore,
            commands::workspace::workspace_rename_branch,
            commands::workspace::workspace_pin,
            commands::workspace::workspace_set_title,
            commands::workspace::workspace_delete,
            commands::workspace::workspace_open,
            commands::workspace::workspace_reconcile,
            commands::workspace::section_list,
            commands::workspace::section_create,
            commands::workspace::section_rename,
            commands::workspace::section_delete,
            commands::workspace::section_reorder,
            commands::workspace::workspace_set_section,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| match event {
            // Quitting from the menu or with Cmd+Q asks first when something is running.
            RunEvent::ExitRequested {
                api, code: None, ..
            } => {
                let state = app.state::<AppState>();
                if !state.quit_confirmed.load(Ordering::SeqCst) && state.running_work() > 0 {
                    api.prevent_exit();
                    let _ = app.emit("app:confirm-quit", state.running_work());
                }
            }
            // Clicking the Dock icon brings back a window that closing only hid.
            RunEvent::Reopen { .. } => composer::tray::show_window(app),
            // Whatever way the app ends, nothing it started is left running.
            RunEvent::Exit => app.state::<AppState>().kill_everything(),
            _ => {}
        });
}
