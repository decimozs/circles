use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tracing_appender::non_blocking::WorkerGuard;

use crate::checkpoint::Checkpoints;
use crate::composer::ComposerHub;
use crate::db::Db;
use crate::env::EnvResolver;
use crate::git::Git;
use crate::github::Gh;
use crate::harness::claude::ClaudeCode;
use crate::harness::sessions::{AgentEvents, SessionManager, WorkspaceSync};
use crate::harness::store::SessionStore;
use crate::integrations::secrets::KeychainSecrets;
use crate::integrations::Integrations;
use crate::logging::Redactor;
use crate::paths::AppPaths;
use crate::pr::PrService;
use crate::project::ProjectService;
use crate::review::comments::CommentStore;
use crate::review::watcher::Watchers;
use crate::settings::watch::SettingsWatcher;
use crate::terminal::{TerminalNotifier, Terminals};
use crate::workspace::{EventSink, Locks, WorkspaceService};

/// The ways the backend tells the UI, and the other parts of the app, that something changed.
pub struct Outputs {
    pub workspace_events: Arc<dyn EventSink>,
    pub terminal_events: Arc<dyn TerminalNotifier>,
    pub agent_events: Arc<dyn AgentEvents>,
    pub workspace_sync: Arc<dyn WorkspaceSync>,
}

/// Shared application state, managed by Tauri and available to every command.
pub struct AppState {
    pub paths: AppPaths,
    pub env: EnvResolver,
    pub db: Arc<Db>,
    pub redactor: Redactor,
    pub sink: Arc<dyn EventSink>,
    pub locks: Arc<Locks>,
    pub terminals: Arc<Terminals>,
    /// Chats with Claude Code, one process per chat.
    pub sessions: Arc<SessionManager>,
    /// Composers: manager agents that use Circles' tools.
    pub composers: Arc<ComposerHub>,
    /// Cards agents show to ask for choices and details.
    pub genui: Arc<crate::genui::service::GenuiService>,
    /// Where card events go, set once the app has a window.
    pub genui_events: Arc<crate::genui::LateEvents>,
    /// Side questions that are running.
    pub side: crate::side::service::SideService,
    /// The small model that names chats.
    pub smart: crate::smart::service::SmartService,
    /// Jira and other outside services.
    pub integrations: Arc<Integrations>,
    /// Comments on lines of workspace files.
    pub comments: CommentStore,
    /// Tell the UI when files of an open workspace change.
    pub watchers: Watchers,
    /// Tells the UI when a settings file changes.
    pub settings_watcher: SettingsWatcher,
    /// Set once the user confirmed quitting while processes were still running.
    pub quit_confirmed: AtomicBool,
    /// Flushes buffered log lines on exit.
    _log_guard: WorkerGuard,
}

impl AppState {
    pub fn new(
        paths: AppPaths,
        env: EnvResolver,
        db: Arc<Db>,
        redactor: Redactor,
        outputs: Outputs,
        log_guard: WorkerGuard,
    ) -> Self {
        let Outputs {
            workspace_events: sink,
            terminal_events: notifier,
            agent_events,
            workspace_sync,
        } = outputs;
        let terminals = Terminals::new(db.clone(), paths.clone(), notifier);
        terminals.set_redactor(redactor.clone());
        let sessions = SessionManager::new(
            SessionStore::new(db.clone()),
            Arc::new(ClaudeCode::lazy(env.clone(), Some(db.clone()))),
            agent_events,
            workspace_sync,
        );
        sessions.set_turn_hook(Arc::new(CheckpointHook {
            env: env.clone(),
            db: db.clone(),
        }));
        let composers = ComposerHub::new(db.clone(), &paths.data_dir);
        sessions.set_launch_hook(composers.clone());
        let integrations_file = paths.data_dir.join("integrations.json");
        let genui_events = Arc::new(crate::genui::LateEvents::default());
        let genui = crate::genui::service::GenuiService::new(
            crate::genui::store::CardStore::new(db.clone()),
            genui_events.clone(),
        );
        let comments = CommentStore::new(db.clone());
        Self {
            paths,
            env,
            db,
            redactor,
            sink,
            locks: Arc::new(Locks::default()),
            terminals,
            sessions,
            composers,
            genui,
            genui_events,
            side: crate::side::service::SideService::default(),
            smart: crate::smart::service::SmartService::default(),
            integrations: Integrations::new(Box::new(KeychainSecrets::new()), integrations_file),
            comments,
            watchers: Watchers::default(),
            settings_watcher: SettingsWatcher::default(),
            quit_confirmed: AtomicBool::new(false),
            _log_guard: log_guard,
        }
    }
}

impl AppState {
    /// Terminals, scripts and agents that are busy right now, for the quit confirmation.
    pub fn running_work(&self) -> usize {
        self.terminals.running_count() + self.sessions.active_count()
    }

    /// Ends everything that was started. Used when the app quits.
    pub fn kill_everything(&self) {
        self.sessions.kill_all();
        self.terminals.kill_all();
        self.watchers.clear();
        self.settings_watcher.stop();
    }
}

impl AppState {
    /// Builds a project service on the resolved login environment.
    pub async fn project_service(&self) -> ProjectService {
        let env = self.env.get().await;
        ProjectService {
            db: self.db.clone(),
            git: Git::new(env.clone()),
            gh: Gh::new(env),
            paths: self.paths.clone(),
        }
    }
}

impl AppState {
    /// Checkpoints and reverting, on the resolved login environment.
    pub async fn checkpoints(&self) -> Checkpoints {
        Checkpoints::new(Git::new(self.env.get().await), self.db.clone())
    }
}

/// Saves the state before each turn. The environment is only resolved when the first turn starts.
pub struct CheckpointHook {
    pub env: EnvResolver,
    pub db: Arc<Db>,
}

#[async_trait::async_trait]
impl crate::harness::sessions::TurnHook for CheckpointHook {
    async fn before_turn(
        &self,
        workspace_id: &str,
        session_id: &str,
        claude_session_id: Option<&str>,
        cwd: &std::path::Path,
    ) -> Option<String> {
        let service = Checkpoints::new(Git::new(self.env.get().await), self.db.clone());
        crate::harness::sessions::TurnHook::before_turn(
            &service,
            workspace_id,
            session_id,
            claude_session_id,
            cwd,
        )
        .await
    }

    fn after_message(&self, handle: &str, message_seq: i64) {
        // Only the database is needed to link the message.
        let rows = self.db.write(|c| {
            c.execute(
                "UPDATE checkpoints SET message_seq = ?1 WHERE id = ?2",
                rusqlite::params![message_seq, handle],
            )
        });
        if let Err(e) = rows {
            tracing::warn!(error = %e, "cannot link the checkpoint to its message");
        }
    }
}

impl AppState {
    /// Pull requests, checks and merging, on the resolved login environment.
    pub async fn pr_service(&self) -> PrService {
        let env = self.env.get().await;
        PrService {
            git: Git::new(env.clone()),
            gh: Gh::new(env),
            db: self.db.clone(),
        }
    }
}

impl AppState {
    pub async fn workspace_service(&self) -> WorkspaceService {
        WorkspaceService {
            projects: self.project_service().await,
            sink: self.sink.clone(),
            locks: self.locks.clone(),
            terminals: self.terminals.clone(),
            env: self.env.get().await,
        }
    }
}
